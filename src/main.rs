use anyhow::Result;
use chrono::{Local, Duration, Datelike};
use once_cell::sync::Lazy;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, env, sync::Arc};
use teloxide::{prelude::*, types::ParseMode, utils::{command::BotCommands, markdown as md}};
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
    #[command(description = "weather <city> (wttr.in)")]
    Weather(String),
    #[command(description = "define <word>")]
    Define(String),
    #[command(description = "wiki <query>")]
    Wiki(String),
    #[command(description = "cheat <query> (cheat.sh)")]
    Cheat(String),
    #[command(description = "daily memo digest")]
    Today,
    #[command(description = "weekly memo digest")]
    Week,
    #[command(description = "GitHub search/explore")]
    Gh(String),
    #[command(description = "fx <pair> e.g. USD-KRW")]
    Fx(String),
    #[command(description = "read <url> summarize via jina.ai")]
    Read(String),
    #[command(description = "list task memos")]
    Tasks,
    #[command(description = "check service health")]
    Containers,
    #[command(description = "GitHub trending repos")]
    Trending,
    #[command(description = "reddit <sub> top posts")]
    Reddit(String),
    #[command(description = "stock <ticker> e.g. AAPL")]
    Stock(String),
    #[command(description = "crypto <coin> e.g. bitcoin")]
    Crypto(String),
    #[command(description = "random poem")]
    Poem,
    #[command(description = "deepresearch <query>")]
    Deepresearch(String),
    #[command(description = "help")]
    Help,
}

#[derive(Clone)]
struct App {
    memos_url: String,
    admin_username: String,
    allowed: Option<Vec<String>>,
    store: Arc<RwLock<HashMap<i64, String>>>,
    store_path: String,
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
            warn!("no bot token for {bot}, fallback to memogram store");
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
    let bot_tokens: HashMap<String, String> = env::var("BOT_TOKENS_JSON").ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let store = Arc::new(RwLock::new(load_store(&store_path).await));
    let app = App { memos_url, admin_username, allowed, store: store.clone(), store_path, bot_tokens };

    info!("memogram-rs starting url={} store={} bots={:?}", app.memos_url, app.store_path, app.bot_tokens.keys().collect::<Vec<_>>());

