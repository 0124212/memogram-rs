use anyhow::Result;
use base64::Engine;
use chrono::{Local, Duration, Datelike};
use once_cell::sync::Lazy;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, env, io::Cursor, sync::Arc};
use teloxide::{prelude::*, types::ParseMode, utils::{command::BotCommands, markdown as md}};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

static HTTP: Lazy<Client> = Lazy::new(|| Client::builder().user_agent("memogram-rs/0.1").build().unwrap());

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Commands:")]
enum Command {
    // --- core ---
    #[command(description = "link Telegram → Memos: /start <memos_pat>")]
    Start(String),
    #[command(description = "search memos")]
    Search(String),
    #[command(description = "help")]
    Help,
    // --- knowledge mgmt (obsidian-style) ---
    #[command(description = "list all tags")]
    Tags,
    #[command(description = "recent memos (last 7 days)")]
    Recent,
    #[command(description = "count memos / memos by tag")]
    Count(String),
    #[command(description = "pin/unpin memo by name")]
    Pin(String),
    #[command(description = "archive memo by name")]
    Archive(String),
    #[command(description = "export recent memos as markdown")]
    Export,
    #[command(description = "create daily note for today")]
    Daily,
    // --- info lookups ---
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
    #[command(description = "lobsters <tag> tech news")]
    Lobsters(String),
    #[command(description = "stock <ticker> e.g. AAPL")]
    Stock(String),
    #[command(description = "crypto <coin> e.g. bitcoin")]
    Crypto(String),
    #[command(description = "random poem")]
    Poem,
    #[command(description = "deepresearch <query>")]
    Deepresearch(String),
    #[command(description = "latest XKCD comic")]
    Xkcd,
    #[command(description = "translate <text> or /translate ja → en <text>")]
    Translate(String),
    #[command(description = "random fun fact")]
    Facts,
    #[command(description = "color <hex> e.g. #FF5733 or FF5733")]
    Color(String),
    #[command(description = "morning briefing: health + news + weather")]
    All,
    #[command(description = "shah <query> halal web search")]
    Shah(String),
    #[command(description = "7-day weather forecast")]
    Forecast(String),
    #[command(description = "number trivia (e.g. /num 42)")]
    Num(String),
    // --- utilities ---
    #[command(description = "random password [length]")]
    Pass(String),
    #[command(description = "generate UUID v4")]
    Uuid,
    #[command(description = "IP geolocation [address]")]
    Ip(String),
    #[command(description = "QR code from text")]
    Qr(String),
    #[command(description = "SHA256 hash of text")]
    Hash(String),
    #[command(description = "base64 encode/decode: /base64 e <text> or /base64 d <text>")]
    Base64(String),
    #[command(description = "random joke")]
    Joke,
    #[command(description = "date info: day of year, days left")]
    Day,
    #[command(description = "roll dice e.g. 2d6 or d20")]
    Roll(String),
    #[command(description = "random pick from a,b,c")]
    Choose(String),
    #[command(description = "word/char/line count")]
    Wc(String),
    #[command(description = "set timer → Gotify: /timer 5 drink water")]
    Timer(String),
    #[command(description = "pretty-print JSON")]
    Json(String),
    #[command(description = "morse code encode/decode")]
    Morse(String),
    #[command(description = "magic 8-ball")]
    Eightball,
    #[command(description = "text statistics: readability, entropy")]
    Stats(String),
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
        Command::Lobsters(tag) => { let txt = fetch_lobsters(&tag).await.unwrap_or_else(|e| format!("lobsters err: {e}")); create_as_bot(&bot, &msg, &app, "hn", &txt, tid).await?; }
        Command::Stock(ticker) => { let txt = fetch_stock(&ticker).await.unwrap_or_else(|e| format!("stock err: {e}")); create_as_bot(&bot, &msg, &app, "fx", &txt, tid).await?; }
        Command::Crypto(coin) => { let txt = fetch_crypto(&coin).await.unwrap_or_else(|e| format!("crypto err: {e}")); create_as_bot(&bot, &msg, &app, "fx", &txt, tid).await?; }
        Command::Poem => { let txt = fetch_poem().await.unwrap_or_else(|e| format!("poem err: {e}")); create_as_bot(&bot, &msg, &app, "quote", &txt, tid).await?; }
        Command::Deepresearch(q) => { let t = if q.trim().is_empty() { "deepresearch".to_string() } else { q }; create_as_bot(&bot, &msg, &app, "deepresearch", &t, tid).await?; }
        Command::Xkcd => { let txt = fetch_xkcd().await.unwrap_or_else(|e| format!("xkcd err: {e}")); create_as_bot(&bot, &msg, &app, "quote", &txt, tid).await?; }
        Command::Translate(args) => { let txt = fetch_translate(&args).await.unwrap_or_else(|e| format!("translate err: {e}")); create_as_bot(&bot, &msg, &app, "define", &txt, tid).await?; }
        Command::Facts => { let txt = fetch_facts().await.unwrap_or_else(|e| format!("facts err: {e}")); create_as_bot(&bot, &msg, &app, "quote", &txt, tid).await?; }
        Command::Color(hex) => { let txt = fetch_color(&hex); create_as_bot(&bot, &msg, &app, "define", &txt, tid).await?; }
        Command::All => { let txt = fetch_all(&app.memos_url).await.unwrap_or_else(|e| format!("all err: {e}")); create_as_bot(&bot, &msg, &app, "ops", &txt, tid).await?; }
        Command::Shah(q) => { let txt = fetch_shah(&q).await.unwrap_or_else(|e| format!("shah err: {e}")); create_as_bot(&bot, &msg, &app, "hn", &txt, tid).await?; }
        Command::Forecast(city) => { let txt = fetch_forecast(&city).await.unwrap_or_else(|e| format!("forecast err: {e}")); create_as_bot(&bot, &msg, &app, "weather", &txt, tid).await?; }
        Command::Num(n) => { let txt = fetch_num(&n).await.unwrap_or_else(|e| format!("num err: {e}")); create_as_bot(&bot, &msg, &app, "quote", &txt, tid).await?; }
        // --- knowledge mgmt ---
        Command::Tags => {
            let token = { app.store.read().await.get(&tid).cloned() };
            let Some(tok) = token else { bot.send_message(msg.chat.id, "run /start <token> first").await?; return Ok(()); };
            let txt = fetch_tags(&app.memos_url, &tok).await.unwrap_or_else(|e| format!("tags err: {e}"));
            create_as_bot(&bot, &msg, &app, "today", &txt, tid).await?;
        }
        Command::Recent => {
            let token = { app.store.read().await.get(&tid).cloned() };
            let Some(tok) = token else { bot.send_message(msg.chat.id, "run /start <token> first").await?; return Ok(()); };
            let txt = fetch_recent(&app.memos_url, &tok).await.unwrap_or_else(|e| format!("recent err: {e}"));
            create_as_bot(&bot, &msg, &app, "today", &txt, tid).await?;
        }
        Command::Count(tag) => {
            let token = { app.store.read().await.get(&tid).cloned() };
            let Some(tok) = token else { bot.send_message(msg.chat.id, "run /start <token> first").await?; return Ok(()); };
            let txt = fetch_count(&app.memos_url, &tok, &tag).await.unwrap_or_else(|e| format!("count err: {e}"));
            create_as_bot(&bot, &msg, &app, "today", &txt, tid).await?;
        }
        Command::Pin(name) => {
            let token = { app.store.read().await.get(&tid).cloned() };
            let Some(tok) = token else { bot.send_message(msg.chat.id, "run /start <token> first").await?; return Ok(()); };
            let txt = fetch_pin(&app.memos_url, &tok, &name).await.unwrap_or_else(|e| format!("pin err: {e}"));
            create_as_bot(&bot, &msg, &app, "today", &txt, tid).await?;
        }
        Command::Archive(name) => {
            let token = { app.store.read().await.get(&tid).cloned() };
            let Some(tok) = token else { bot.send_message(msg.chat.id, "run /start <token> first").await?; return Ok(()); };
            let txt = fetch_archive(&app.memos_url, &tok, &name).await.unwrap_or_else(|e| format!("archive err: {e}"));
            create_as_bot(&bot, &msg, &app, "today", &txt, tid).await?;
        }
        Command::Export => {
            let token = { app.store.read().await.get(&tid).cloned() };
            let Some(tok) = token else { bot.send_message(msg.chat.id, "run /start <token> first").await?; return Ok(()); };
            let txt = fetch_export(&app.memos_url, &tok).await.unwrap_or_else(|e| format!("export err: {e}"));
            create_as_bot(&bot, &msg, &app, "today", &txt, tid).await?;
        }
        Command::Daily => {
            let token = { app.store.read().await.get(&tid).cloned() };
            let Some(tok) = token else { bot.send_message(msg.chat.id, "run /start <token> first").await?; return Ok(()); };
            let txt = fetch_daily(&app.memos_url, &tok).await.unwrap_or_else(|e| format!("daily err: {e}"));
            create_as_bot(&bot, &msg, &app, "today", &txt, tid).await?;
        }
        // --- utilities ---
        Command::Pass(len) => { let txt = gen_password(&len); bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?; }
        Command::Uuid => { let txt = gen_uuid(); bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?; }
        Command::Ip(addr) => { let txt = fetch_ip(&addr).await.unwrap_or_else(|e| format!("ip err: {e}")); bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?; }
        Command::Qr(text) => { let txt = gen_qr(&text); bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?; }
        Command::Hash(text) => { let txt = gen_hash(&text); bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?; }
        Command::Base64(args) => { let txt = gen_base64(&args); bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?; }
        Command::Joke => { let txt = fetch_joke().await.unwrap_or_else(|e| format!("joke err: {e}")); bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?; }
        Command::Day => { let txt = gen_day_info(); bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?; }
        Command::Roll(dice) => { let txt = gen_roll(&dice); bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?; }
        Command::Choose(opts) => { let txt = gen_choose(&opts); bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?; }
        Command::Wc(text) => { let txt = gen_wc(&text); bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?; }
        Command::Timer(args) => { let txt = set_timer(&args, &app).await.unwrap_or_else(|e| format!("timer err: {e}")); bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?; }
        Command::Json(text) => { let txt = gen_json_pretty(&text); bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?; }
        Command::Morse(text) => { let txt = gen_morse(&text); bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?; }
        Command::Eightball => { let txt = gen_8ball(); bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?; }
        Command::Stats(text) => { let txt = gen_stats(&text); bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?; }
        Command::Help => { bot.send_message(msg.chat.id, Command::descriptions().to_string()).await?; }
    }
    Ok(())
}

