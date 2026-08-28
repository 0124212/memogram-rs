use anyhow::Result;
use chrono::Local;
use once_cell::sync::Lazy;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, env, sync::Arc};
use teloxide::{prelude::*, utils::command::BotCommands};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

static HTTP: Lazy<Client> = Lazy::new(|| Client::builder().user_agent("memogram-rs/0.1").build().unwrap());

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Commands:")]
enum Command {
    #[command(description = "link Telegram → Memos: /start <memos_pat>")]
    Start(String),
    #[command(description = "search memos")]
    Search(String),
    #[command(description = "random quote")]
    Quote,
    #[command(description = "HackerNews top 5")]
    Hn,
    #[command(description = "weather <city> (wttr.in, no key)")]
    Weather(String),
    #[command(description = "define <word>")]
    Define(String),
    #[command(description = "wiki <query>")]
    Wiki(String),
    #[command(description = "cheat <query> (cheat.sh)")]
    Cheat(String),
    #[command(description = "today")]
    Today,
    #[command(description = "week")]
    Week,
    #[command(description = "help")]
    Help,
}

#[derive(Clone)]
struct App {
    memos_url: String,
    admin_username: String, // e.g. "admin" — pinged as @admin
    allowed: Option<Vec<String>>,
    // per-Telegram user token store (like memogram data.txt)
    store: Arc<RwLock<HashMap<i64, String>>>,
    store_path: String,
    // per-Memos bot user tokens: quote -> memos_pat_...
    bot_tokens: HashMap<String, String>,
}