    let _ = bot.set_my_commands(vec![
        teloxide::types::BotCommand { command: "start".into(), description: "link Telegram → Memos".into() },
        teloxide::types::BotCommand { command: "search".into(), description: "search memos".into() },
        teloxide::types::BotCommand { command: "quote".into(), description: "random quote".into() },
        teloxide::types::BotCommand { command: "hn".into(), description: "HackerNews top 5".into() },
        teloxide::types::BotCommand { command: "weather".into(), description: "weather <city>".into() },
        teloxide::types::BotCommand { command: "define".into(), description: "define <word>".into() },
        teloxide::types::BotCommand { command: "wiki".into(), description: "wiki <query>".into() },
        teloxide::types::BotCommand { command: "cheat".into(), description: "cheat <query>".into() },
        teloxide::types::BotCommand { command: "today".into(), description: "daily memo digest".into() },
        teloxide::types::BotCommand { command: "week".into(), description: "weekly memo digest".into() },
        teloxide::types::BotCommand { command: "gh".into(), description: "GitHub search".into() },
        teloxide::types::BotCommand { command: "fx".into(), description: "fx <pair> USD-KRW".into() },
        teloxide::types::BotCommand { command: "read".into(), description: "read <url> summarize".into() },
        teloxide::types::BotCommand { command: "tasks".into(), description: "list task memos".into() },
        teloxide::types::BotCommand { command: "containers".into(), description: "check service health".into() },
        teloxide::types::BotCommand { command: "trending".into(), description: "GitHub trending".into() },
        teloxide::types::BotCommand { command: "reddit".into(), description: "reddit <sub>".into() },
        teloxide::types::BotCommand { command: "stock".into(), description: "stock <ticker>".into() },
        teloxide::types::BotCommand { command: "crypto".into(), description: "crypto <coin>".into() },
        teloxide::types::BotCommand { command: "poem".into(), description: "random poem".into() },
        teloxide::types::BotCommand { command: "deepresearch".into(), description: "deepresearch <query>".into() },
        teloxide::types::BotCommand { command: "help".into(), description: "help".into() },
    ]).await;

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
            let token = { app.store.read().await.get(&tid).cloned() };
            let Some(tok) = token else { bot.send_message(msg.chat.id, "run /start <token> first").await?; return Ok(()); };
            let today = Local::now().format("%Y-%m-%d").to_string();
            let txt = fetch_daily_digest(&app.memos_url, &tok, &today).await.unwrap_or_else(|e| format!("digest err: {e}"));
            create_as_bot(&bot, &msg, &app, "today", &txt, tid).await?;
        }
        Command::Week => {
            let token = { app.store.read().await.get(&tid).cloned() };
            let Some(tok) = token else { bot.send_message(msg.chat.id, "run /start <token> first").await?; return Ok(()); };
            let now = Local::now();
            let monday = now.naive_local().date() - Duration::days(now.naive_local().weekday().num_days_from_monday() as i64);
            let sunday = monday + Duration::days(6);
            let start = monday.format("%Y-%m-%d").to_string();
            let end = sunday.format("%Y-%m-%d").to_string();
            let txt = fetch_weekly_digest(&app.memos_url, &tok, &start, &end).await.unwrap_or_else(|e| format!("digest err: {e}"));
            create_as_bot(&bot, &msg, &app, "week", &txt, tid).await?;
        }
        Command::Gh(q) => { let txt = fetch_gh(&q).await.unwrap_or_else(|e| format!("gh err: {e}")); create_as_bot(&bot, &msg, &app, "gh", &txt, tid).await?; }
        Command::Fx(pair) => { let txt = fetch_fx(&pair).await.unwrap_or_else(|e| format!("fx err: {e}")); create_as_bot(&bot, &msg, &app, "fx", &txt, tid).await?; }
        Command::Read(url) => { let txt = fetch_read(&url).await.unwrap_or_else(|e| format!("read err: {e}")); create_as_bot(&bot, &msg, &app, "read", &txt, tid).await?; }
        Command::Tasks => {
            let token = { app.store.read().await.get(&tid).cloned() };
            let Some(tok) = token else { bot.send_message(msg.chat.id, "run /start <token> first").await?; return Ok(()); };
            let txt = fetch_tasks(&app.memos_url, &tok).await.unwrap_or_else(|e| format!("tasks err: {e}"));
            create_as_bot(&bot, &msg, &app, "tasks", &txt, tid).await?;
        }
        Command::Containers => { let txt = fetch_containers(&app.memos_url).await.unwrap_or_else(|e| format!("containers err: {e}")); create_as_bot(&bot, &msg, &app, "ops", &txt, tid).await?; }
        Command::Trending => { let txt = fetch_trending().await.unwrap_or_else(|e| format!("trending err: {e}")); create_as_bot(&bot, &msg, &app, "gh", &txt, tid).await?; }
        Command::Reddit(sub) => { let txt = fetch_reddit(&sub).await.unwrap_or_else(|e| format!("reddit err: {e}")); create_as_bot(&bot, &msg, &app, "hn", &txt, tid).await?; }
        Command::Stock(ticker) => { let txt = fetch_stock(&ticker).await.unwrap_or_else(|e| format!("stock err: {e}")); create_as_bot(&bot, &msg, &app, "fx", &txt, tid).await?; }
        Command::Crypto(coin) => { let txt = fetch_crypto(&coin).await.unwrap_or_else(|e| format!("crypto err: {e}")); create_as_bot(&bot, &msg, &app, "fx", &txt, tid).await?; }
        Command::Poem => { let txt = fetch_poem().await.unwrap_or_else(|e| format!("poem err: {e}")); create_as_bot(&bot, &msg, &app, "quote", &txt, tid).await?; }
        Command::Deepresearch(q) => { let t = if q.trim().is_empty() { "deepresearch".to_string() } else { q }; create_as_bot(&bot, &msg, &app, "deepresearch", &t, tid).await?; }
        Command::Help => { bot.send_message(msg.chat.id, Command::descriptions().to_string()).await?; }
    }
    Ok(())
}