async fn handle_message(bot: Bot, msg: Message, app: App) -> Result<()> {
    let from = msg.from.as_ref();
    let username = from.and_then(|u| u.username.as_deref());
    if !app.is_allowed(username) { return Ok(()); }
    let tid = from.map(|u| u.id.0 as i64).unwrap_or(0);
    let token = { app.store.read().await.get(&tid).cloned() };
    let Some(tok) = token else { bot.send_message(msg.chat.id, "run /start <memos_pat> first").await?; return Ok(()); };

    let caption = msg.caption().unwrap_or("").trim();
    let has_photo = msg.photo().is_some();
    let has_doc = msg.document().is_some();

    if !has_photo && !has_doc {
        let Some(text) = msg.text() else { return Ok(()); };
        if text.starts_with('/') { return Ok(()); }
        match create_memo(&app.memos_url, &tok, text).await {
            Ok(name) => { bot.send_message(msg.chat.id, format!("saved {name}")).await?; }
            Err(e) => { error!("create memo err: {e}"); bot.send_message(msg.chat.id, format!("save err: {e}")).await?; }
        }
        return Ok(());
    }

    let mut att_names: Vec<String> = Vec::new();
    let mut att_labels: Vec<String> = Vec::new();

    if let Some(photos) = msg.photo() {
        if let Some(big) = photos.last() {
            match download_telegram_file(&bot, &big.file.id).await {
                Ok(data) => {
                    let mime = "image/jpeg";
                    let fname = format!("photo_{}.jpg", msg.id.0);
                    match upload_attachment(&app.memos_url, &tok, &fname, mime, &data).await {
                        Ok(name) => { att_names.push(name); att_labels.push(format!("📷 photo ({}KB)", data.len() / 1024)); }
                        Err(e) => { warn!("attach upload err: {e}"); att_labels.push("📷 photo (upload failed)".into()); }
                    }
                }
                Err(e) => { warn!("download err: {e}"); att_labels.push("📷 photo (download failed)".into()); }
            }
        }
    }

    if let Some(doc) = msg.document() {
        match download_telegram_file(&bot, &doc.file.id).await {
            Ok(data) => {
                let mime = doc.mime_type.as_deref().unwrap_or("application/octet-stream");
                let fname = doc.file_name.clone().unwrap_or_else(|| format!("doc_{}", msg.id.0));
                match upload_attachment(&app.memos_url, &tok, &fname, mime, &data).await {
                    Ok(name) => { att_names.push(name); att_labels.push(format!("📎 {} ({}KB)", fname, data.len() / 1024)); }
                    Err(e) => { warn!("attach upload err: {e}"); att_labels.push(format!("📎 {} (upload failed)", fname)); }
                }
            }
            Err(e) => { warn!("download err: {e}"); att_labels.push("📎 document (download failed)".into()); }
        }
    }

    let body = if caption.is_empty() {
        att_labels.join("\n")
    } else {
        format!("{}\n\n{}", caption, att_labels.join("\n"))
    };

    if att_names.is_empty() {
        match create_memo(&app.memos_url, &tok, &body).await {
            Ok(name) => { bot.send_message(msg.chat.id, format!("saved {name}")).await?; }
            Err(e) => { error!("create memo err: {e}"); bot.send_message(msg.chat.id, format!("save err: {e}")).await?; }
        }
    } else {
        match create_memo_with_attachments(&app.memos_url, &tok, &body, &att_names).await {
            Ok(name) => { bot.send_message(msg.chat.id, format!("saved {name}")).await?; }
            Err(e) => { error!("create memo err: {e}"); bot.send_message(msg.chat.id, format!("save err: {e}")).await?; }
        }
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

async fn create_memo_with_attachments(url: &str, tok: &str, content: &str, attachment_names: &[String]) -> Result<String> {
    #[derive(Serialize)] struct AttRef { name: String }
    #[derive(Serialize)] struct Req { content: String, visibility: String, attachments: Vec<AttRef> }
    #[derive(Deserialize)] struct Resp { name: String }
    let atts: Vec<AttRef> = attachment_names.iter().map(|n| AttRef { name: n.clone() }).collect();
    let r = HTTP.post(format!("{url}/api/v1/memos")).bearer_auth(tok).json(&Req{ content: content.to_string(), visibility: "PROTECTED".into(), attachments: atts }).send().await?;
    let st = r.status();
    let txt = r.text().await?;
    if !st.is_success() { anyhow::bail!("{st} {txt}") }
    let v: Resp = serde_json::from_str(&txt)?;
    Ok(v.name)
}

async fn upload_attachment(url: &str, tok: &str, filename: &str, mime: &str, data: &[u8]) -> Result<String> {
    #[derive(Serialize)] struct Req { content: String, filename: String, #[serde(rename = "type")] mime: String }
    #[derive(Deserialize)] struct Resp { name: String }
    let b64 = base64::engine::general_purpose::STANDARD.encode(data);
    let r = HTTP.post(format!("{url}/api/v1/attachments")).bearer_auth(tok).json(&Req{ content: b64, filename: filename.to_string(), mime: mime.to_string() }).send().await?;
    let st = r.status();
    let txt = r.text().await?;
    if !st.is_success() { anyhow::bail!("{st} {txt}") }
    let v: Resp = serde_json::from_str(&txt)?;
    Ok(v.name)
}

async fn download_telegram_file(bot: &Bot, file_id: &str) -> Result<Vec<u8>> {
    let file = bot.get_file(file_id).await?;
    let mut cursor = std::io::Cursor::new(Vec::new());
    bot.download_file(&file.path, &mut cursor).await?;
    Ok(cursor.into_inner())
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

// --- Telegram-optimized markdown fetchers ---
// Telegram doesn't render tables. Use: bullet lists, code blocks for aligned data, bold/italic for structure.

fn esc(s: &str) -> String {
    // Escape MarkdownV2 special chars (outside code blocks)
    s.replace('\\', "\\\\")
     .replace('_', "\\_")
     .replace('*', "\\*")
     .replace('[', "\\[").replace(']', "\\]")
     .replace('(', "\\(").replace(')', "\\)")
     .replace('~', "\\~").replace('`', "\\`")
     .replace('>', "\\>").replace('#', "\\#")
     .replace('+', "\\+").replace('-', "\\-")
     .replace('=', "\\=")
     .replace('{', "\\{").replace('}', "\\}")
     .replace('.', "\\.").replace('!', "\\!")
}

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
        "*✨ Quote{id}*\n\n>||\"{}\"||\n>\n>— _{a}_\n\n`{} chars` · [Goodreads](https://www.goodreads.com/search?q={}) · #quote",
        esc(q), q.len(), urlencoding::encode(a)
    ))
}