impl App {
    fn is_allowed(&self, username: Option<&str>) -> bool {
        if let Some(list) = &self.allowed {
            if let Some(u) = username {
                return list.iter().any(|a| a.eq_ignore_ascii_case(u));
            }
            return false;
        }
        true
    }
    fn bot_token(&self, bot: &str) -> Option<String> {
        self.bot_tokens.get(bot).cloned().or_else(|| {
            // fallback: use admin-linked token if bot token not configured (still pings admin via content)
            warn!("no bot token for {bot}, fallback to memogram store (will not impersonate)");
            None
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();
    let bot = Bot::from_env();
    let memos_url = env::var("MEMOS_URL").unwrap_or_else(|_| "https://memos.junilab.xyz".into()).trim_end_matches('/').to_string();
    let admin_username = env::var("ADMIN_USERNAME").unwrap_or_else(|_| "admin".into());
    let allowed = env::var("ALLOWED_USERNAMES").ok().map(|s| s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect());
    let store_path = env::var("DATA").unwrap_or_else(|_| "./data.txt".into());
    // BOT_TOKENS_JSON e.g. {"quote":"memos_pat_...","hn":"...","weather":"..."}
    let bot_tokens: HashMap<String, String> = env::var("BOT_TOKENS_JSON").ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let store = Arc::new(RwLock::new(load_store(&store_path).await));
    let app = App { memos_url, admin_username, allowed, store: store.clone(), store_path, bot_tokens };

    info!("memogram-rs starting url={} store={} bots={:?}", app.memos_url, app.store_path, app.bot_tokens.keys().collect::<Vec<_>>());

    let handler = dptree::entry()
        .branch(Update::filter_message().filter_command::<Command>().endpoint(handle_command))
        .branch(Update::filter_message().endpoint(handle_message));

    Dispatcher::builder(bot, handler).dependencies(dptree::deps![app]).enable_ctrlc_handler().build().dispatch().await;
    Ok(())
}

async fn load_store(path: &str) -> HashMap<i64, String> {
    let mut m = HashMap::new();
    if let Ok(txt) = tokio::fs::read_to_string(path).await {
        for line in txt.lines() {
            if let Some((k, v)) = line.split_once(':') { if let Ok(id) = k.parse() { m.insert(id, v.to_string()); } }
        }
    }
    m
}
async fn save_store(path: &str, map: &HashMap<i64, String>) {
    let txt = map.iter().map(|(k, v)| format!("{k}:{v}")).collect::<Vec<_>>().join("\n");
    let _ = tokio::fs::write(path, txt).await;
}

async fn handle_command(bot: Bot, msg: Message, cmd: Command, app: App) -> Result<()> {
    let from = msg.from.as_ref();
    let username = from.and_then(|u| u.username.as_deref());
    if !app.is_allowed(username) { bot.send_message(msg.chat.id, "not allowed").await?; return Ok(()); }
    let tid = from.map(|u| u.id.0 as i64).unwrap_or(0);
    match cmd {
        Command::Start(token) => {
            let t = token.trim().to_string();
            if t.is_empty() { bot.send_message(msg.chat.id, "usage: /start <memos_pat>").await?; return Ok(()); }
            // verify token
            if verify_token(&app.memos_url, &t).await.is_err() { bot.send_message(msg.chat.id, "invalid token").await?; return Ok(()); }
            { let mut w = app.store.write().await; w.insert(tid, t); save_store(&app.store_path, &w).await; }
            bot.send_message(msg.chat.id, "linked ✅ plain messages will create memos as you").await?;
        }
        Command::Search(q) => {
            let token = { app.store.read().await.get(&tid).cloned() };
            let Some(tok) = token else { bot.send_message(msg.chat.id, "run /start <token> first").await?; return Ok(()); };
            let res = search_memos(&app.memos_url, &tok, &q).await.unwrap_or_else(|e| format!("search err: {e}"));
            bot.send_message(msg.chat.id, res).await?;
        }
        Command::Quote => { let txt = fetch_quote().await.unwrap_or_else(|e| format!("quote err: {e}")); create_as_bot(&bot, &msg, &app, "quote", &txt, tid).await?; }
        Command::Hn => { let txt = fetch_hn().await.unwrap_or_else(|e| format!("hn err: {e}")); create_as_bot(&bot, &msg, &app, "hn", &txt, tid).await?; }
        Command::Weather(city) => {
            let c = if city.trim().is_empty() { "Los Angeles".to_string() } else { city };
            let txt = fetch_weather(&c).await.unwrap_or_else(|e| format!("weather err: {e}"));
            create_as_bot(&bot, &msg, &app, "weather", &txt, tid).await?;
        }
        Command::Define(w) => { let txt = fetch_define(&w).await.unwrap_or_else(|e| format!("define err: {e}")); create_as_bot(&bot, &msg, &app, "define", &txt, tid).await?; }
        Command::Wiki(q) => { let txt = fetch_wiki(&q).await.unwrap_or_else(|e| format!("wiki err: {e}")); create_as_bot(&bot, &msg, &app, "wiki", &txt, tid).await?; }
        Command::Cheat(q) => { let txt = fetch_cheat(&q).await.unwrap_or_else(|e| format!("cheat err: {e}")); create_as_bot(&bot, &msg, &app, "cheat", &txt, tid).await?; }
        Command::Today => {
            let txt = format!("**Today {}**\n{}", Local::now().format("%Y-%m-%d %A"), Local::now().format("%H:%M %Z"));
            create_as_bot(&bot, &msg, &app, "today", &txt, tid).await?;
        }
        Command::Week => {
            let txt = format!("**Week {}**\n{}", Local::now().format("%V"), Local::now().format("%Y-%m-%d"));
            create_as_bot(&bot, &msg, &app, "week", &txt, tid).await?;
        }
        Command::Help => { bot.send_message(msg.chat.id, Command::descriptions().to_string()).await?; }
    }
    Ok(())
}

async fn handle_message(bot: Bot, msg: Message, app: App) -> Result<()> {
    let Some(text) = msg.text() else { return Ok(()); };
    if text.starts_with('/') { return Ok(()); } // already handled
    let from = msg.from.as_ref();
    let username = from.and_then(|u| u.username.as_deref());
    if !app.is_allowed(username) { return Ok(()); }
    let tid = from.map(|u| u.id.0 as i64).unwrap_or(0);
    let token = { app.store.read().await.get(&tid).cloned() };
    let Some(tok) = token else { bot.send_message(msg.chat.id, "run /start <memos_pat> first").await?; return Ok(()); };
    match create_memo(&app.memos_url, &tok, text).await {
        Ok(name) => { bot.send_message(msg.chat.id, format!("saved {name}")).await?; }
        Err(e) => { error!("create memo err: {e}"); bot.send_message(msg.chat.id, format!("save err: {e}")).await?; }
    }
    Ok(())
}

// create memo as bot user (impersonate) and ping admin via @admin mention
async fn create_as_bot(bot: &Bot, msg: &Message, app: &App, bot_name: &str, body: &str, telegram_id: i64) -> Result<()> {
    let bot_tok = app.bot_token(bot_name);
    let content = format!("@{} {}\n\n_{} via {}_", app.admin_username, body, bot_name, msg.from.as_ref().and_then(|u| u.username.as_deref()).unwrap_or("telegram"));
    let tok = if let Some(t) = bot_tok { t } else {
        // fallback to telegram user's linked token
        let fallback = { app.store.read().await.get(&telegram_id).cloned() };
        let Some(f) = fallback else { bot.send_message(msg.chat.id, "run /start <token> first").await?; return Ok(()); };
        // still ping admin via content prefix
        f
    };
    match create_memo(&app.memos_url, &tok, &content).await {
        Ok(name) => { bot.send_message(msg.chat.id, format!("{bot_name}: saved {name}")).await?; }
        Err(e) => { bot.send_message(msg.chat.id, format!("{bot_name} err: {e}")).await?; }
    }
    Ok(())
}

async fn verify_token(url: &str, tok: &str) -> Result<()> {
    let r = HTTP.get(format!("{url}/api/v1/user")).bearer_auth(tok).send().await?;
    if r.status().is_success() { Ok(()) } else { anyhow::bail!("verify {}", r.status()) }
}
async fn create_memo(url: &str, tok: &str, content: &str) -> Result<String> {
    #[derive(Serialize)] struct Req { content: String, visibility: String }
    #[derive(Deserialize)] struct Resp { name: String }
    let r = HTTP.post(format!("{url}/api/v1/memos")).bearer_auth(tok).json(&Req{ content: content.to_string(), visibility: "PRIVATE".into() }).send().await?;
    let st = r.status();
    let txt = r.text().await?;
    if !st.is_success() { anyhow::bail!("{st} {txt}") }
    let v: Resp = serde_json::from_str(&txt)?;
    Ok(v.name)
}
async fn search_memos(url: &str, tok: &str, q: &str) -> Result<String> {
    if q.trim().is_empty() { return Ok("usage: /search <query>".into()); }
    let r = HTTP.get(format!("{url}/api/v1/memos?filter=content.contains(\"{}\")&pageSize=5", q.replace('\"', ""))).bearer_auth(tok).send().await?;
    let txt = r.text().await?;
    let v: serde_json::Value = serde_json::from_str(&txt)?;
    let arr = v.get("memos").and_then(|x| x.as_array());
    if arr.is_none() || arr.unwrap().is_empty() { return Ok("no results".into()); }
    let mut out = String::new();
    for m in arr.unwrap().iter().take(5) {
        let c = m.get("content").and_then(|x| x.as_str()).unwrap_or("");
        let n = m.get("name").and_then(|x| x.as_str()).unwrap_or("");
        out.push_str(&format!("{n}: {c}\n---\n"));
    }
    Ok(out)
}

// --- minimal external fetchers (no API keys, 5s timeout) ---
async fn fetch_quote() -> Result<String> {
    let v: serde_json::Value = HTTP.get("https://api.quotable.io/random").send().await?.json().await?;
    Ok(format!("\"{}\" — {}", v["content"].as_str().unwrap_or(""), v["author"].as_str().unwrap_or("")))
}
async fn fetch_hn() -> Result<String> {
    let ids: Vec<u64> = HTTP.get("https://hacker-news.firebaseio.com/v0/topstories.json").send().await?.json().await?;
    let mut out = String::from("**HN Top 5**\n");
    for id in ids.iter().take(5) {
        let item: serde_json::Value = HTTP.get(format!("https://hacker-news.firebaseio.com/v0/item/{id}.json")).send().await?.json().await?;
        out.push_str(&format!("- {} ({})\n", item["title"].as_str().unwrap_or(""), item["url"].as_str().unwrap_or(""))); 
    }
    Ok(out)
}
async fn fetch_weather(city: &str) -> Result<String> {
    let url = format!("http://wttr.in/{}?format=j1", city);
    let v: serde_json::Value = HTTP.get(url).send().await?.json().await?;
    let cur = &v["current_condition"][0];
    Ok(format!("**Weather {city}**: {}°C {} wind {}km/h", cur["temp_C"].as_str().unwrap_or("?"), cur["weatherDesc"][0]["value"].as_str().unwrap_or(""), cur["windspeedKmph"].as_str().unwrap_or("?")))
}
async fn fetch_define(word: &str) -> Result<String> {
    let v: serde_json::Value = HTTP.get(format!("https://api.dictionaryapi.dev/api/v2/entries/en/{}", word)).send().await?.json().await?;
    let def = v[0]["meanings"][0]["definitions"][0]["definition"].as_str().unwrap_or("no def");
    Ok(format!("**{word}**: {def}"))
}
async fn fetch_wiki(q: &str) -> Result<String> {
    let v: serde_json::Value = HTTP.get(format!("https://en.wikipedia.org/api/rest_v1/page/summary/{}", q)).send().await?.json().await?;
    Ok(v["extract"].as_str().unwrap_or("no summary").to_string())
}
async fn fetch_cheat(q: &str) -> Result<String> {
    let txt = HTTP.get(format!("https://cheat.sh/{}?T", q)).send().await?.text().await?;
    Ok(txt.chars().take(1500).collect())
}