async fn handle_message(bot: Bot, msg: Message, app: App) -> Result<()> {
    let Some(text) = msg.text() else { return Ok(()); };
    if text.starts_with('/') { return Ok(()); }
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

async fn create_as_bot(bot: &Bot, msg: &Message, app: &App, bot_name: &str, body: &str, telegram_id: i64) -> Result<()> {
    let bot_tok = app.bot_token(bot_name);
    let content = format!("@{}\n\n{}\n\n— _via {} · asher_", app.admin_username, body, bot_name);
    let tok = if let Some(t) = bot_tok { t } else {
        let fallback = { app.store.read().await.get(&telegram_id).cloned() };
        let Some(f) = fallback else { bot.send_message(msg.chat.id, "run /start <token> first").await?; return Ok(()); };
        f
    };
    let _ = bot.send_message(msg.chat.id, body).await;
    match create_memo(&app.memos_url, &tok, &content).await {
        Ok(name) => { bot.send_message(msg.chat.id, format!("{bot_name}: saved {name} → @{} inbox", app.admin_username)).await?; }
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
    let r = HTTP.post(format!("{url}/api/v1/memos")).bearer_auth(tok).json(&Req{ content: content.to_string(), visibility: "PROTECTED".into() }).send().await?;
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

// --- memo digests ---

async fn list_memos_filtered(url: &str, tok: &str, filter: &str) -> Result<Vec<serde_json::Value>> {
    let req_url = format!("{url}/api/v1/memos?filter={}&pageSize=50&orderBy=update_time desc", urlencoding::encode(filter));
    let r = HTTP.get(&req_url).bearer_auth(tok).send().await?;
    let v: serde_json::Value = r.json().await?;
    Ok(v.get("memos").and_then(|x| x.as_array()).cloned().unwrap_or_default())
}

async fn fetch_daily_digest(url: &str, tok: &str, date: &str) -> Result<String> {
    let filter = format!("created_ts >= timestamp(\"{}T00:00:00Z\") && created_ts <= timestamp(\"{}T23:59:59Z\")", date, date);
    let memos = list_memos_filtered(url, tok, &filter).await?;
    if memos.is_empty() { return Ok(format!("## 📋 Daily Digest — {date}\n\n_No memos today._")); }
    let mut out = format!("## 📋 Daily Digest — {date}\n\n**{} memo(s)**\n\n", memos.len());
    for (i, m) in memos.iter().enumerate() {
        let c = m.get("content").and_then(|x| x.as_str()).unwrap_or("");
        let name = m.get("name").and_then(|x| x.as_str()).unwrap_or("");
        let ts = m.get("createTime").and_then(|x| x.as_str()).unwrap_or("");
        let time = ts.get(11..16).unwrap_or("");
        out.push_str(&format!("{}. `{time}` {} — _{}_\n", i + 1, name, chars(c, 80)));
    }
    out.push_str("\n#dailydigest");
    Ok(out)
}

async fn fetch_weekly_digest(url: &str, tok: &str, start: &str, end: &str) -> Result<String> {
    let filter = format!("created_ts >= timestamp(\"{start}T00:00:00Z\") && created_ts <= timestamp(\"{end}T23:59:59Z\")");
    let memos = list_memos_filtered(url, tok, &filter).await?;
    if memos.is_empty() { return Ok(format!("## 📋 Weekly Digest — {start} → {end}\n\n_No memos this week._")); }
    let mut by_date: HashMap<String, Vec<&serde_json::Value>> = HashMap::new();
    for m in &memos {
        let ts = m.get("createTime").and_then(|x| x.as_str()).unwrap_or("");
        let day = ts.get(..10).unwrap_or("unknown").to_string();
        by_date.entry(day).or_default().push(m);
    }
    let mut out = format!("## 📋 Weekly Digest — {start} → {end}\n\n**{} memo(s)**\n\n", memos.len());
    let mut days: Vec<&String> = by_date.keys().collect();
    days.sort();
    for day in days {
        let day_memos = &by_date[day];
        out.push_str(&format!("### {} ({})\n", day, day_memos.len()));
        for m in day_memos {
            let c = m.get("content").and_then(|x| x.as_str()).unwrap_or("");
            let ts = m.get("createTime").and_then(|x| x.as_str()).unwrap_or("");
            let time = ts.get(11..16).unwrap_or("");
            out.push_str(&format!("- `{time}` {}\n", chars(c, 80)));
        }
        out.push('\n');
    }
    out.push_str("#weeklydigest");
    Ok(out)
}

async fn fetch_tasks(url: &str, tok: &str) -> Result<String> {
    let filter = "content.contains(\"#tasks\")";
    let memos = list_memos_filtered(url, tok, filter).await?;
    if memos.is_empty() { return Ok("## ✅ Tasks\n\n_No task memos found (tagged with #tasks)._".into()); }
    let mut out = format!("## ✅ Tasks\n\n**{} task(s)**\n\n", memos.len());
    for (i, m) in memos.iter().enumerate() {
        let c = m.get("content").and_then(|x| x.as_str()).unwrap_or("");
        let name = m.get("name").and_then(|x| x.as_str()).unwrap_or("");
        let ts = m.get("createTime").and_then(|x| x.as_str()).unwrap_or("");
        let _time = ts.get(..10).unwrap_or("");
        out.push_str(&format!("{}. [ ] {} — _{}_\n", i + 1, name, chars(c, 80)));
    }
    out.push_str("\n#tasks");
    Ok(out)
}

fn chars(s: &str, n: usize) -> String {
    let clean: String = s.chars().filter(|c| *c != '\n').collect();
    if clean.len() <= n { clean } else { format!("{}…", &clean[..n]) }
}

// --- external fetchers ---

async fn fetch_quote() -> Result<String> {
    let v: serde_json::Value = if let Ok(j) = HTTP.get("https://dummyjson.com/quotes/random").send().await?.json::<serde_json::Value>().await {
        j
    } else {
        HTTP.get("https://api.quotable.io/random").send().await?.json().await?
    };
    let q = v["quote"].as_str().or(v["content"].as_str()).unwrap_or("");
    let a = v["author"].as_str().unwrap_or("Unknown");
    let id = v["id"].as_u64().map(|i| format!(" #{i}")).unwrap_or_default();
    Ok(format!(
        "## ✨ Quote{id}\n\n> \"{q}\"\n>\n> — *{a}*\n\n`{} chars` · [Goodreads](https://www.goodreads.com/search?q={}) · #quote",
        q.len(),
        urlencoding::encode(a)
    ))
}

async fn fetch_hn() -> Result<String> {
    let ids: Vec<u64> = HTTP.get("https://hacker-news.firebaseio.com/v0/topstories.json").send().await?.json().await?;
    let mut out = String::from("# 🔥 Hacker News — Top 5\n\n| # | Title | Points | Comments | By |\n|---|---|---|---|---|\n");
    for (i, id) in ids.iter().take(5).enumerate() {
        let item: serde_json::Value = HTTP.get(format!("https://hacker-news.firebaseio.com/v0/item/{id}.json")).send().await?.json().await?;
        let title = item["title"].as_str().unwrap_or("(no title)").replace('|', "\\|");
        let url = item["url"].as_str().map(|s| s.to_string()).unwrap_or_else(|| format!("https://news.ycombinator.com/item?id={id}"));
        let score = item["score"].as_u64().unwrap_or(0);
        let comments = item["descendants"].as_u64().unwrap_or(0);
        let by = item["by"].as_str().unwrap_or("?");
        let time = item["time"].as_i64().unwrap_or(0);
        let ago = if time > 0 {
            let hrs = (chrono::Utc::now().timestamp() - time) / 3600;
            if hrs < 1 { "now".into() } else if hrs == 1 { "1h ago".into() } else { format!("{hrs}h ago") }
        } else { "".into() };
        out.push_str(&format!("| {} | [{title}]({url}) | ↑{score} | 💬{comments} | {by} · {ago} |\n", i + 1));
    }
    out.push_str("\n> [View on HN](https://news.ycombinator.com) · #hn");
    Ok(out)
}

async fn fetch_weather(city: &str) -> Result<String> {
    let url = format!("http://wttr.in/{}?format=j1", city);
    let v: serde_json::Value = HTTP.get(url).send().await?.json().await?;
    let cur = &v["current_condition"][0];
    let temp = cur["temp_C"].as_str().unwrap_or("?");
    let feels = cur["FeelsLikeC"].as_str().unwrap_or("?");
    let desc = cur["weatherDesc"][0]["value"].as_str().unwrap_or("");
    let hum = cur["humidity"].as_str().unwrap_or("?");
    let wind = cur["windspeedKmph"].as_str().unwrap_or("?");
    let winddir = cur["winddir16Point"].as_str().unwrap_or("");
    let emoji = match desc.to_lowercase().as_str() {
        s if s.contains("sun") || s.contains("clear") => "☀️",
        s if s.contains("cloud") => "☁️",
        s if s.contains("rain") => "🌧️",
        s if s.contains("snow") => "❄️",
        _ => "🌤️",
    };
    let mut out = format!(
        "## {emoji} Weather — {city}\n\n**Now:** {temp}°C (feels {feels}°C) — *{desc}* · 💧 {hum}% · 💨 {wind} km/h {winddir}\n\n"
    );
    if let Some(arr) = v["weather"].as_array() {
        for day in arr.iter().take(3) {
            let date = day["date"].as_str().unwrap_or("");
            let maxt = day["maxtempC"].as_str().unwrap_or("?");
            let mint = day["mintempC"].as_str().unwrap_or("?");
            out.push_str(&format!("### {date} — ↑{maxt}°C ↓{mint}°C\n\n| Time | Temp | Condition | Rain | Humidity | Wind |\n|---|---|---|---|---|---|\n"));
            if let Some(hours) = day["hourly"].as_array() {
                for h in hours.iter().step_by(2) {
                    let t = h["time"].as_str().unwrap_or("0");
                    let hh = format!("{:0>4}", t);
                    let hm = format!("{}:{}", &hh[0..2], &hh[2..4]);
                    let tc = h["tempC"].as_str().unwrap_or("?");
                    let d = h["weatherDesc"][0]["value"].as_str().unwrap_or("");
                    let rain = h["chanceofrain"].as_str().unwrap_or("?");
                    let hu = h["humidity"].as_str().unwrap_or("?");
                    let wi = h["windspeedKmph"].as_str().unwrap_or("?");
                    out.push_str(&format!("| {hm} | {tc}°C | {d} | {rain}% | {hu}% | {wi} km/h |\n"));
                }
            }
            out.push('\n');
        }
    }
    out.push_str("#weather");
    Ok(out)
}

async fn fetch_define(word: &str) -> Result<String> {
    let v: serde_json::Value = HTTP.get(format!("https://api.dictionaryapi.dev/api/v2/entries/en/{}", word)).send().await?.json().await?;
    let entry = &v[0];
    let phon = entry["phonetic"].as_str().or(entry["phonetics"][0]["text"].as_str()).unwrap_or("");
    let audio = entry["phonetics"][0]["audio"].as_str().unwrap_or("");
    let origin = entry["origin"].as_str().unwrap_or("");
    let mut out = format!("## 📖 {word}\n\n");
    if !phon.is_empty() {
        out.push_str(&format!("*Phonetic:* `{phon}`"));
        if !audio.is_empty() { out.push_str(&format!(" · [🔊]({audio})")); }
        out.push_str("\n\n");
    }
    if !origin.is_empty() { out.push_str(&format!("> *Origin:* {origin}\n\n")); }
    if let Some(meanings) = entry["meanings"].as_array() {
        for m in meanings.iter().take(3) {
            let pos = m["partOfSpeech"].as_str().unwrap_or("");
            out.push_str(&format!("### _{pos}_\n\n"));
            if let Some(defs) = m["definitions"].as_array() {
                for (i, d) in defs.iter().take(3).enumerate() {
                    let def = d["definition"].as_str().unwrap_or("");
                    let ex = d["example"].as_str().unwrap_or("");
                    let syn = d["synonyms"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default();
                    out.push_str(&format!("{}. {def}\n", i + 1));
                    if !ex.is_empty() { out.push_str(&format!("   > *Ex:* {ex}\n")); }
                    if !syn.is_empty() { out.push_str(&format!("   > Syn: `{syn}`\n")); }
                }
            }
            out.push('\n');
        }
    }
    out.push_str("#define");
    Ok(out)
}

async fn fetch_wiki(q: &str) -> Result<String> {
    let v: serde_json::Value = HTTP.get(format!("https://en.wikipedia.org/api/rest_v1/page/summary/{}", q)).send().await?.json().await?;
    let title = v["title"].as_str().unwrap_or(q);
    let extract = v["extract"].as_str().unwrap_or("no summary");
    let url = v["content_urls"]["desktop"]["page"].as_str().map(|s| s.to_string()).unwrap_or_else(|| format!("https://en.wikipedia.org/wiki/{}", q));
    let thumb = v["thumbnail"]["source"].as_str().unwrap_or("");
    let mut out = format!("## 📚 {title}\n\n");
    if !thumb.is_empty() { out.push_str(&format!("![{title}]({thumb})\n\n")); }
    out.push_str(&format!("{extract}\n\n> [Read more on Wikipedia]({url})\n\n#wiki"));
    Ok(out)
}

async fn fetch_cheat(q: &str) -> Result<String> {
    let txt = HTTP.get(format!("https://cheat.sh/{}?TQ", q)).send().await?.text().await?;
    let clean = txt.chars().take(1400).collect::<String>();
    Ok(format!("## 💻 cheat — `{q}`\n\n```sh\n{clean}\n```\n\n> [cheat.sh/{q}](https://cheat.sh/{q}) · #cheat"))
}

async fn fetch_gh(q: &str) -> Result<String> {
    let query = if q.trim().is_empty() { "stars:>50000" } else { q.trim() };
    let url = format!("https://api.github.com/search/repositories?q={}&sort=stars&per_page=5", urlencoding::encode(query));
    let v: serde_json::Value = HTTP.get(&url).header("Accept", "application/vnd.github.v3+json").header("User-Agent", "memogram-rs").send().await?.json().await?;
    let items = v["items"].as_array().ok_or_else(|| anyhow::anyhow!("no items"))?;
    let mut out = format!("## ⭐ GitHub — `{query}`\n\n| Repo | ⭐ Stars | 🍴 Forks | Language |\n|---|---|---|---|\n");
    for it in items.iter().take(5) {
        let name = it["full_name"].as_str().unwrap_or("?");
        let html = it["html_url"].as_str().unwrap_or("");
        let stars = it["stargazers_count"].as_u64().unwrap_or(0);
        let forks = it["forks_count"].as_u64().unwrap_or(0);
        let lang = it["language"].as_str().unwrap_or("-");
        let desc = it["description"].as_str().unwrap_or("").replace('|', "\\|").chars().take(60).collect::<String>();
        out.push_str(&format!("| [{name}]({html}) | {stars} | {forks} | {lang} |\n"));
        if !desc.is_empty() { out.push_str(&format!("|  | *{desc}* | | |\n")); }
    }
    out.push_str("\n> [View on GitHub](https://github.com/search?q=) · #gh");
    Ok(out)
}

async fn fetch_fx(pair: &str) -> Result<String> {
    let parts: Vec<&str> = pair.split('-').collect();
    if parts.len() != 2 { return Ok("usage: /fx USD-KRW".into()); }
    let base = parts[0].to_uppercase();
    let quote = parts[1].to_uppercase();
    let url = format!("https://open.er-api.com/v6/latest/{}", base);
    let v: serde_json::Value = HTTP.get(&url).send().await?.json().await?;
    let rate = v["rates"][&quote].as_f64().ok_or_else(|| anyhow::anyhow!("pair not found"))?;
    let now = Local::now().format("%Y-%m-%d %H:%M");
    Ok(format!(
        "## 💱 Exchange Rate\n\n**1 {base} = {rate:.4} {quote}**\n\n`{now}`\n\n> [Source: ExchangeRate API](https://open.er-api.com) · #fx"
    ))
}

async fn fetch_read(url: &str) -> Result<String> {
    if url.trim().is_empty() { return Ok("usage: /read <url>".into()); }
    let jina_url = format!("https://r.jina.ai/{}", url);
    let txt = HTTP.get(&jina_url).header("Accept", "text/markdown").send().await?.text().await?;
    let clean = txt.chars().take(3800).collect::<String>();
    let title = clean.lines().find(|l| l.starts_with("# ")).unwrap_or("").trim_start_matches("# ");
    let body = clean.lines().skip_while(|l| l.starts_with("# ") || l.is_empty()).take(50).collect::<Vec<_>>().join("\n");
    let mut out = format!("## 📰 Read — `{url}`\n\n");
    if !title.is_empty() { out.push_str(&format!("**{title}**\n\n")); }
    out.push_str(&format!("{body}\n\n> Summarized via [jina.ai](https://jina.ai/reader/) · #read"));
    Ok(out)
}

async fn fetch_containers(memos_url: &str) -> Result<String> {
    let services = vec![
        ("Memos", format!("{memos_url}/api/v1/status")),
        ("Vikunja", "https://vikunja.junilab.xyz".to_string()),
        ("Radicale", "https://radicale.junilab.xyz".to_string()),
        ("Gotify", "http://172.20.0.1:8080/health".to_string()),
    ];
    let mut out = String::from("## 🐳 Service Health\n\n| Service | Status | Latency |\n|---|---|---|\n");
    for (name, url) in services {
        let start = std::time::Instant::now();
        let status = match HTTP.get(&url).timeout(std::time::Duration::from_secs(5)).send().await {
            Ok(r) => {
                let code = r.status().as_u16();
                if code == 200 { "✅ OK".to_string() } else { format!("⚠️ {code}") }
            }
            Err(_) => "❌ DOWN".to_string(),
        };
        let ms = start.elapsed().as_millis();
        out.push_str(&format!("| {name} | {status} | {ms}ms |\n"));
    }
    out.push_str(&format!("\n`{}` · #containers", Local::now().format("%Y-%m-%d %H:%M")));
    Ok(out)
}

async fn fetch_trending() -> Result<String> {
    let v: serde_json::Value = HTTP.get("https://api.github.com/search/repositories?q=stars:>1000+pushed:>2026-08-01&sort=stars&order=desc&per_page=10")
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "memogram-rs")
        .send().await?.json().await?;
    let items = v["items"].as_array().ok_or_else(|| anyhow::anyhow!("no items"))?;
    let mut out = String::from("## 🔥 GitHub Trending — Top 10\n\n| # | Repo | ⭐ Stars | Language | Description |\n|---|---|---|---|---|\n");
    for (i, it) in items.iter().enumerate() {
        let name = it["full_name"].as_str().unwrap_or("?");
        let html = it["html_url"].as_str().unwrap_or("");
        let stars = it["stargazers_count"].as_u64().unwrap_or(0);
        let lang = it["language"].as_str().unwrap_or("-");
        let desc = it["description"].as_str().unwrap_or("").replace('|', "\\|").chars().take(50).collect::<String>();
        out.push_str(&format!("| {} | [{}]({}) | {} | {} | {} |\n", i + 1, name, html, stars, lang, desc));
    }
    out.push_str("\n> [GitHub Trending](https://github.com/trending) · #trending");
    Ok(out)
}

async fn fetch_reddit(sub: &str) -> Result<String> {
    let sub = if sub.trim().is_empty() { "technology" } else { sub.trim() };
    let url = format!("https://www.reddit.com/r/{}/hot.json?limit=5", sub);
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "memogram-rs/0.1").send().await?.json().await?;
    let posts = v["data"]["children"].as_array().ok_or_else(|| anyhow::anyhow!("no posts"))?;
    if posts.is_empty() { return Ok(format!("## 📡 r/{sub}\n\n_No posts found._")); }
    let mut out = format!("## 📡 r/{sub} — Hot\n\n| # | Title | ⬆ | 💬 | By |\n|---|---|---|---|---|\n");
    for (i, p) in posts.iter().enumerate() {
        let d = &p["data"];
        let title = d["title"].as_str().unwrap_or("?").replace('|', "\\|").chars().take(60).collect::<String>();
        let permalink = d["permalink"].as_str().unwrap_or("");
        let score = d["score"].as_u64().unwrap_or(0);
        let comments = d["num_comments"].as_u64().unwrap_or(0);
        let author = d["author"].as_str().unwrap_or("?");
        out.push_str(&format!("| {} | [{}]({}) | {} | {} | {} |\n", i + 1, title, format!("https://reddit.com{permalink}"), score, comments, author));
    }
    out.push_str(&format!("\n> [r/{sub}](https://reddit.com/r/{sub}) · #reddit", sub = sub));
    Ok(out)
}

async fn fetch_stock(ticker: &str) -> Result<String> {
    let ticker = ticker.trim().to_uppercase();
    if ticker.is_empty() { return Ok("usage: /stock AAPL".into()); }
    let url = format!("https://query1.finance.yahoo.com/v8/finance/chart/{}?interval=1d&range=5d", ticker);
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "Mozilla/5.0").send().await?.json().await?;
    let result = v["chart"]["result"].as_array().and_then(|a| a.first()).ok_or_else(|| anyhow::anyhow!("ticker not found"))?;
    let meta = &result["meta"];
    let price = meta["regularMarketPrice"].as_f64().unwrap_or(0.0);
    let prev = meta["chartPreviousClose"].as_f64().unwrap_or(price);
    let change = price - prev;
    let pct = if prev != 0.0 { change / prev * 100.0 } else { 0.0 };
    let emoji = if change >= 0.0 { "📈" } else { "📉" };
    let sign = if change >= 0.0 { "+" } else { "" };
    let name = meta["shortName"].as_str().unwrap_or(&ticker);
    let currency = meta["currency"].as_str().unwrap_or("USD");
    let now = Local::now().format("%Y-%m-%d %H:%M");
    Ok(format!(
        "## {emoji} {name} ({ticker})\n\n**{price:.2} {currency}**\n\n{sign}{change:.2} ({sign}{pct:.2}%)\n\n`{now}` · #stock"
    ))
}

async fn fetch_crypto(coin: &str) -> Result<String> {
    let coin_id = if coin.trim().is_empty() { "bitcoin".to_string() } else { coin.trim().to_lowercase() };
    let url = format!("https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=usd&include_24hr_change=true&include_market_cap=true", coin_id);
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "memogram-rs").send().await?.json().await?;
    let data = v.get(&coin_id).ok_or_else(|| anyhow::anyhow!("coin not found"))?;
    let price = data["usd"].as_f64().unwrap_or(0.0);
    let change = data["usd_24h_change"].as_f64().unwrap_or(0.0);
    let mcap = data["usd_market_cap"].as_f64().unwrap_or(0.0);
    let emoji = if change >= 0.0 { "📈" } else { "📉" };
    let sign = if change >= 0.0 { "+" } else { "" };
    let mcap_str = if mcap >= 1e12 { format!("${:.2}T", mcap / 1e12) } else if mcap >= 1e9 { format!("${:.2}B", mcap / 1e9) } else if mcap >= 1e6 { format!("${:.2}M", mcap / 1e6) } else { format!("${:.0}", mcap) };
    let now = Local::now().format("%Y-%m-%d %H:%M");
    Ok(format!(
        "## {emoji} {coin_id}\n\n**${price:.2}**\n\n{sign}{change:.2}% · MCap: {mcap_str}\n\n`{now}` · #crypto"
    ))
}

async fn fetch_poem() -> Result<String> {
    let v: serde_json::Value = HTTP.get("https://poetrydb.org/random").send().await?.json().await?;
    let poem = v.get(0).ok_or_else(|| anyhow::anyhow!("no poem"))?;
    let title = poem["title"].as_str().unwrap_or("Untitled");
    let author = poem["author"].as_str().unwrap_or("Unknown");
    let lines: Vec<&str> = poem["lines"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
    let body: Vec<&str> = lines.iter().take(20).copied().collect();
    let mut out = format!("## 📜 {title}\n\n*{author}*\n\n");
    for line in &body {
        if line.is_empty() { out.push('\n'); } else { out.push_str(line); out.push('\n'); }
    }
    if lines.len() > 20 { out.push_str("\n_...truncated_"); }
    out.push_str("\n\n> [Poetry DB](https://poetrydb.org) · #poem");
    Ok(out)
}