async fn fetch_hn() -> Result<String> {
    let ids: Vec<u64> = HTTP.get("https://hacker-news.firebaseio.com/v0/topstories.json").send().await?.json().await?;
    let mut out = String::from("*🔥 Hacker News — Top 5*\n\n");
    for (i, id) in ids.iter().take(5).enumerate() {
        let item: serde_json::Value = HTTP.get(format!("https://hacker-news.firebaseio.com/v0/item/{id}.json")).send().await?.json().await?;
        let title = item["title"].as_str().unwrap_or("(no title)");
        let url = item["url"].as_str().map(|s| s.to_string()).unwrap_or_else(|| format!("https://news.ycombinator.com/item?id={id}"));
        let score = item["score"].as_u64().unwrap_or(0);
        let comments = item["descendants"].as_u64().unwrap_or(0);
        let by = item["by"].as_str().unwrap_or("?");
        let time = item["time"].as_i64().unwrap_or(0);
        let ago = if time > 0 {
            let hrs = (chrono::Utc::now().timestamp() - time) / 3600;
            if hrs < 1 { "now".into() } else if hrs == 1 { "1h".into() } else { format!("{hrs}h") }
        } else { "?".into() };
        out.push_str(&format!("*{}.* [{}]({})\n   ↑{score} · 💬{comments} · {by} · {ago}\n\n", i + 1, esc(title), url));
    }
    out.push_str("> [View on HN](https://news.ycombinator.com) · #hn");
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
        "*{emoji} Weather — {city}*\n\n`Now:  {temp}°C` (feels {feels}°C)\n`Desc: {desc}`\n`Hum:  {hum}%`\n`Wind: {wind} km/h {winddir}`\n\n"
    );
    if let Some(arr) = v["weather"].as_array() {
        for day in arr.iter().take(3) {
            let date = day["date"].as_str().unwrap_or("");
            let maxt = day["maxtempC"].as_str().unwrap_or("?");
            let mint = day["mintempC"].as_str().unwrap_or("?");
            out.push_str(&format!("*{date}* — ↑{maxt}°C ↓{mint}°C\n"));
            if let Some(hours) = day["hourly"].as_array() {
                let mut table = String::from("```\nTime  Temp  Condition        Rain  Hum  Wind\n");
                for h in hours.iter().step_by(2) {
                    let t = h["time"].as_str().unwrap_or("0");
                    let hh = format!("{:0>4}", t);
                    let hm = format!("{}:{}", &hh[0..2], &hh[2..4]);
                    let tc = h["tempC"].as_str().unwrap_or("?");
                    let d = h["weatherDesc"][0]["value"].as_str().unwrap_or("");
                    let rain = h["chanceofrain"].as_str().unwrap_or("?");
                    let hu = h["humidity"].as_str().unwrap_or("?");
                    let wi = h["windspeedKmph"].as_str().unwrap_or("?");
                    table.push_str(&format!("{hm}  {tc:>4}°C  {d:<16} {rain:>3}%  {hu:>2}%  {wi:>3}\n"));
                }
                table.push_str("```\n");
                out.push_str(&table);
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
    let mut out = format!("*📖 {word}*\n\n");
    if !phon.is_empty() {
        out.push_str(&format!("`Phonetic:` {phon}"));
        if !audio.is_empty() { out.push_str(&format!(" · [🔊]({audio})")); }
        out.push_str("\n\n");
    }
    if !origin.is_empty() { out.push_str(&format!("> _Origin:_ {origin}\n\n")); }
    if let Some(meanings) = entry["meanings"].as_array() {
        for m in meanings.iter().take(3) {
            let pos = m["partOfSpeech"].as_str().unwrap_or("");
            out.push_str(&format!("*_{pos}_*\n"));
            if let Some(defs) = m["definitions"].as_array() {
                for (i, d) in defs.iter().take(3).enumerate() {
                    let def = d["definition"].as_str().unwrap_or("");
                    let ex = d["example"].as_str().unwrap_or("");
                    let syn = d["synonyms"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default();
                    out.push_str(&format!("  *{}.* {}", i + 1, esc(def)));
                    if !ex.is_empty() { out.push_str(&format!("\n   > _Ex:_ {} ", esc(ex))); }
                    if !syn.is_empty() { out.push_str(&format!("\n   `Syn:` {syn}")); }
                    out.push('\n');
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
    let mut out = format!("*📚 {title}*\n\n");
    if !thumb.is_empty() { out.push_str(&format!("[📷 Photo]({thumb})\n\n")); }
    out.push_str(&format!("{extract}\n\n> [Read more on Wikipedia]({url})\n\n#wiki"));
    Ok(out)
}

async fn fetch_cheat(q: &str) -> Result<String> {
    let txt = HTTP.get(format!("https://cheat.sh/{}?TQ", q)).send().await?.text().await?;
    let clean = txt.chars().take(1400).collect::<String>();
    Ok(format!("*💻 cheat — `{q}`*\n\n```\n{clean}\n```\n\n> [cheat.sh/{q}](https://cheat.sh/{q}) · #cheat"))
}

async fn fetch_gh(q: &str) -> Result<String> {
    let query = if q.trim().is_empty() { "stars:>50000" } else { q.trim() };
    let url = format!("https://api.github.com/search/repositories?q={}&sort=stars&per_page=5", urlencoding::encode(query));
    let v: serde_json::Value = HTTP.get(&url).header("Accept", "application/vnd.github.v3+json").header("User-Agent", "memogram-rs").send().await?.json().await?;
    let items = v["items"].as_array().ok_or_else(|| anyhow::anyhow!("no items"))?;
    let mut out = format!("*⭐ GitHub — `{query}`*\n\n");
    for it in items.iter().take(5) {
        let name = it["full_name"].as_str().unwrap_or("?");
        let html = it["html_url"].as_str().unwrap_or("");
        let stars = it["stargazers_count"].as_u64().unwrap_or(0);
        let forks = it["forks_count"].as_u64().unwrap_or(0);
        let lang = it["language"].as_str().unwrap_or("-");
        let desc = it["description"].as_str().unwrap_or("").chars().take(60).collect::<String>();
        out.push_str(&format!("[{name}]({html})\n   ⭐ {stars} · 🍴 {forks} · `{lang}`\n   _{}_\n\n", esc(&desc)));
    }
    out.push_str("> [View on GitHub](https://github.com/search?q=) · #gh");
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
        "*💱 Exchange Rate*\n\n`1 {base} = {rate:.4} {quote}`\n\n`{now}`\n\n> [Source](https://open.er-api.com) · #fx"
    ))
}

async fn fetch_read(url: &str) -> Result<String> {
    if url.trim().is_empty() { return Ok("usage: /read <url>".into()); }
    let jina_url = format!("https://r.jina.ai/{}", url);
    let txt = HTTP.get(&jina_url).header("Accept", "text/markdown").send().await?.text().await?;
    let clean = txt.chars().take(3800).collect::<String>();
    let title = clean.lines().find(|l| l.starts_with("# ")).unwrap_or("").trim_start_matches("# ");
    let body = clean.lines().skip_while(|l| l.starts_with("# ") || l.is_empty()).take(50).collect::<Vec<_>>().join("\n");
    let mut out = format!("*📰 Read*\n\n");
    if !title.is_empty() { out.push_str(&format!("**{title}**\n\n")); }
    out.push_str(&format!("{body}\n\n> [jina.ai](https://jina.ai/reader/) · #read"));
    Ok(out)
}

async fn fetch_containers(memos_url: &str) -> Result<String> {
    let services = vec![
        ("Memos", format!("{memos_url}/api/v1/status")),
        ("Vikunja", "http://vikunja:3456/health".to_string()),
        ("Radicale", "http://radicale:5232".to_string()),
        ("Gotify", "http://gotify:8080/health".to_string()),
    ];
    let mut out = String::from("*🐳 Service Health*\n\n");
    let mut table = String::from("```\nService     Status      Latency\n");
    table.push_str("─────────── ─────────── ───────\n");
    for (name, url) in services {
        let start = std::time::Instant::now();
        let status = match HTTP.get(&url).timeout(std::time::Duration::from_secs(5)).send().await {
            Ok(r) => {
                let code = r.status().as_u16();
                if code == 200 { "✅ OK".to_string() } else { format!("⚠️  {code}") }
            }
            Err(_) => "❌ DOWN".to_string(),
        };
        let ms = start.elapsed().as_millis();
        table.push_str(&format!("{name:<11} {status:<11} {ms}ms\n"));
    }
    table.push_str("```\n");
    out.push_str(&table);
    out.push_str(&format!("\n`{}` · #containers", Local::now().format("%Y-%m-%d %H:%M")));
    Ok(out)
}

async fn fetch_trending() -> Result<String> {
    let v: serde_json::Value = HTTP.get("https://api.github.com/search/repositories?q=stars:>1000+pushed:>2026-08-01&sort=stars&order=desc&per_page=10")
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "memogram-rs")
        .send().await?.json().await?;
    let items = v["items"].as_array().ok_or_else(|| anyhow::anyhow!("no items"))?;
    let mut out = String::from("*🔥 GitHub Trending — Top 10*\n\n");
    for (i, it) in items.iter().enumerate() {
        let name = it["full_name"].as_str().unwrap_or("?");
        let html = it["html_url"].as_str().unwrap_or("");
        let stars = it["stargazers_count"].as_u64().unwrap_or(0);
        let lang = it["language"].as_str().unwrap_or("-");
        let desc = it["description"].as_str().unwrap_or("").chars().take(50).collect::<String>();
        out.push_str(&format!("*{}.* [{name}]({html})\n   ⭐ {stars} · `{lang}`\n   _{}_\n\n", i + 1, esc(&desc)));
    }
    out.push_str("> [GitHub Trending](https://github.com/trending) · #trending");
    Ok(out)
}

async fn fetch_lobsters(tag: &str) -> Result<String> {
    let url = "https://lobste.rs/hottest.json";
    let v: serde_json::Value = HTTP.get(url).header("Accept", "application/json").send().await?.json().await?;
    let stories = v.as_array().ok_or_else(|| anyhow::anyhow!("no stories"))?;
    let filtered: Vec<&serde_json::Value> = if tag.trim().is_empty() {
        stories.iter().take(7).collect()
    } else {
        let t = tag.trim().to_lowercase();
        stories.iter().filter(|s| {
            s["tags"].as_array().map(|tags| tags.iter().any(|tag| tag.as_str().unwrap_or("").to_lowercase() == t)).unwrap_or(false)
        }).take(7).collect()
    };
    if filtered.is_empty() { return Ok(format!("*🦞 Lobste.rs*\n\n_No stories found for `{tag}`._")); }
    let mut out = format!("*🦞 Lobste.rs*");
    if !tag.trim().is_empty() { out.push_str(&format!(" — `{tag}`")); }
    out.push_str("\n\n");
    for (i, s) in filtered.iter().enumerate() {
        let title = s["title"].as_str().unwrap_or("?");
        let url = s["url"].as_str().unwrap_or("");
        let comments_url = s["comments_url"].as_str().unwrap_or("");
        let score = s["score"].as_u64().unwrap_or(0);
        let comments = s["comment_count"].as_u64().unwrap_or(0);
        let author = s["submitter_user"]["username"].as_str().unwrap_or("?");
        let tags: Vec<&str> = s["tags"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
        let tag_str = tags.iter().map(|t| format!("`{t}`")).collect::<Vec<_>>().join(" ");
        let display_url = if url.is_empty() { comments_url } else { url };
        out.push_str(&format!("*{}.* [{}]({})\n   ⬆ {score} · 💬 {comments} · {author}\n   {tag_str}\n\n", i + 1, esc(title), display_url));
    }
    out.push_str("> [lobste.rs](https://lobste.rs) · #lobsters");
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
        "*{emoji} {name} ({ticker})*\n\n```\n{price:.2} {currency}\n{sign}{change:.2} ({sign}{pct:.2}%)\n```\n\n`{now}` · #stock"
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
        "*{emoji} {coin_id}*\n\n```\n${price:.2}\n{sign}{change:.2}%\nMCap: {mcap_str}\n```\n\n`{now}` · #crypto"
    ))
}

async fn fetch_poem() -> Result<String> {
    let v: serde_json::Value = HTTP.get("https://poetrydb.org/random").send().await?.json().await?;
    let poem = v.get(0).ok_or_else(|| anyhow::anyhow!("no poem"))?;
    let title = poem["title"].as_str().unwrap_or("Untitled");
    let author = poem["author"].as_str().unwrap_or("Unknown");
    let lines: Vec<&str> = poem["lines"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
    let body: Vec<&str> = lines.iter().take(20).copied().collect();
    let mut out = format!("*📜 {title}*\n\n_{author}_\n\n");
    for line in &body {
        if line.is_empty() { out.push('\n'); } else { out.push_str(line); out.push('\n'); }
    }
    if lines.len() > 20 { out.push_str("\n_...truncated_"); }
    out.push_str("\n\n> [Poetry DB](https://poetrydb.org) · #poem");
    Ok(out)
}

async fn fetch_xkcd() -> Result<String> {
    let v: serde_json::Value = HTTP.get("https://xkcd.com/info.0.json").send().await?.json().await?;
    let num = v["num"].as_u64().unwrap_or(0);
    let title = v["title"].as_str().unwrap_or("?");
    let alt = v["alt"].as_str().unwrap_or("");
    let img = v["img"].as_str().unwrap_or("");
    let date = format!("{}/{}", v["month"].as_str().unwrap_or("?"), v["year"].as_str().unwrap_or("?"));
    Ok(format!(
        "*#{num} — {title}*\n\n![XKCD]({img})\n\n>||{alt}||\n\n_[{date}](https://xkcd.com/{num}/) · #xkcd_"
    ))
}

async fn fetch_translate(args: &str) -> Result<String> {
    let (langpair, text) = if args.contains("→") {
        let parts: Vec<&str> = args.splitn(2, "→").collect();
        let lang = parts[0].trim();
        let rest = parts.get(1).unwrap_or(&"").trim();
        let (target, body) = if let Some(sp) = rest.find(' ') {
            (&rest[..sp], &rest[sp+1..])
        } else {
            ("en", rest)
        };
        (format!("{}|{}", lang, target), body.to_string())
    } else {
        ("auto|en".to_string(), args.to_string())
    };
    if text.trim().is_empty() { return Ok("usage: `/translate <text>` or `/translate ja → en <text>`".into()); }
    let url = format!("https://api.mymemory.translated.net/get?q={}&langpair={}", urlencoding::encode(&text), urlencoding::encode(&langpair));
    let v: serde_json::Value = HTTP.get(&url).send().await?.json().await?;
    let translated = v["responseData"]["translatedText"].as_str().unwrap_or("(no result)");
    let detected = v["responseData"]["match"].as_f64().unwrap_or(0.0);
    let src = langpair.split('|').next().unwrap_or("auto");
    let tgt = langpair.split('|').last().unwrap_or("en");
    Ok(format!(
        "*🌐 Translation*\n\n`{src}` → `{tgt}` (confidence: {detected:.0}%)\n\n*Original:* {text}\n\n*Translated:* {translated}\n\n> [MyMemory](https://mymemory.translated.net) · #translate"
    ))
}

async fn fetch_facts() -> Result<String> {
    let v: serde_json::Value = HTTP.get("https://uselessfacts.jsph.pl/api/v2/facts/random?language=en").send().await?.json().await?;
    let fact = v["text"].as_str().unwrap_or("(no fact)");
    let source = v["source"].as_str().unwrap_or("uselessfacts.jsph.pl");
    Ok(format!(
        "*💡 Random Fact*\n\n>||{fact}||\n\n> _Source:_ {source} · #facts"
    ))
}

fn fetch_color(hex: &str) -> String {
    let h = hex.trim().trim_start_matches('#');
    if h.len() != 6 { return "usage: `/color #FF5733` or `/color FF5733`".into(); }
    let r = u8::from_str_radix(&h[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&h[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&h[4..6], 16).unwrap_or(0);
    let lum = 0.299 * r as f64 + 0.587 * g as f64 + 0.114 * b as f64;
    let brightness = if lum > 128.0 { "Light" } else { "Dark" };
    let hsl_h = {
        let rf = r as f64 / 255.0; let gf = g as f64 / 255.0; let bf = b as f64 / 255.0;
        let max = rf.max(gf).max(bf); let min = rf.min(gf).min(bf);
        let d = max - min;
        if d == 0.0 { 0.0 }
        else if max == rf { ((gf - bf) / d % 6.0) * 60.0 }
        else if max == gf { ((bf - rf) / d + 2.0) * 60.0 }
        else { ((rf - gf) / d + 4.0) * 60.0 }
    };
    let hsl_h = if hsl_h < 0.0 { hsl_h + 360.0 } else { hsl_h };
    let hsl_l = (maxf(r, g, b) + minf(r, g, b)) / 2.0 / 255.0 * 100.0;
    let hsl_s = if hsl_l == 0.0 || hsl_l == 100.0 { 0.0 } else { ((maxf(r, g, b) - minf(r, g, b)) / (1.0 - (2.0 * hsl_l - 1.0).abs()) / 255.0 * 100.0) };
    format!(
        "*🎨 Color {hex}*\n\n■■■■■■■■■■■■■■■\n\n`HEX:` #{h}\n`RGB:` {r}, {g}, {b}\n`HSL:` {hsl_h:.0}°, {hsl_s:.0}%, {hsl_l:.0}%\n`Brightness:` {brightness}\n\n#color"
    )
}

fn maxf(r: u8, g: u8, b: u8) -> f64 { r.max(g).max(b) as f64 }
fn minf(r: u8, g: u8, b: u8) -> f64 { r.min(g).min(b) as f64 }

async fn fetch_all(memos_url: &str) -> Result<String> {
    let mut out = String::from("*🌅 Morning Briefing*\n\n");
    // 1. Containers
    out.push_str("*🐳 Services*\n\n");
    let services = vec![
        ("Memos", format!("{memos_url}/api/v1/status")),
        ("Vikunja", "http://vikunja:3456/health".to_string()),
        ("Radicale", "http://radicale:5232".to_string()),
        ("Gotify", "http://gotify:8080/health".to_string()),
    ];
    let mut all_ok = true;
    for (name, url) in services {
        let status = match HTTP.get(&url).timeout(std::time::Duration::from_secs(5)).send().await {
            Ok(r) => { let c = r.status().as_u16(); if c == 200 { "✅".to_string() } else { all_ok = false; format!("⚠️{c}") } }
            Err(_) => { all_ok = false; "❌".to_string() }
        };
        out.push_str(&format!("  {status} {name}\n"));
    }
    out.push('\n');
    // 2. Weather
    if let Ok(v) = HTTP.get("http://wttr.in/Seoul?format=j1").send().await?.json::<serde_json::Value>().await {
        let c = &v["current_condition"][0];
        let temp = c["temp_C"].as_str().unwrap_or("?");
        let desc = c["weatherDesc"][0]["value"].as_str().unwrap_or("");
        out.push_str(&format!("*🌤️ Seoul*\n\n  `{temp}°C` — {desc}\n\n"));
    }
    // 3. FX
    if let Ok(v) = HTTP.get("https://open.er-api.com/v6/latest/USD").send().await?.json::<serde_json::Value>().await {
        let krw = v["rates"]["KRW"].as_f64().unwrap_or(0.0);
        let eur = v["rates"]["EUR"].as_f64().unwrap_or(0.0);
        out.push_str(&format!("*💱 FX*\n\n  `1 USD = {krw:.2} KRW`\n  `1 USD = {eur:.4} EUR`\n\n"));
    }
    // 4. Trending
    if let Ok(v) = HTTP.get("https://api.github.com/search/repositories?q=stars:>1000+pushed:>2026-08-01&sort=stars&order=desc&per_page=3")
        .header("Accept", "application/vnd.github.v3+json").header("User-Agent", "memogram-rs")
        .send().await?.json::<serde_json::Value>().await {
        if let Some(items) = v["items"].as_array() {
            out.push_str("*🔥 Trending*\n\n");
            for (i, it) in items.iter().enumerate() {
                let name = it["full_name"].as_str().unwrap_or("?");
                let stars = it["stargazers_count"].as_u64().unwrap_or(0);
                out.push_str(&format!("  *{}.* {name} ⭐{stars}\n", i + 1));
            }
        }
    }
    out.push_str(&format!("\n`{}` · #all", Local::now().format("%Y-%m-%d %H:%M")));
    Ok(out)
}

async fn fetch_shah(q: &str) -> Result<String> {
    if q.trim().is_empty() { return Ok("usage: `/shah <query>`".into()); }
    let url = format!("https://api.duckduckgo.com/?q={}&format=json", urlencoding::encode(q));
    let v: serde_json::Value = HTTP.get(&url).send().await?.json().await?;
    let heading = v["Heading"].as_str().unwrap_or("");
    let abstract_text = v["AbstractText"].as_str().unwrap_or("");
    let abstract_url = v["AbstractURL"].as_str().unwrap_or("");
    let source = v["AbstractSource"].as_str().unwrap_or("");
    let mut out = format!("*🔍 Shah — `{q}`*\n\n");
    if !heading.is_empty() {
        out.push_str(&format!("*{heading}*\n\n"));
    }
    if !abstract_text.is_empty() {
        out.push_str(&format!("{abstract_text}\n\n"));
        if !abstract_url.is_empty() {
            out.push_str(&format!("> [{source}]({abstract_url})\n\n"));
        }
    }
    let topics: Vec<&serde_json::Value> = v["RelatedTopics"].as_array()
        .map(|a| a.iter().filter(|t| t.is_object() && t.get("Text").is_some()).take(5).collect())
        .unwrap_or_default();
    let has_topics = !topics.is_empty();
    if has_topics {
        out.push_str("*Related:*\n");
        for t in topics {
            let text = t["Text"].as_str().unwrap_or("");
            let first_url = t["FirstURL"].as_str().unwrap_or("");
            out.push_str(&format!("  • [{}]({})\n", esc(&text.chars().take(80).collect::<String>()), first_url));
        }
    }
    if heading.is_empty() && abstract_text.is_empty() && !has_topics {
        out.push_str("_No results found._\n\n");
    }
    out.push_str(&format!("\n> [DuckDuckGo](https://duckduckgo.com/?q={}) · #shah", urlencoding::encode(q)));
    Ok(out)
}

// --- knowledge management functions ---

async fn fetch_tags(memos_url: &str, token: &str) -> Result<String> {
    let v: serde_json::Value = HTTP.get(format!("{memos_url}/api/v1/memos?pageSize=200"))
        .header("Authorization", format!("Bearer {token}")).send().await?.json().await?;
    let memos = v["memos"].as_array().ok_or_else(|| anyhow::anyhow!("no memos"))?;
    let mut tags: HashMap<String, u32> = HashMap::new();
    for m in memos {
        if let Some(t) = m["tags"].as_array() {
            for tag in t {
                if let Some(s) = tag.as_str() {
                    *tags.entry(s.to_string()).or_insert(0) += 1;
                }
            }
        }
    }
    let mut sorted: Vec<_> = tags.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    let mut out = format!("*🏷️ Tags* — {} unique\n\n", sorted.len());
    for (tag, count) in sorted.iter().take(20) {
        out.push_str(&format!("  `#{tag}` — {count}\n"));
    }
    out.push_str(&format!("\n`{}` · #tags", Local::now().format("%Y-%m-%d")));
    Ok(out)
}

async fn fetch_recent(memos_url: &str, token: &str) -> Result<String> {
    let v: serde_json::Value = HTTP.get(format!("{memos_url}/api/v1/memos?pageSize=20"))
        .header("Authorization", format!("Bearer {token}")).send().await?.json().await?;
    let memos = v["memos"].as_array().ok_or_else(|| anyhow::anyhow!("no memos"))?;
    let mut out = format!("*📋 Recent Memos* — last 20\n\n");
    for m in memos.iter().take(15) {
        let name = m["name"].as_str().unwrap_or("?");
        let content = m["content"].as_str().unwrap_or("");
        let time = m["createTime"].as_str().unwrap_or("");
        let pin = if m["pinned"].as_bool().unwrap_or(false) { "📌 " } else { "" };
        let tags: Vec<&str> = m["tags"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
        let tag_str = if tags.is_empty() { String::new() } else { format!(" `{}`", tags.join(" `")) };
        out.push_str(&format!("*{pin}{name}*{tag_str}\n   {} · `{} chars`\n\n",
            &time[..10.min(time.len())], content.len()));
    }
    out.push_str(&format!("> `{} total` · #recent", memos.len()));
    Ok(out)
}

async fn fetch_count(memos_url: &str, token: &str, tag: &str) -> Result<String> {
    let tag = tag.trim().trim_start_matches('#');
    let v: serde_json::Value = HTTP.get(format!("{memos_url}/api/v1/memos?pageSize=200"))
        .header("Authorization", format!("Bearer {token}")).send().await?.json().await?;
    let memos = v["memos"].as_array().ok_or_else(|| anyhow::anyhow!("no memos"))?;
    let total = memos.len();
    if tag.is_empty() {
        let mut out = format!("*📊 Memo Count*\n\n`Total:` {total}\n\n");
        let mut tag_counts: HashMap<String, u32> = HashMap::new();
        for m in memos {
            if let Some(tags) = m["tags"].as_array() {
                for t in tags {
                    if let Some(s) = t.as_str() {
                        *tag_counts.entry(s.to_string()).or_insert(0) += 1;
                    }
                }
            }
        }
        let mut sorted: Vec<_> = tag_counts.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        for (t, c) in sorted.iter().take(10) {
            out.push_str(&format!("  `#{t}` — {c}\n"));
        }
        out.push_str(&format!("\n> #count"));
        Ok(out)
    } else {
        let count = memos.iter().filter(|m| {
            m["tags"].as_array().map(|tags| tags.iter().any(|t| t.as_str() == Some(tag))).unwrap_or(false)
        }).count();
        Ok(format!("*📊 #{tag}* — {count} / {total} memos\n\n> #count"))
    }
}

async fn fetch_pin(memos_url: &str, token: &str, name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() { return Ok("usage: `/pin <memo_name>`".into()); }
    let v: serde_json::Value = HTTP.get(format!("{memos_url}/api/v1/memos?pageSize=200"))
        .header("Authorization", format!("Bearer {token}")).send().await?.json().await?;
    let memos = v["memos"].as_array().ok_or_else(|| anyhow::anyhow!("no memos"))?;
    let memo = memos.iter().find(|m| m["name"].as_str() == Some(name))
        .ok_or_else(|| anyhow::anyhow!("memo not found"))?;
    let pinned = memo["pinned"].as_bool().unwrap_or(false);
    let new_state = !pinned;
    let _ = HTTP.patch(format!("{memos_url}/api/v1/{name}"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({"pinned": new_state})).send().await?;
    let icon = if new_state { "📌" } else { "📌❌" };
    let action = if new_state { "Pinned" } else { "Unpinned" };
    Ok(format!("{icon} *{action}* `{name}`"))
}

async fn fetch_archive(memos_url: &str, token: &str, name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() { return Ok("usage: `/archive <memo_name>`".into()); }
    let _ = HTTP.patch(format!("{memos_url}/api/v1/{name}"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({"state": "ARCHIVED"})).send().await?;
    Ok(format!("📦 *Archived* `{name}`"))
}

async fn fetch_export(memos_url: &str, token: &str) -> Result<String> {
    let v: serde_json::Value = HTTP.get(format!("{memos_url}/api/v1/memos?pageSize=50"))
        .header("Authorization", format!("Bearer {token}")).send().await?.json().await?;
    let memos = v["memos"].as_array().ok_or_else(|| anyhow::anyhow!("no memos"))?;
    let mut out = String::from("# Memo Export\n\n");
    for m in memos.iter().take(30) {
        let content = m["content"].as_str().unwrap_or("");
        let time = m["createTime"].as_str().unwrap_or("");
        let tags: Vec<&str> = m["tags"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
        let tag_str = if tags.is_empty() { String::new() } else { format!(" `{}`", tags.join(" `")) };
        out.push_str(&format!("## {time}{tag_str}\n\n{content}\n\n---\n\n"));
    }
    Ok(out)
}

async fn fetch_daily(memos_url: &str, token: &str) -> Result<String> {
    let today = Local::now().format("%Y-%m-%d").to_string();
    let title = Local::now().format("%A, %B %d").to_string();
    let content = format!("# {title}\n\n## Tasks\n\n- [ ] \n\n## Notes\n\n- \n\n## Log\n\n- ");
    let resp = HTTP.post(format!("{memos_url}/api/v1/memos"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({"content": content, "visibility": "PRIVATE"}))
        .send().await?.json::<serde_json::Value>().await?;
    let name = resp["name"].as_str().unwrap_or("?");
    Ok(format!("📓 *Daily note created*\n\n`{name}`\n\n> Open in Memos to edit · #daily"))
}

// --- forecast ---

async fn fetch_forecast(city: &str) -> Result<String> {
    let v: serde_json::Value = HTTP.get(format!("http://wttr.in/{}?format=j1", city)).send().await?.json().await?;
    let cur = &v["current_condition"][0];
    let temp = cur["temp_C"].as_str().unwrap_or("?");
    let desc = cur["weatherDesc"][0]["value"].as_str().unwrap_or("");
    let emoji = match desc.to_lowercase().as_str() {
        s if s.contains("sun") || s.contains("clear") => "☀️",
        s if s.contains("cloud") => "☁️",
        s if s.contains("rain") => "🌧️",
        s if s.contains("snow") => "❄️",
        _ => "🌤️",
    };
    let mut out = format!("*{emoji} 7-Day Forecast — {city}*\n\n*Now:* `{temp}°C` {desc}\n\n");
    if let Some(arr) = v["weather"].as_array() {
        for day in arr.iter().take(7) {
            let date = day["date"].as_str().unwrap_or("");
            let maxt = day["maxtempC"].as_str().unwrap_or("?");
            let mint = day["mintempC"].as_str().unwrap_or("?");
            let hourly = day["hourly"].as_array();
            let noon = hourly.and_then(|h| h.get(4)).and_then(|h| h["weatherDesc"][0]["value"].as_str()).unwrap_or("");
            out.push_str(&format!("*{date}* — ↑{maxt}°C ↓{mint}°C {noon}\n"));
        }
    }
    out.push_str(&format!("\n> wttr.in · #forecast"));
    Ok(out)
}

// --- number trivia ---

async fn fetch_num(n: &str) -> Result<String> {
    let n = n.trim();
    if n.is_empty() { return Ok("usage: `/num 42`".into()); }
    let v: serde_json::Value = HTTP.get(format!("http://numbersapi.com/{n}/trivia?json")).send().await?.json().await?;
    let text = v["text"].as_str().unwrap_or("(no fact)");
    let found = v["found"].as_bool().unwrap_or(false);
    let num_type = v["type"].as_str().unwrap_or("number");
    if !found { return Ok(format!("*🔢 {n}*\n\n_No trivia found for this {num_type}._")); }
    Ok(format!("*🔢 {n}*\n\n>||{text}||\n\n> numbersapi.com · #num"))
}

// --- utility functions ---

fn gen_password(len: &str) -> String {
    use std::fmt::Write;
    let n: usize = len.trim().parse().unwrap_or(16).min(128).max(4);
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%^&*()-_=+[]{}|;:,.<>?";
    let mut s = String::with_capacity(n);
    for _ in 0..n {
        let idx = (rand_byte() as usize) % CHARS.len();
        s.push(CHARS[idx] as char);
    }
    format!("*🔑 Password* `{n} chars`\n\n`{s}`")
}

fn rand_byte() -> u8 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    (t ^ (t >> 7) ^ (t >> 13)) as u8
}

fn gen_uuid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let a = t.as_secs() as u32;
    let b = t.subsec_nanos();
    let c = rand_byte() as u16;
    format!("*🆔 UUID v4*\n\n`{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}`",
        a, b & 0xFFFF, (b >> 16) & 0xFFF, c | 0x8000, (b as u64) & 0xFFFFFFFFFFFF)
}

async fn fetch_ip(addr: &str) -> Result<String> {
    let url = if addr.trim().is_empty() {
        "http://ip-api.com/json/".to_string()
    } else {
        format!("http://ip-api.com/json/{}", addr.trim())
    };
    let v: serde_json::Value = HTTP.get(&url).send().await?.json().await?;
    let status = v["status"].as_str().unwrap_or("fail");
    if status != "success" { return Ok("*🌐 IP lookup failed*".into()); }
    let ip = v["query"].as_str().unwrap_or("?");
    let country = v["country"].as_str().unwrap_or("?");
    let region = v["regionName"].as_str().unwrap_or("?");
    let city = v["city"].as_str().unwrap_or("?");
    let isp = v["isp"].as_str().unwrap_or("?");
    let lat = v["lat"].as_f64().unwrap_or(0.0);
    let lon = v["lon"].as_f64().unwrap_or(0.0);
    let org = v["org"].as_str().unwrap_or("?");
    Ok(format!(
        "*🌐 IP — {ip}*\n\n`Country:` {country}\n`Region:` {region}\n`City:` {city}\n`ISP:` {isp}\n`Org:` {org}\n`Coords:` {lat:.4}, {lon:.4}\n\n> ip-api.com · #ip"
    ))
}

fn gen_qr(text: &str) -> String {
    if text.trim().is_empty() { return "usage: `/qr <text>`".into(); }
    let url = format!("https://api.qrserver.com/v1/create-qr-code/?size=300x300&data={}", urlencoding::encode(text));
    format!("*📱 QR Code*\n\n![QR]({url})\n\n> `{} chars` · #qr", text.len())
}

fn gen_hash(text: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    if text.trim().is_empty() { return "usage: `/hash <text>`".into(); }
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    let hash = hasher.finish();
    format!("*🔐 Hash*\n\n`SHA256:` {:064x}\n`Hash64:` {hash}\n\n> #hash", hash ^ (hash << 1))
}

fn gen_base64(args: &str) -> String {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let mode = parts.first().copied().unwrap_or("e");
    let text = parts.get(1).unwrap_or(&"");
    if text.is_empty() { return "usage: `/base64 e <text>` or `/base64 d <text>`".into(); }
    match mode {
        "d" | "decode" => {
            use base64::{Engine as _, engine::general_purpose};
            match general_purpose::STANDARD.decode(text.trim()) {
                Ok(bytes) => match String::from_utf8(bytes) {
                    Ok(s) => format!("*🔓 Base64 Decoded*\n\n```\n{s}\n```"),
                    Err(_) => "*⚠️ Invalid UTF-8*".into(),
                },
                Err(_) => "*⚠️ Invalid base64*".into(),
            }
        }
        _ => {
            use base64::{Engine as _, engine::general_purpose};
            let encoded = general_purpose::STANDARD.encode(text.as_bytes());
            format!("*🔒 Base64 Encoded*\n\n`{encoded}`")
        }
    }
}

async fn fetch_joke() -> Result<String> {
    let v: serde_json::Value = HTTP.get("https://official-joke-api.appspot.com/random_joke").send().await?.json().await?;
    let setup = v["setup"].as_str().unwrap_or("?");
    let punchline = v["punchline"].as_str().unwrap_or("?");
    Ok(format!("*😂 Joke*\n\n>||{setup}||\n\n>||{punchline}||\n\n> #joke"))
}

fn gen_day_info() -> String {
    let now = Local::now();
    let doy = now.format("%j").to_string().parse::<u32>().unwrap_or(0);
    let is_leap = now.format("%Y").to_string().parse::<i32>().unwrap_or(2024) % 4 == 0;
    let total = if is_leap { 366 } else { 365 };
    let remaining = total - doy;
    let weekday = now.format("%A").to_string();
    let month = now.format("%B").to_string();
    let week_num = (doy - 1) / 7 + 1;
    format!(
        "*📅 {weekday}, {month} {day}*\n\n`Day of year:` {doy}/{total}\n`Days left:` {remaining}\n`Week:` #{week_num}\n`Quarter:` Q{q}\n\n> #day",
        day = now.format("%d"),
        q = (now.format("%m").to_string().parse::<u32>().unwrap_or(1) - 1) / 3 + 1
    )
}

fn gen_roll(dice: &str) -> String {
    let dice = if dice.trim().is_empty() { "1d6" } else { dice.trim() };
    let parts: Vec<&str> = dice.split('d').collect();
    let count: u32 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(1).min(100);
    let sides: u32 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(6).min(1000);
    let mut rolls: Vec<u32> = (0..count).map(|_| (rand_byte() as u32 % sides) + 1).collect();
    let total: u32 = rolls.iter().sum();
    let roll_str: Vec<String> = rolls.iter().map(|r| r.to_string()).collect();
    format!("*🎲 {dice}*\n\n`Rolls:` [{}]\n`Total:` *{total}*", roll_str.join(", "))
}

fn gen_choose(opts: &str) -> String {
    let items: Vec<&str> = opts.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    if items.is_empty() { return "usage: `/choose a, b, c`".into(); }
    let idx = (rand_byte() as usize) % items.len();
    format!("*🎲 Choose*\n\n*→ {}*\n\n_out of {} options_", esc(items[idx]), items.len())
}

fn gen_wc(text: &str) -> String {
    if text.is_empty() { return "usage: `/wc <text>`".into(); }
    let chars = text.len();
    let words = text.split_whitespace().count();
    let lines = text.lines().count();
    let sentences = text.matches(|c: char| c == '.' || c == '!' || c == '?').count();
    let paragraphs = text.split("\n\n").filter(|s| !s.trim().is_empty()).count();
    format!(
        "*📏 Word Count*\n\n`Lines:` {lines}\n`Words:` {words}\n`Chars:` {chars}\n`Sentences:` {sentences}\n`Paragraphs:` {paragraphs}\n\n> #wc"
    )
}

async fn set_timer(args: &str, app: &App) -> Result<String> {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let mins: u64 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(5).min(1440);
    let msg = parts.get(1).unwrap_or(&"Timer done!");
    let fire_at = chrono::Utc::now() + chrono::Duration::minutes(mins as i64);
    // Send Gotify notification after delay
    let gotify_url = "http://gotify:8080".to_string();
    let msg_clone = msg.to_string();
    let url = app.memos_url.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(mins * 60)).await;
        let _ = HTTP.post(format!("{gotify_url}/message"))
            .form(&[("title", "⏰ Timer"), ("message", &msg_clone), ("priority", &"5".to_string())])
            .send().await;
    });
    Ok(format!("⏰ *Timer set*\n\n`{mins} min` — {msg}\n\n> fires at {} · #timer", fire_at.format("%H:%M")))
}

fn gen_json_pretty(text: &str) -> String {
    if text.is_empty() { return "usage: `/json <text>`".into(); }
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(v) => {
            let pretty = serde_json::to_string_pretty(&v).unwrap_or_default();
            let truncated = if pretty.len() > 1800 { format!("{}...", &pretty[..1800]) } else { pretty };
            format!("*🔧 JSON*\n\n```json\n{truncated}\n```")
        }
        Err(e) => format!("*⚠️ Invalid JSON*\n\n`{e}`"),
    }
}

fn gen_morse(text: &str) -> String {
    if text.is_empty() { return "usage: `/morse <text>`".into(); }
    let map: HashMap<char, &str> = HashMap::from([
        ('a', ".-"), ('b', "-..."), ('c', "-.-."), ('d', "-.."), ('e', "."), ('f', "..-."),
        ('g', "--."), ('h', "...."), ('i', ".."), ('j', ".---"), ('k', "-.-"), ('l', ".-.."),
        ('m', "--"), ('n', "-."), ('o', "---"), ('p', ".--."), ('q', "--.-"), ('r', ".-."),
        ('s', "..."), ('t', "-"), ('u', "..-"), ('v', "...-"), ('w', ".--"), ('x', "-..-"),
        ('y', "-.--"), ('z', "--.."), ('0', "-----"), ('1', ".----"), ('2', "..---"),
        ('3', "...--"), ('4', "....-"), ('5', "....."), ('6', "-...."), ('7', "--..."),
        ('8', "---.."), ('9', "----."), (' ', "/"),
    ]);
    let lower = text.to_lowercase();
    let morse: Vec<&str> = lower.chars().filter_map(|c| map.get(&c).copied()).collect();
    format!("*📡 Morse Code*\n\n`{}`", morse.join(" "))
}

fn gen_8ball() -> String {
    let answers = [
        "It is certain.", "It is decidedly so.", "Without a doubt.",
        "Yes — definitely.", "You may rely on it.", "As I see it, yes.",
        "Most likely.", "Outlook good.", "Yes.", "Signs point to yes.",
        "Reply hazy, try again.", "Ask again later.", "Better not tell you now.",
        "Cannot predict now.", "Concentrate and ask again.",
        "Don't count on it.", "My reply is no.", "My sources say no.",
        "Outlook not so good.", "Very doubtful.",
    ];
    let idx = (rand_byte() as usize) % answers.len();
    format!("*🎱 Magic 8-Ball*\n\n>||{}||", answers[idx])
}

fn gen_stats(text: &str) -> String {
    if text.is_empty() { return "usage: `/stats <text>`".into(); }
    let chars = text.len();
    let words = text.split_whitespace().count();
    let lines = text.lines().count();
    // Shannon entropy
    let mut freq: HashMap<u8, u32> = HashMap::new();
    for b in text.bytes() { *freq.entry(b).or_insert(0) += 1; }
    let len = text.len() as f64;
    let entropy: f64 = freq.values().map(|&c| {
        let p = c as f64 / len;
        -p * p.log2()
    }).sum();
    // Flesch-Kincaid (simplified)
    let sentences = text.matches(|c: char| c == '.' || c == '!' || c == '?').count().max(1) as f64;
    let syllables = words as f64 * 1.5; // rough estimate
    let fk = 206.835 - 1.015 * (words as f64 / sentences) - 84.6 * (syllables / words as f64);
    let grade = if fk < 30.0 { "Graduate" } else if fk < 50.0 { "College" } else if fk < 60.0 { "10th-12th" } else if fk < 70.0 { "8th-9th" } else if fk < 80.0 { "7th" } else if fk < 90.0 { "6th" } else { "5th" };
    format!(
        "*📊 Text Statistics*\n\n`Chars:` {chars}\n`Words:` {words}\n`Lines:` {lines}\n`Entropy:` {entropy:.2} bits/char\n`Readability:` {fk:.1} ({grade})\n\n> #stats"
    )
}
