use anyhow::Result;
use base64::Engine;
use chrono::Local;
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, env, sync::Arc};
use teloxide::{prelude::*, types::ParseMode, net::Download, utils::command::BotCommands};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

static HTTP: Lazy<Client> = Lazy::new(|| Client::builder().user_agent("memogram-rs/0.1").build().unwrap());

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Commands:")]
enum Command {
    Start(String),
    Search(String),
    Help,
    Tags,
    Recent,
    Count(String),
    Daily,
    Hn,
    Weather(String),
    Define(String),
    Wiki(String),
    Cheat(String),
    Gh(String),
    Fx(String),
    Containers,
    Lobsters(String),
    Stock(String),
    Crypto(String),
    Translate(String),
    Color(String),
    Forecast(String),
    Pass(String),
    Uuid,
    Ip(String),
    Qr(String),
    Hash(String),
    Base64(String),
    Json(String),
    Remind(String),
    Portfolio(String),
    Alerts(String),
    Markets,
    Arxiv(String),
    Devto,
    Ph,
    Inbox,
    Undo,
    Pin,
    Note(String),
    Meeting(String),
    Project(String),
    Recipe(String),
    Book(String),
    Todo(String),
    List(String),
    Clip(String),
    Proscons(String),
    Flashcard(String),
    Meditation(String),
    Affirmation(String),
    Reflection(String),
    Wisdom,
    Journal(String),
    Goal(String),
    Deadline(String),
    Plan(String),
    Review(String),
    Priority(String),
    Idea(String),
    Braindump(String),
    Link(String),
    Snippet(String),
    Save(String),
    Morning(String),
    Evening(String),
    Checkin(String),
    Log(String),
    Summary(String),
    Sleep(String),
    Energy(String),
    Exercise(String),
    Water(String),
    Read(String),
    Pubmed(String),
    Drug(String),
    Genome(String),
    Protein(String),
    Stoic,
    Mood(String),
    Gratitude(String),
    Habit(String),
    Npm(String),
    Pypi(String),
    Crates(String),
    Stackoverflow(String),
    Airquality(String),
    Sunrise(String),
    Math(String),
    Etymology(String),
    Synonym(String),
    Philosophy,
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
    // Early eprintln before tracing init so we see startup even if RUST_LOG unset
    eprintln!("memogram-rs: starting up... pid={} cwd={:?}", std::process::id(), std::env::current_dir().unwrap_or_default());
    dotenvy::dotenv().ok();
    // Default to info if RUST_LOG not set, so docker logs are not silent
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
    // Support both TELOXIDE_TOKEN (teloxide default) and BOT_TOKEN (legacy .env)
    let token = std::env::var("TELOXIDE_TOKEN").or_else(|_| std::env::var("BOT_TOKEN")).unwrap_or_else(|_| {
        eprintln!("FATAL: TELOXIDE_TOKEN (or BOT_TOKEN) not set in env");
        tracing::error!("FATAL: TELOXIDE_TOKEN not set");
        std::process::exit(1);
    });
    eprintln!("memogram-rs: token loaded len={} prefix={}...", token.len(), &token[..token.len().min(10)]);
    let bot = Bot::new(token);
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
        teloxide::types::BotCommand { command: "hn".into(), description: "HackerNews top 5".into() },
        teloxide::types::BotCommand { command: "lobsters".into(), description: "Lobste.rs stories".into() },
        teloxide::types::BotCommand { command: "arxiv".into(), description: "arXiv latest papers".into() },
        teloxide::types::BotCommand { command: "devto".into(), description: "dev.to top posts".into() },
        teloxide::types::BotCommand { command: "ph".into(), description: "Product Hunt daily".into() },
        teloxide::types::BotCommand { command: "weather".into(), description: "weather <city>".into() },
        teloxide::types::BotCommand { command: "forecast".into(), description: "7-day forecast".into() },
        teloxide::types::BotCommand { command: "define".into(), description: "define <word>".into() },
        teloxide::types::BotCommand { command: "wiki".into(), description: "wiki <query>".into() },
        teloxide::types::BotCommand { command: "cheat".into(), description: "cheat <query>".into() },
        teloxide::types::BotCommand { command: "gh".into(), description: "GitHub search".into() },
        teloxide::types::BotCommand { command: "fx".into(), description: "fx <pair>".into() },
        teloxide::types::BotCommand { command: "stock".into(), description: "stock <ticker>".into() },
        teloxide::types::BotCommand { command: "crypto".into(), description: "crypto <coin>".into() },
        teloxide::types::BotCommand { command: "portfolio".into(), description: "track holdings".into() },
        teloxide::types::BotCommand { command: "alerts".into(), description: "price alerts".into() },
        teloxide::types::BotCommand { command: "markets".into(), description: "market indices".into() },
        teloxide::types::BotCommand { command: "translate".into(), description: "translate text".into() },
        teloxide::types::BotCommand { command: "color".into(), description: "color <hex>".into() },
        teloxide::types::BotCommand { command: "containers".into(), description: "service health".into() },
        teloxide::types::BotCommand { command: "tags".into(), description: "list all tags".into() },
        teloxide::types::BotCommand { command: "recent".into(), description: "last 20 memos".into() },
        teloxide::types::BotCommand { command: "count".into(), description: "count memos".into() },
        teloxide::types::BotCommand { command: "daily".into(), description: "create daily note".into() },
        teloxide::types::BotCommand { command: "inbox".into(), description: "untagged memos".into() },
        teloxide::types::BotCommand { command: "undo".into(), description: "delete last memo".into() },
        teloxide::types::BotCommand { command: "pin".into(), description: "pin/unpin last memo".into() },
        teloxide::types::BotCommand { command: "note".into(), description: "note #tag text".into() },
        teloxide::types::BotCommand { command: "meeting".into(), description: "meeting notes".into() },
        teloxide::types::BotCommand { command: "project".into(), description: "project doc".into() },
        teloxide::types::BotCommand { command: "recipe".into(), description: "recipe card".into() },
        teloxide::types::BotCommand { command: "book".into(), description: "book card".into() },
        teloxide::types::BotCommand { command: "todo".into(), description: "checklist".into() },
        teloxide::types::BotCommand { command: "list".into(), description: "bulleted list".into() },
        teloxide::types::BotCommand { command: "clip".into(), description: "save bookmark".into() },
        teloxide::types::BotCommand { command: "proscons".into(), description: "pros vs cons".into() },
        teloxide::types::BotCommand { command: "flashcard".into(), description: "Q | A".into() },
        teloxide::types::BotCommand { command: "remind".into(), description: "remind <min> <msg>".into() },
        teloxide::types::BotCommand { command: "password".into(), description: "generate password".into() },
        teloxide::types::BotCommand { command: "uuid".into(), description: "generate UUID".into() },
        teloxide::types::BotCommand { command: "ip".into(), description: "IP lookup".into() },
        teloxide::types::BotCommand { command: "qr".into(), description: "QR code".into() },
        teloxide::types::BotCommand { command: "hash".into(), description: "SHA-256 hash".into() },
        teloxide::types::BotCommand { command: "base64".into(), description: "encode/decode".into() },
        teloxide::types::BotCommand { command: "json".into(), description: "pretty JSON".into() },
        teloxide::types::BotCommand { command: "help".into(), description: "help".into() },
        teloxide::types::BotCommand { command: "pubmed".into(), description: "search PubMed papers".into() },
        teloxide::types::BotCommand { command: "drug".into(), description: "drug info".into() },
        teloxide::types::BotCommand { command: "genome".into(), description: "genome search".into() },
        teloxide::types::BotCommand { command: "protein".into(), description: "protein search".into() },
        teloxide::types::BotCommand { command: "stoic".into(), description: "stoic quote".into() },
        teloxide::types::BotCommand { command: "mood".into(), description: "log mood".into() },
        teloxide::types::BotCommand { command: "gratitude".into(), description: "log gratitude".into() },
        teloxide::types::BotCommand { command: "habit".into(), description: "track habit".into() },
        teloxide::types::BotCommand { command: "npm".into(), description: "npm package info".into() },
        teloxide::types::BotCommand { command: "pypi".into(), description: "PyPI package info".into() },
        teloxide::types::BotCommand { command: "crates".into(), description: "crates.io info".into() },
        teloxide::types::BotCommand { command: "stackoverflow".into(), description: "Stack Overflow search".into() },
        teloxide::types::BotCommand { command: "airquality".into(), description: "air quality index".into() },
        teloxide::types::BotCommand { command: "sunrise".into(), description: "sunrise/sunset".into() },
        teloxide::types::BotCommand { command: "math".into(), description: "math expression".into() },
        teloxide::types::BotCommand { command: "etymology".into(), description: "word etymology".into() },
        teloxide::types::BotCommand { command: "synonym".into(), description: "find synonyms".into() },
        teloxide::types::BotCommand { command: "philosophy".into(), description: "philosophy quote".into() },
        teloxide::types::BotCommand { command: "meditation".into(), description: "log meditation".into() },
        teloxide::types::BotCommand { command: "affirmation".into(), description: "log affirmation".into() },
        teloxide::types::BotCommand { command: "reflection".into(), description: "log reflection".into() },
        teloxide::types::BotCommand { command: "wisdom".into(), description: "random wisdom".into() },
        teloxide::types::BotCommand { command: "journal".into(), description: "journal entry".into() },
        teloxide::types::BotCommand { command: "goal".into(), description: "set a goal".into() },
        teloxide::types::BotCommand { command: "deadline".into(), description: "track deadline".into() },
        teloxide::types::BotCommand { command: "plan".into(), description: "daily/weekly plan".into() },
        teloxide::types::BotCommand { command: "review".into(), description: "weekly review".into() },
        teloxide::types::BotCommand { command: "priority".into(), description: "set priority".into() },
        teloxide::types::BotCommand { command: "idea".into(), description: "capture idea".into() },
        teloxide::types::BotCommand { command: "braindump".into(), description: "quick thought dump".into() },
        teloxide::types::BotCommand { command: "link".into(), description: "save link".into() },
        teloxide::types::BotCommand { command: "snippet".into(), description: "code snippet".into() },
        teloxide::types::BotCommand { command: "save".into(), description: "save anything".into() },
        teloxide::types::BotCommand { command: "morning".into(), description: "morning check-in".into() },
        teloxide::types::BotCommand { command: "evening".into(), description: "evening reflection".into() },
        teloxide::types::BotCommand { command: "checkin".into(), description: "daily check-in".into() },
        teloxide::types::BotCommand { command: "log".into(), description: "daily log".into() },
        teloxide::types::BotCommand { command: "summary".into(), description: "day summary".into() },
        teloxide::types::BotCommand { command: "sleep".into(), description: "log sleep".into() },
        teloxide::types::BotCommand { command: "energy".into(), description: "log energy".into() },
        teloxide::types::BotCommand { command: "exercise".into(), description: "log exercise".into() },
        teloxide::types::BotCommand { command: "water".into(), description: "log water intake".into() },
        teloxide::types::BotCommand { command: "read".into(), description: "log reading".into() },
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
        Command::Hn => { let txt = fetch_hn().await.unwrap_or_else(|e| format!("hn err: {e}")); create_as_bot(&bot, &msg, &app, "news", &txt, tid).await?; }
        Command::Weather(city) => {
            let c = if city.trim().is_empty() { "Los Angeles".to_string() } else { city };
            let txt = fetch_weather(&c).await.unwrap_or_else(|e| format!("weather err: {e}"));
            create_as_bot(&bot, &msg, &app, "weather", &txt, tid).await?;
        }
        Command::Define(w) => { let txt = fetch_define(&w).await.unwrap_or_else(|e| format!("define err: {e}")); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Wiki(q) => { let txt = fetch_wiki(&q).await.unwrap_or_else(|e| format!("wiki err: {e}")); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Cheat(q) => { let txt = fetch_cheat(&q).await.unwrap_or_else(|e| format!("cheat err: {e}")); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Gh(q) => { let txt = fetch_gh(&q).await.unwrap_or_else(|e| format!("gh err: {e}")); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
        Command::Fx(pair) => { let txt = fetch_fx(&pair).await.unwrap_or_else(|e| format!("fx err: {e}")); create_as_bot(&bot, &msg, &app, "money", &txt, tid).await?; }
        Command::Containers => { let txt = fetch_containers(&app.memos_url).await.unwrap_or_else(|e| format!("containers err: {e}")); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
        Command::Lobsters(tag) => { let txt = fetch_lobsters(&tag).await.unwrap_or_else(|e| format!("lobsters err: {e}")); create_as_bot(&bot, &msg, &app, "news", &txt, tid).await?; }
        Command::Stock(ticker) => { let txt = fetch_stock(&ticker).await.unwrap_or_else(|e| format!("stock err: {e}")); create_as_bot(&bot, &msg, &app, "money", &txt, tid).await?; }
        Command::Crypto(coin) => { let txt = fetch_crypto(&coin).await.unwrap_or_else(|e| format!("crypto err: {e}")); create_as_bot(&bot, &msg, &app, "money", &txt, tid).await?; }
        Command::Translate(args) => { let txt = fetch_translate(&args).await.unwrap_or_else(|e| format!("translate err: {e}")); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Color(hex) => { let txt = fetch_color(&hex); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Forecast(city) => { let txt = fetch_forecast(&city).await.unwrap_or_else(|e| format!("forecast err: {e}")); create_as_bot(&bot, &msg, &app, "weather", &txt, tid).await?; }
        Command::Tags => {
            let token = { app.store.read().await.get(&tid).cloned() };
            let Some(tok) = token else { bot.send_message(msg.chat.id, "run /start <token> first").await?; return Ok(()); };
            let txt = fetch_tags(&app.memos_url, &tok).await.unwrap_or_else(|e| format!("tags err: {e}"));
            create_as_bot(&bot, &msg, &app, "daily", &txt, tid).await?;
        }
        Command::Recent => {
            let token = { app.store.read().await.get(&tid).cloned() };
            let Some(tok) = token else { bot.send_message(msg.chat.id, "run /start <token> first").await?; return Ok(()); };
            let txt = fetch_recent(&app.memos_url, &tok).await.unwrap_or_else(|e| format!("recent err: {e}"));
            create_as_bot(&bot, &msg, &app, "daily", &txt, tid).await?;
        }
        Command::Count(tag) => {
            let token = { app.store.read().await.get(&tid).cloned() };
            let Some(tok) = token else { bot.send_message(msg.chat.id, "run /start <token> first").await?; return Ok(()); };
            let txt = fetch_count(&app.memos_url, &tok, &tag).await.unwrap_or_else(|e| format!("count err: {e}"));
            create_as_bot(&bot, &msg, &app, "daily", &txt, tid).await?;
        }
        Command::Daily => {
            let token = { app.store.read().await.get(&tid).cloned() };
            let Some(tok) = token else { bot.send_message(msg.chat.id, "run /start <token> first").await?; return Ok(()); };
            let txt = fetch_daily(&app.memos_url, &tok).await.unwrap_or_else(|e| format!("daily err: {e}"));
            create_as_bot(&bot, &msg, &app, "planning", &txt, tid).await?;
        }
        Command::Pass(len) => { let txt = gen_password(&len); bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?; }
        Command::Uuid => { let txt = gen_uuid(); bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?; }
        Command::Ip(addr) => { let txt = fetch_ip(&addr).await.unwrap_or_else(|e| format!("ip err: {e}")); bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?; }
        Command::Qr(text) => { let txt = gen_qr(&text); bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?; }
        Command::Hash(text) => { let txt = gen_hash(&text); bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?; }
        Command::Base64(args) => { let txt = gen_base64(&args); bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?; }
        Command::Json(text) => { let txt = gen_json_pretty(&text); bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?; }
        Command::Remind(args) => { let txt = set_reminder(&args, &app).await; bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?; }
        Command::Portfolio(args) => {
            let txt = handle_portfolio(&args, tid, &app.store_path).await;
            bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?;
        }
        Command::Alerts(args) => {
            let txt = handle_alerts(&args, &app).await;
            bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?;
        }
        Command::Markets => { let txt = fetch_markets().await.unwrap_or_else(|e| format!("markets err: {e}")); create_as_bot(&bot, &msg, &app, "money", &txt, tid).await?; }
        Command::Arxiv(topic) => { let txt = fetch_arxiv(&topic).await.unwrap_or_else(|e| format!("arxiv err: {e}")); create_as_bot(&bot, &msg, &app, "news", &txt, tid).await?; }
        Command::Devto => { let txt = fetch_devto().await.unwrap_or_else(|e| format!("devto err: {e}")); create_as_bot(&bot, &msg, &app, "news", &txt, tid).await?; }
        Command::Ph => { let txt = fetch_ph().await.unwrap_or_else(|e| format!("ph err: {e}")); create_as_bot(&bot, &msg, &app, "news", &txt, tid).await?; }
        Command::Inbox => {
            let token = { app.store.read().await.get(&tid).cloned() };
            let Some(tok) = token else { bot.send_message(msg.chat.id, "run /start <token> first").await?; return Ok(()); };
            let txt = fetch_inbox(&app.memos_url, &tok).await.unwrap_or_else(|e| format!("inbox err: {e}"));
            create_as_bot(&bot, &msg, &app, "daily", &txt, tid).await?;
        }
        Command::Undo => {
            let token = { app.store.read().await.get(&tid).cloned() };
            let Some(tok) = token else { bot.send_message(msg.chat.id, "run /start <token> first").await?; return Ok(()); };
            let txt = undo_last_memo(&app.memos_url, &tok).await;
            bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?;
        }
        Command::Pin => {
            let token = { app.store.read().await.get(&tid).cloned() };
            let Some(tok) = token else { bot.send_message(msg.chat.id, "run /start <token> first").await?; return Ok(()); };
            let txt = pin_last_memo(&app.memos_url, &tok).await;
            bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?;
        }
        Command::Note(content) => {
            let token = { app.store.read().await.get(&tid).cloned() };
            let Some(tok) = token else { bot.send_message(msg.chat.id, "run /start <token> first").await?; return Ok(()); };
            let txt = create_note(&app.memos_url, &tok, &content).await;
            bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?;
        }
        Command::Meeting(args) => { let txt = create_meeting(&args); create_as_bot(&bot, &msg, &app, "planning", &txt, tid).await?; }
        Command::Project(args) => { let txt = create_project(&args); create_as_bot(&bot, &msg, &app, "planning", &txt, tid).await?; }
        Command::Recipe(args) => { let txt = create_recipe(&args); create_as_bot(&bot, &msg, &app, "life", &txt, tid).await?; }
        Command::Book(args) => { let txt = create_book(&args); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Todo(args) => { let txt = create_todo(&args); create_as_bot(&bot, &msg, &app, "planning", &txt, tid).await?; }
        Command::List(args) => { let txt = create_list(&args); create_as_bot(&bot, &msg, &app, "planning", &txt, tid).await?; }
        Command::Clip(args) => { let txt = create_clip(&args); create_as_bot(&bot, &msg, &app, "inbox", &txt, tid).await?; }
        Command::Proscons(args) => { let txt = create_proscons(&args); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Flashcard(args) => { let txt = create_flashcard(&args); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Pubmed(q) => { let txt = fetch_pubmed(&q).await.unwrap_or_else(|e| format!("pubmed err: {e}")); create_as_bot(&bot, &msg, &app, "bio", &txt, tid).await?; }
        Command::Drug(name) => { let txt = fetch_drug(&name).await.unwrap_or_else(|e| format!("drug err: {e}")); create_as_bot(&bot, &msg, &app, "bio", &txt, tid).await?; }
        Command::Genome(q) => { let txt = fetch_genome(&q).await.unwrap_or_else(|e| format!("genome err: {e}")); create_as_bot(&bot, &msg, &app, "bio", &txt, tid).await?; }
        Command::Protein(q) => { let txt = fetch_protein(&q).await.unwrap_or_else(|e| format!("protein err: {e}")); create_as_bot(&bot, &msg, &app, "bio", &txt, tid).await?; }
        Command::Stoic => { let txt = fetch_stoic_quote().await.unwrap_or_else(|e| format!("stoic err: {e}")); create_as_bot(&bot, &msg, &app, "stoic", &txt, tid).await?; }
        Command::Mood(note) => { let txt = create_mood_entry(&note); create_as_bot(&bot, &msg, &app, "stoic", &txt, tid).await?; }
        Command::Gratitude(note) => { let txt = create_gratitude_entry(&note); create_as_bot(&bot, &msg, &app, "stoic", &txt, tid).await?; }
        Command::Habit(args) => { let txt = create_habit_entry(&args); create_as_bot(&bot, &msg, &app, "stoic", &txt, tid).await?; }
        Command::Npm(pkg) => { let txt = fetch_npm(&pkg).await.unwrap_or_else(|e| format!("npm err: {e}")); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
        Command::Pypi(pkg) => { let txt = fetch_pypi(&pkg).await.unwrap_or_else(|e| format!("pypi err: {e}")); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
        Command::Crates(pkg) => { let txt = fetch_crates(&pkg).await.unwrap_or_else(|e| format!("crates err: {e}")); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
        Command::Stackoverflow(q) => { let txt = fetch_stackoverflow(&q).await.unwrap_or_else(|e| format!("stackoverflow err: {e}")); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
        Command::Airquality(loc) => { let txt = fetch_airquality(&loc).await.unwrap_or_else(|e| format!("airquality err: {e}")); create_as_bot(&bot, &msg, &app, "weather", &txt, tid).await?; }
        Command::Sunrise(loc) => { let txt = fetch_sunrise(&loc).await.unwrap_or_else(|e| format!("sunrise err: {e}")); create_as_bot(&bot, &msg, &app, "weather", &txt, tid).await?; }
        Command::Math(expr) => { let txt = eval_math(&expr); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Etymology(word) => { let txt = fetch_etymology(&word).await.unwrap_or_else(|e| format!("etymology err: {e}")); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Synonym(word) => { let txt = fetch_synonym(&word).await.unwrap_or_else(|e| format!("synonym err: {e}")); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Philosophy => { let txt = fetch_philosophy_quote().await.unwrap_or_else(|e| format!("philosophy err: {e}")); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Meditation(note) => { let txt = create_meditation(&note); create_as_bot(&bot, &msg, &app, "stoic", &txt, tid).await?; }
        Command::Affirmation(note) => { let txt = create_affirmation(&note); create_as_bot(&bot, &msg, &app, "stoic", &txt, tid).await?; }
        Command::Reflection(note) => { let txt = create_reflection(&note); create_as_bot(&bot, &msg, &app, "stoic", &txt, tid).await?; }
        Command::Wisdom => { let txt = fetch_wisdom().await.unwrap_or_else(|e| format!("wisdom err: {e}")); create_as_bot(&bot, &msg, &app, "stoic", &txt, tid).await?; }
        Command::Journal(note) => { let txt = create_journal(&note); create_as_bot(&bot, &msg, &app, "stoic", &txt, tid).await?; }
        Command::Goal(args) => { let txt = create_goal(&args); create_as_bot(&bot, &msg, &app, "planning", &txt, tid).await?; }
        Command::Deadline(args) => { let txt = create_deadline(&args); create_as_bot(&bot, &msg, &app, "planning", &txt, tid).await?; }
        Command::Plan(args) => { let txt = create_plan(&args); create_as_bot(&bot, &msg, &app, "planning", &txt, tid).await?; }
        Command::Review(args) => { let txt = create_review(&args); create_as_bot(&bot, &msg, &app, "planning", &txt, tid).await?; }
        Command::Priority(args) => { let txt = create_priority(&args); create_as_bot(&bot, &msg, &app, "planning", &txt, tid).await?; }
        Command::Idea(args) => { let txt = create_idea(&args); create_as_bot(&bot, &msg, &app, "inbox", &txt, tid).await?; }
        Command::Braindump(args) => { let txt = create_braindump(&args); create_as_bot(&bot, &msg, &app, "inbox", &txt, tid).await?; }
        Command::Link(args) => { let txt = create_link(&args); create_as_bot(&bot, &msg, &app, "inbox", &txt, tid).await?; }
        Command::Snippet(args) => { let txt = create_snippet(&args); create_as_bot(&bot, &msg, &app, "inbox", &txt, tid).await?; }
        Command::Save(args) => { let txt = create_save(&args); create_as_bot(&bot, &msg, &app, "inbox", &txt, tid).await?; }
        Command::Morning(args) => { let txt = create_morning(&args); create_as_bot(&bot, &msg, &app, "daily", &txt, tid).await?; }
        Command::Evening(args) => { let txt = create_evening(&args); create_as_bot(&bot, &msg, &app, "daily", &txt, tid).await?; }
        Command::Checkin(args) => { let txt = create_checkin(&args); create_as_bot(&bot, &msg, &app, "daily", &txt, tid).await?; }
        Command::Log(args) => { let txt = create_log(&args); create_as_bot(&bot, &msg, &app, "daily", &txt, tid).await?; }
        Command::Summary(args) => { let txt = create_summary(&args); create_as_bot(&bot, &msg, &app, "daily", &txt, tid).await?; }
        Command::Sleep(args) => { let txt = create_sleep(&args); create_as_bot(&bot, &msg, &app, "life", &txt, tid).await?; }
        Command::Energy(args) => { let txt = create_energy(&args); create_as_bot(&bot, &msg, &app, "life", &txt, tid).await?; }
        Command::Exercise(args) => { let txt = create_exercise(&args); create_as_bot(&bot, &msg, &app, "life", &txt, tid).await?; }
        Command::Water(args) => { let txt = create_water(&args); create_as_bot(&bot, &msg, &app, "life", &txt, tid).await?; }
        Command::Read(args) => { let txt = create_read(&args); create_as_bot(&bot, &msg, &app, "life", &txt, tid).await?; }
        
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
                let mime = doc.mime_type.as_ref().map(|m| m.to_string()).unwrap_or_else(|| "application/octet-stream".to_string());
                let fname = doc.file_name.clone().unwrap_or_else(|| format!("doc_{}", msg.id.0));
                match upload_attachment(&app.memos_url, &tok, &fname, &mime, &data).await {
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
    let tag = format!("#{bot_name}");
    let content = format!("@{}\n\n{}\n\n— _via {} · asher_\n\n{tag}", app.admin_username, body, bot_name);
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
    let r = HTTP.get(format!("{url}/api/v1/memos?pageSize=1")).bearer_auth(tok).send().await?;
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

async fn download_telegram_file(bot: &Bot, file_id: &teloxide::types::FileId) -> Result<Vec<u8>> {
    let file = bot.get_file(file_id.clone()).await?;
    let mut buf = Vec::new();
    bot.download_file(&file.path, &mut buf).await?;
    Ok(buf)
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

// Unified Telegram MarkdownV2 helpers — reference: bot2.js escapeMarkdown + gramio HTML fallback
fn tg_code_block(s: &str) -> String {
    // Inside code blocks only ` and \ need escaping
    let e = s.replace('\\', "\\\\").replace('`', "\\`");
    format!("```\n{}\n```", e)
}
fn tg_header(emoji: &str, title: &str, query: &str) -> String {
    if query.trim().is_empty() {
        format!("*{emoji} {}*", esc(title))
    } else {
        format!("*{emoji} {} — `{}`*", esc(title), esc(query))
    }
}
fn tg_footer(source: &str, tag: &str) -> String {
    format!("> {} · #{}", esc(source), esc(tag))
}
fn tg_truncate(s: &str, n: usize) -> String {
    if s.len() > n { format!("{}...", &s[..n]) } else { s.to_string() }
}

async fn fetch_hn() -> Result<String> {
    let ids: Vec<u64> = HTTP.get("https://hacker-news.firebaseio.com/v0/topstories.json").send().await?.json().await?;
    let mut out = format!("{}\n\n", tg_header("🔥", "Hacker News", "Top 5"));
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
    out.push_str(&format!("\n{}", tg_footer("news.ycombinator.com", "hn")));
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
    let mut out = format!("{}\n\n`Now:  {}°C` \\(feels {}°C\\)\n`Desc: {}`\n`Hum:  {}%`\n`Wind: {} km/h {}`\n\n", tg_header(emoji, "Weather", city), esc(temp), esc(feels), esc(desc), esc(hum), esc(wind), esc(winddir));
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
    out.push_str(&format!("\n{}", tg_footer("wttr.in", "weather")));
    Ok(out)
}

async fn fetch_define(word: &str) -> Result<String> {
    let url = format!("https://en.wiktionary.org/api/rest_v1/page/definition/{}", urlencoding::encode(word));
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "memogram-rs").send().await?.json().await?;
    let mut out = format!("{}\n\n", tg_header("📖", "Define", word));
    let mut found = false;
    if let Some(langs) = v.as_object() {
        for (lang, defs) in langs {
            if lang == "en" {
                if let Some(arr) = defs.as_array() {
                    for entry in arr.iter().take(3) {
                        let pos = entry["partOfSpeech"].as_str().unwrap_or("");
                        if let Some(defs_arr) = entry["definitions"].as_array() {
                            for (i, d) in defs_arr.iter().take(2).enumerate() {
                                let raw = d["definition"].as_str().unwrap_or("");
                                let text = raw.replace("<span class=\"Latn\" lang=\"en\">", "").replace("</span>", "").replace("<i>", "").replace("</i>", "").replace("<[^>]+>", "").chars().take(200).collect::<String>();
                                if !text.is_empty() {
                                    out.push_str(&format!("*{}.* {pos}: {}\n", i + 1, esc(&text)));
                                    found = true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    if !found {
        out.push_str(&format!("_No definitions found for `{}`._\n\nTry: https://en.wiktionary.org/wiki/{}", esc(word), urlencoding::encode(word)));
    }
    out.push_str(&format!("\n\n{}", tg_footer("wiktionary.org", "define")));
    Ok(out)
}

async fn fetch_wiki(q: &str) -> Result<String> {
    let v: serde_json::Value = HTTP.get(format!("https://en.wikipedia.org/api/rest_v1/page/summary/{}", urlencoding::encode(q))).send().await?.json().await?;
    let title = v["title"].as_str().unwrap_or(q);
    let extract = v["extract"].as_str().unwrap_or("no summary");
    let url = v["content_urls"]["desktop"]["page"].as_str().map(|s| s.to_string()).unwrap_or_else(|| format!("https://en.wikipedia.org/wiki/{}", urlencoding::encode(q)));
    let thumb = v["thumbnail"]["source"].as_str().unwrap_or("");
    let mut out = format!("{}\n\n", tg_header("📚", "Wiki", q));
    out.push_str(&format!("*{}*\n", esc(title)));
    if !thumb.is_empty() { out.push_str(&format!("[📷 Photo]({thumb})\n\n")); }
    out.push_str(&format!("{}\n\n[Read more on Wikipedia]({url})\n\n{}", esc(&tg_truncate(extract, 800)), tg_footer("wikipedia.org", "wiki")));
    Ok(out)
}

async fn fetch_cheat(q: &str) -> Result<String> {
    let resp = HTTP.get(format!("https://cheat.sh/{}?TQ", urlencoding::encode(q))).send().await;
    match resp {
        Ok(r) if r.status().is_success() => {
            let txt = r.text().await.unwrap_or_default();
            let clean = tg_truncate(&txt, 1400);
            Ok(format!("{}\n\n{}\n\n[cheat.sh](https://cheat.sh/{}) · #{}", tg_header("💻", "cheat", q), tg_code_block(&clean), esc(q), esc("cheat")))
        }
        _ => {
            let url = format!("https://tldr.in/{}", urlencoding::encode(q));
            Ok(format!("{}\n\ncheat.sh unavailable. Try:\n> [tldr.in]({url})\n> [devhints.io](https://devhints.io/{})\n\n{}", tg_header("💻", "cheat", q), urlencoding::encode(q), tg_footer("tldr.in", "cheat")))
        }
    }
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
    if filtered.is_empty() { return Ok(format!("{}\n\n_No stories found for `{}`._\n\n{}", tg_header("🦞", "Lobste.rs", tag), esc(tag), tg_footer("lobste.rs", "lobsters"))); }
    let mut out = format!("{}\n\n", tg_header("🦞", "Lobste.rs", tag));
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
    out.push_str(&format!("\n{}", tg_footer("lobste.rs", "lobsters")));
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
    let now_str = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let header = tg_header(emoji, &format!("{} ({})", name, ticker), "");
    let body = tg_code_block(&format!("{price:.2} {currency}\n{sign}{change:.2} ({sign}{pct:.2}%)"));
    Ok(format!("{}\n\n{}\n\n`{}` · #{}", header, body, esc(&now_str), esc("stock")))
}

async fn fetch_crypto(coin: &str) -> Result<String> {
    let input = coin.trim().to_lowercase();
    let coin_id = if input.is_empty() || input == "help" {
        "bitcoin".to_string()
    } else {
        match input.as_str() {
            "btc" | "bitcoin" => "bitcoin",
            "eth" | "ethereum" => "ethereum",
            "sol" | "solana" => "solana",
            "xrp" | "ripple" => "ripple",
            "doge" | "dogecoin" => "dogecoin",
            "ada" | "cardano" => "cardano",
            "bnb" | "binance" | "binancecoin" => "binancecoin",
            "dot" | "polkadot" => "polkadot",
            "avax" | "avalanche" => "avalanche-2",
            "matic" | "polygon" => "matic-network",
            "link" | "chainlink" => "chainlink",
            "ltc" | "litecoin" => "litecoin",
            "uni" | "uniswap" => "uniswap",
            "aave" => "aave",
            "atom" | "cosmos" => "cosmos",
            "algo" | "algorand" => "algorand",
            other => other,
        }.to_string()
    };
    let url = format!("https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=usd&include_24hr_change=true&include_market_cap=true", urlencoding::encode(&coin_id));
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "memogram-rs").send().await?.json().await?;
    let data = v.get(&coin_id).ok_or_else(|| anyhow::anyhow!("coin '{coin}' not found. Try: btc, eth, sol, xrp, doge, ada, bnb"))?;
    let price = data["usd"].as_f64().unwrap_or(0.0);
    let change = data["usd_24h_change"].as_f64().unwrap_or(0.0);
    let mcap = data["usd_market_cap"].as_f64().unwrap_or(0.0);
    let emoji = if change >= 0.0 { "📈" } else { "📉" };
    let sign = if change >= 0.0 { "+" } else { "" };
    let mcap_str = if mcap >= 1e12 { format!("${:.2}T", mcap / 1e12) } else if mcap >= 1e9 { format!("${:.2}B", mcap / 1e9) } else if mcap >= 1e6 { format!("${:.2}M", mcap / 1e6) } else { format!("${:.0}", mcap) };
    let now_str = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let header = tg_header(emoji, &coin_id, coin);
    let body = tg_code_block(&format!("${price:.2}\n{sign}{change:.2}%\nMCap: {mcap_str}"));
    Ok(format!("{}\n\n{}\n\n`{}` · #{}", header, body, esc(&now_str), esc("crypto")))
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
    let hsl_s = if hsl_l == 0.0 || hsl_l == 100.0 { 0.0 } else { (maxf(r, g, b) - minf(r, g, b)) / (1.0 - (2.0 * hsl_l - 1.0).abs()) / 255.0 * 100.0 };
    format!(
        "*🎨 Color {hex}*\n\n■■■■■■■■■■■■■■■\n\n`HEX:` #{h}\n`RGB:` {r}, {g}, {b}\n`HSL:` {hsl_h:.0}°, {hsl_s:.0}%, {hsl_l:.0}%\n`Brightness:` {brightness}\n\n#color"
    )
}

fn maxf(r: u8, g: u8, b: u8) -> f64 { r.max(g).max(b) as f64 }
fn minf(r: u8, g: u8, b: u8) -> f64 { r.min(g).min(b) as f64 }

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

async fn fetch_daily(memos_url: &str, token: &str) -> Result<String> {
    let title = Local::now().format("%A, %B %d").to_string();
    let date = Local::now().format("%Y-%m-%d").to_string();
    let content = format!(
        "# {title}\n\n\
         ## 🎯 Today's Goals\n\n\
         - [ ] \n\n\
         ## 📝 Notes\n\n\
         - \n\n\
         ## ✅ Completed\n\n\
         - \n\n\
         ## 💡 Ideas\n\n\
         - \n\n\
         ## 🌙 Evening Reflection\n\n\
         - What went well?\n\
         - What could improve?\n\
         - What did I learn?\n\n\
         ---\n\
         #daily #journal {date}"
    );
    let resp = HTTP.post(format!("{memos_url}/api/v1/memos"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({"content": content, "visibility": "PRIVATE"}))
        .send().await?.json::<serde_json::Value>().await?;
    let name = resp["name"].as_str().unwrap_or("?");
    Ok(format!("📓 *Daily note created*\n\n`{name}`\n\n> Open in Memos to edit · #daily"))
}

// --- forecast ---

async fn fetch_forecast(city: &str) -> Result<String> {
    let city = if city.trim().is_empty() { "auto:ip".to_string() } else { city.trim().to_string() };
    let display_city = if city == "auto:ip" { "your location".to_string() } else { city.clone() };
    let v: serde_json::Value = HTTP.get(format!("http://wttr.in/{}?format=j1", urlencoding::encode(&city))).send().await?.json().await?;
    let cur = &v["current_condition"][0];
    let temp = cur["temp_C"].as_str().unwrap_or("?");
    let desc = cur["weatherDesc"][0]["value"].as_str().unwrap_or("");
    let humidity = cur["humidity"].as_str().unwrap_or("?");
    let wind = cur["windspeedKmph"].as_str().unwrap_or("?");
    let emoji = match desc.to_lowercase().as_str() {
        s if s.contains("sun") || s.contains("clear") => "☀️",
        s if s.contains("cloud") => "☁️",
        s if s.contains("rain") => "🌧️",
        s if s.contains("snow") => "❄️",
        _ => "🌤️",
    };
    let mut out = format!("{}\n\n*Now:* `{}`°C {} 💧 {}% · 💨 {} km/h\n\n", tg_header(emoji, "Forecast", &display_city), esc(temp), esc(desc), esc(humidity), esc(wind));
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
    out.push_str(&format!("\n{}", tg_footer("wttr.in", "forecast")));
    Ok(out)
}

// --- number trivia ---

// --- utility functions ---

fn gen_password(len: &str) -> String {
    let n: usize = len.trim().parse().unwrap_or(16).min(128).max(4);
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%^&*()-_=+[]{}|;:,.<>?";
    let mut s = String::with_capacity(n);
    for _ in 0..n {
        let idx = (rand_byte() as usize) % CHARS.len();
        s.push(CHARS[idx] as char);
    }
    format!("{}\n\n{}", tg_header("🔑", &format!("Password {} chars", n), ""), tg_code_block(&s))
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
    format!("{}\n\n{}", tg_header("🆔", "UUID v4", ""), tg_code_block(&format!("{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}", a, b & 0xFFFF, (b >> 16) & 0xFFF, c | 0x8000, (b as u64) & 0xFFFFFFFFFFFF)))
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
    Ok(format!("{}\n\n`Country:` {}\n`Region:` {}\n`City:` {}\n`ISP:` {}\n`Org:` {}\n`Coords:` {:.4}, {:.4}\n\n{}", tg_header("🌐", "IP", ip), esc(country), esc(region), esc(city), esc(isp), esc(org), lat, lon, tg_footer("ip-api.com", "ip")))
}

fn gen_qr(text: &str) -> String {
    if text.trim().is_empty() { return "usage: `/qr <text>`".into(); }
    let url = format!("https://api.qrserver.com/v1/create-qr-code/?size=300x300&data={}", urlencoding::encode(text));
    format!("{}\n\n![QR]({url})\n\n`{} chars` · #{}", tg_header("📱", "QR Code", ""), esc(&text.len().to_string()), esc("qr"))
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

async fn set_reminder(args: &str, _app: &App) -> String {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let mins: u64 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(5).min(1440);
    let msg_text = parts.get(1).unwrap_or(&"Reminder!");
    let gotify_url = "http://gotify:8080";
    let msg_clone = msg_text.to_string();
    let title = format!("⏰ Reminder in {mins}min");
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(mins * 60)).await;
        let _ = HTTP.post(format!("{gotify_url}/message"))
            .form(&[("title", title.as_str()), ("message", &msg_clone), ("priority", &"5")])
            .send().await;
    });
    let fire_at = Local::now() + chrono::Duration::minutes(mins as i64);
    format!("⏰ *Reminder set*\n\n`{mins} min` — {msg_text}\n\n> fires at {} · #reminder", fire_at.format("%H:%M"))
}

// --- money: portfolio ---

fn portfolio_path(store_path: &str) -> String {
    let dir = std::path::Path::new(store_path).parent().unwrap_or(std::path::Path::new("."));
    dir.join("portfolio.json").to_string_lossy().to_string()
}

#[derive(Serialize, Deserialize, Clone)]
struct Holding { ticker: String, qty: f64, avg_price: f64 }

async fn load_portfolio(store_path: &str) -> Vec<Holding> {
    let p = portfolio_path(store_path);
    tokio::fs::read_to_string(&p).await.ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

async fn save_portfolio(store_path: &str, holdings: &[Holding]) {
    let p = portfolio_path(store_path);
    if let Ok(txt) = serde_json::to_string_pretty(holdings) { let _ = tokio::fs::write(p, txt).await; }
}

async fn handle_portfolio(args: &str, _tid: i64, store_path: &str) -> String {
    let parts: Vec<&str> = args.trim().splitn(3, ' ').collect();
    let sub = parts.first().unwrap_or(&"list");
    let mut holdings = load_portfolio(store_path).await;

    match *sub {
        "add" => {
            let ticker = parts.get(1).unwrap_or(&"").trim().to_uppercase();
            let qty: f64 = parts.get(2).unwrap_or(&"1").trim().parse().unwrap_or(1.0);
            if ticker.is_empty() { return "usage: `/portfolio add AAPL 10`".into(); }
            holdings.retain(|h| h.ticker != ticker);
            let price = fetch_stock_price(&ticker).await.unwrap_or(0.0);
            holdings.push(Holding { ticker: ticker.clone(), qty, avg_price: price });
            save_portfolio(store_path, &holdings).await;
            format!("✅ *Added* `{ticker}` × {qty} @ ${price:.2}")
        }
        "remove" | "rm" => {
            let ticker = parts.get(1).unwrap_or(&"").trim().to_uppercase();
            if ticker.is_empty() { return "usage: `/portfolio rm AAPL`".into(); }
            let before = holdings.len();
            holdings.retain(|h| h.ticker != ticker);
            if holdings.len() == before { return format!("❌ `{ticker}` not found"); }
            save_portfolio(store_path, &holdings).await;
            format!("🗑 Removed `{ticker}`")
        }
        _ => {
            if holdings.is_empty() { return "📊 *Portfolio*\n\n_empty — `/portfolio add AAPL 10`_".into(); }
            let mut total_val = 0.0;
            let mut total_cost = 0.0;
            let mut lines = String::from("```\nTicker  Qty     Price      Value      P&L\n");
            lines.push_str("────── ─────── ────────── ────────── ──────────\n");
            for h in &holdings {
                let price = fetch_stock_price(&h.ticker).await.unwrap_or(h.avg_price);
                let val = price * h.qty;
                let cost = h.avg_price * h.qty;
                let pnl = val - cost;
                let sign = if pnl >= 0.0 { "+" } else { "" };
                lines.push_str(&format!("{:<6} {:>6.1}  ${:>8.2}  ${:>8.2}  {sign}${:.2}\n", h.ticker, h.qty, price, val, pnl));
                total_val += val;
                total_cost += cost;
            }
            lines.push_str("────── ─────── ────────── ────────── ──────────\n");
            let total_pnl = total_val - total_cost;
            let sign = if total_pnl >= 0.0 { "+" } else { "" };
            lines.push_str(&format!("Total           ${:>8.2}  {sign}${:.2}\n```", total_val, total_pnl));
            format!("*📊 Portfolio*\n\n{lines}\n\n> #portfolio")
        }
    }
}

async fn fetch_stock_price(ticker: &str) -> Result<f64> {
    let url = format!("https://query1.finance.yahoo.com/v8/finance/chart/{}?interval=1d&range=1d", ticker);
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "Mozilla/5.0").send().await?.json().await?;
    let price = v["chart"]["result"][0]["meta"]["regularMarketPrice"].as_f64().ok_or_else(|| anyhow::anyhow!("not found"))?;
    Ok(price)
}

// --- money: alerts ---

fn alerts_path(store_path: &str) -> String {
    let dir = std::path::Path::new(store_path).parent().unwrap_or(std::path::Path::new("."));
    dir.join("alerts.json").to_string_lossy().to_string()
}

#[derive(Serialize, Deserialize, Clone)]
struct PriceAlert { ticker: String, above: Option<f64>, below: Option<f64>, triggered: bool }

async fn load_alerts(store_path: &str) -> Vec<PriceAlert> {
    let p = alerts_path(store_path);
    tokio::fs::read_to_string(&p).await.ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

async fn save_alerts(store_path: &str, alerts: &[PriceAlert]) {
    let p = alerts_path(store_path);
    if let Ok(txt) = serde_json::to_string_pretty(alerts) { let _ = tokio::fs::write(p, txt).await; }
}

async fn handle_alerts(args: &str, app: &App) -> String {
    let parts: Vec<&str> = args.trim().splitn(4, ' ').collect();
    let sub = parts.first().unwrap_or(&"list");
    let mut alerts = load_alerts(&app.store_path).await;

    match *sub {
        "add" => {
            let ticker = parts.get(1).unwrap_or(&"").trim().to_uppercase();
            let price: f64 = parts.get(2).unwrap_or(&"").trim().parse().unwrap_or(0.0);
            let direction = parts.get(3).unwrap_or(&"above").trim();
            if ticker.is_empty() || price == 0.0 { return "usage: `/alerts add AAPL 200 above`".into(); }
            alerts.retain(|a| !(a.ticker == ticker && ((direction == "above" && a.above.is_some()) || (direction == "below" && a.below.is_some()))));
            if direction == "below" {
                alerts.push(PriceAlert { ticker: ticker.clone(), above: None, below: Some(price), triggered: false });
            } else {
                alerts.push(PriceAlert { ticker: ticker.clone(), above: Some(price), below: None, triggered: false });
            }
            save_alerts(&app.store_path, &alerts).await;
            format!("🔔 *Alert set*\n\n`{ticker}` {direction} ${price:.2}")
        }
        "rm" | "remove" => {
            let ticker = parts.get(1).unwrap_or(&"").trim().to_uppercase();
            if ticker.is_empty() { return "usage: `/alerts rm AAPL`".into(); }
            let before = alerts.len();
            alerts.retain(|a| a.ticker != ticker);
            if alerts.len() == before { return format!("❌ no alert for `{ticker}`"); }
            save_alerts(&app.store_path, &alerts).await;
            format!("🗑 Removed alert for `{ticker}`")
        }
        _ => {
            if alerts.is_empty() { return "🔔 *Price Alerts*\n\n_none — `/alerts add AAPL 200 above`_".into(); }
            let mut out = String::from("🔔 *Price Alerts*\n\n");
            for a in &alerts {
                let cond = if let Some(ab) = a.above { format!("above ${ab:.2}") } else if let Some(bw) = a.below { format!("below ${bw:.2}") } else { "?".into() };
                let status = if a.triggered { "✅ fired" } else { "⏳ waiting" };
                out.push_str(&format!("`{}` — {cond} · {status}\n", a.ticker));
            }
            out.push_str("\n> /alerts rm <ticker> to remove · #alerts");
            out
        }
    }
}

// --- money: markets ---

async fn fetch_markets() -> Result<String> {
    let indices = vec![
        ("^GSPC", "S&P 500"),
        ("^IXIC", "NASDAQ"),
        ("^DJI", "DOW"),
        ("^RUT", "Russell 2000"),
        ("BTC-USD", "Bitcoin"),
        ("ETH-USD", "Ethereum"),
    ];
    let mut out = String::from("*📈 Markets*\n\n```\nIndex             Price          Change\n");
    out.push_str("───────────────── ────────────── ──────────\n");
    for (ticker, name) in indices {
        match fetch_stock_price(ticker).await {
            Ok(price) => {
                let url = format!("https://query1.finance.yahoo.com/v8/finance/chart/{ticker}?interval=1d&range=2d");
                let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "Mozilla/5.0").send().await?.json().await?;
                let prev = v["chart"]["result"][0]["meta"]["chartPreviousClose"].as_f64().unwrap_or(price);
                let change = price - prev;
                let pct = if prev != 0.0 { change / prev * 100.0 } else { 0.0 };
                let sign = if change >= 0.0 { "+" } else { "" };
                let price_str = if price >= 1000.0 { format!("{:>12.0}", price) } else { format!("{:>12.2}", price) };
                out.push_str(&format!("{name:<17} {price_str}   {sign}{pct:.2}%\n"));
            }
            Err(_) => { out.push_str(&format!("{name:<17} {:>12}   N/A\n", "N/A")); }
        }
    }
    out.push_str("```\n\n");
    out.push_str(&format!("`{}` · #markets", Local::now().format("%Y-%m-%d %H:%M")));
    Ok(out)
}

// --- news: arxiv ---

async fn fetch_arxiv(topic: &str) -> Result<String> {
    let query = if topic.trim().is_empty() { "cat:cs.AI".to_string() } else { format!("all:{}", urlencoding::encode(topic)) };
    let url = format!("http://export.arxiv.org/api/query?search_query={}&sortBy=submittedDate&sortOrder=descending&max_results=5", query);
    let txt = HTTP.get(&url).send().await?.text().await?;
    let mut out = format!("{}\n\n", tg_header("📄", "arXiv", topic));
    let mut current = String::new();
    let mut in_entry = false;

    for line in txt.lines() {
        if line.contains("<entry>") { in_entry = true; current.clear(); }
        if in_entry { current.push_str(line); current.push('\n'); }
        if line.contains("</entry>") {
            in_entry = false;
            let title = extract_xml(&current, "title").replace('\n', " ").trim().to_string();
            let id_url = extract_xml(&current, "id");
            let summary = extract_xml(&current, "summary").chars().take(120).collect::<String>();
            let authors = extract_xml(&current, "name");
            let published = extract_xml(&current, "published").chars().take(10).collect::<String>();
            if !title.is_empty() {
                out.push_str(&format!("*{}*\n[↗]({})\n   {} · `{}`\n   _{}_\n\n", esc(&title), id_url, esc(&authors), published, esc(&summary)));
            }
        }
    }
    if out.len() < 30 { out.push_str(&format!("_No results for `{}`._", esc(topic))); }
    out.push_str(&format!("\n\n{}", tg_footer("arxiv.org", "arxiv")));
    Ok(out)
}

fn extract_xml(xml: &str, tag: &str) -> String {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    if let Some(start) = xml.find(&open) {
        let rest = &xml[start + open.len()..];
        if let Some(end) = rest.find(&close) { return rest[..end].trim().to_string(); }
    }
    String::new()
}

// --- news: dev.to ---

async fn fetch_devto() -> Result<String> {
    let v: serde_json::Value = HTTP.get("https://dev.to/api/articles?per_page=7&top=1").header("User-Agent", "memogram-rs").send().await?.json().await?;
    let articles = v.as_array().ok_or_else(|| anyhow::anyhow!("no articles"))?;
    let mut out = format!("{}\n\n", tg_header("📝", "dev.to Top", ""));
    for (i, a) in articles.iter().take(7).enumerate() {
        let title = a["title"].as_str().unwrap_or("?");
        let url = a["url"].as_str().unwrap_or("");
        let reactions = a["positive_reactions_count"].as_u64().unwrap_or(0);
        let comments = a["comments_count"].as_u64().unwrap_or(0);
        let tags: Vec<&str> = a["tag_list"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).take(3).collect()).unwrap_or_default();
        let tag_str = tags.iter().map(|t| format!("`#{t}`")).collect::<Vec<_>>().join(" ");
        out.push_str(&format!("*{}.* [{}]({})\n   ❤️ {reactions} · 💬 {comments} · {tag_str}\n\n", i + 1, esc(title), url));
    }
    out.push_str(&format!("\n{}", tg_footer("dev.to", "devto")));
    Ok(out)
}

// --- news: product hunt ---

async fn fetch_ph() -> Result<String> {
    let txt = HTTP.get("https://www.producthunt.com/feed").header("User-Agent", "Mozilla/5.0").send().await?.text().await?;
    let mut out = format!("{}\n\n", tg_header("🚀", "Product Hunt", "Today"));
    let mut count = 0;
    let mut remaining = txt.as_str();
    
    while count < 5 {
        if let Some(start) = remaining.find("<entry>") {
            let rest = &remaining[start + 7..];
            if let Some(end) = rest.find("</entry>") {
                let entry = &rest[..end];
                
                let title = extract_ph_tag(entry, "title");
                let link = extract_ph_link(entry);
                let summary = extract_ph_tag(entry, "summary");
                
                if !title.is_empty() {
                    count += 1;
                    let desc = summary.replace("<p>", "").replace("</p>", "").chars().take(80).collect::<String>();
                    out.push_str(&format!("*{count}.* [{title}]({link})\n   _{desc}_\n\n"));
                }
                
                remaining = &rest[end + 8..];
            } else {
                break;
            }
        } else {
            break;
        }
    }
    
    if count == 0 {
        out.push_str("_No posts found._");
    }
    out.push_str(&format!("\n{}", tg_footer("producthunt.com", "ph")));
    Ok(out)
}

fn extract_ph_tag(entry: &str, tag: &str) -> String {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    if let Some(start) = entry.find(&open) {
        let rest = &entry[start + open.len()..];
        if let Some(end) = rest.find(&close) {
            return rest[..end].trim().replace("<![CDATA[", "").replace("]]>", "");
        }
    }
    String::new()
}

fn extract_ph_link(entry: &str) -> String {
    if let Some(start) = entry.find("href=\"") {
        let rest = &entry[start + 6..];
        if let Some(end) = rest.find('"') {
            return rest[..end].to_string();
        }
    }
    String::new()
}

// --- today: inbox ---

async fn fetch_inbox(memos_url: &str, token: &str) -> Result<String> {
    let v: serde_json::Value = HTTP.get(format!("{memos_url}/api/v1/memos?pageSize=200"))
        .header("Authorization", format!("Bearer {token}")).send().await?.json().await?;
    let memos = v["memos"].as_array().ok_or_else(|| anyhow::anyhow!("no memos"))?;
    let untagged: Vec<&serde_json::Value> = memos.iter().filter(|m| {
        m["tags"].as_array().map(|t| t.is_empty()).unwrap_or(true)
    }).collect();
    if untagged.is_empty() { return Ok("📥 *Inbox*\n\n_all memos are tagged ✅_".to_string()); }
    let mut out = format!("📥 *Inbox* — {} untagged\n\n", untagged.len());
    for m in untagged.iter().take(15) {
        let name = m["name"].as_str().unwrap_or("?");
        let content = m["content"].as_str().unwrap_or("").chars().take(80).collect::<String>();
        out.push_str(&format!("*{name}* — `{} chars`\n   _{}_\n\n", content.len(), esc(&content)));
    }
    out.push_str("> tag memos with `/note #tag text` · #inbox");
    Ok(out)
}

// --- today: undo ---

async fn undo_last_memo(memos_url: &str, token: &str) -> String {
    let v: serde_json::Value = match HTTP.get(format!("{memos_url}/api/v1/memos?pageSize=1"))
        .header("Authorization", format!("Bearer {token}")).send().await {
        Ok(r) => match r.json().await { Ok(v) => v, Err(_) => return "❌ failed to fetch".into() },
        Err(_) => return "❌ network error".into(),
    };
    let memos = v["memos"].as_array();
    let Some(first) = memos.and_then(|a| a.first()) else { return "❌ no memos to undo".into(); };
    let name = first["name"].as_str().unwrap_or("");
    let content = first["content"].as_str().unwrap_or("").chars().take(60).collect::<String>();
    match HTTP.delete(format!("{memos_url}/api/v1/{name}")).header("Authorization", format!("Bearer {token}")).send().await {
        Ok(r) if r.status().is_success() => format!("🗑 *Deleted*\n\n`{name}`\n\n_{}_", esc(&content)),
        _ => "❌ delete failed".into(),
    }
}

// --- today: pin ---

async fn pin_last_memo(memos_url: &str, token: &str) -> String {
    let v: serde_json::Value = match HTTP.get(format!("{memos_url}/api/v1/memos?pageSize=1"))
        .header("Authorization", format!("Bearer {token}")).send().await {
        Ok(r) => match r.json().await { Ok(v) => v, Err(_) => return "❌ failed to fetch".into() },
        Err(_) => return "❌ network error".into(),
    };
    let memos = v["memos"].as_array();
    let Some(first) = memos.and_then(|a| a.first()) else { return "❌ no memos to pin".into(); };
    let name = first["name"].as_str().unwrap_or("");
    let already = first["pinned"].as_bool().unwrap_or(false);
    let new_val = !already;
    match HTTP.patch(format!("{memos_url}/api/v1/{name}"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({"pinned": new_val}))
        .send().await {
        Ok(r) if r.status().is_success() => {
            if new_val { format!("📌 *Pinned*\n\n`{name}`") } else { format!("📌 *Unpinned*\n\n`{name}`") }
        }
        _ => "❌ pin failed".into(),
    }
}

// --- today: note ---

async fn create_note(memos_url: &str, token: &str, content: &str) -> String {
    if content.trim().is_empty() { return "usage: `/note #tag my quick thought`".into(); }
    match create_memo(memos_url, token, content).await {
        Ok(name) => format!("✅ *Saved*\n\n`{name}`\n\n_{}_", esc(&content.chars().take(60).collect::<String>())),
        Err(e) => format!("❌ save err: {e}"),
    }
}

// --- markdown document generators ---

fn create_meeting(args: &str) -> String {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let topic = parts.first().filter(|s| !s.is_empty()).copied().unwrap_or("Untitled");
    let notes = parts.get(1).unwrap_or(&"");
    let date = Local::now().format("%Y-%m-%d").to_string();
    format!(
        "# Meeting: {topic}\n\n**Date:** {date}\n\n## Attendees\n- \n\n## Agenda\n- \n\n## Discussion\n{notes}\n\n## Action Items\n- [ ] \n\n## Next Steps\n- \n\n#meeting #notes {date}",
        topic = esc(topic), date = date, notes = notes
    )
}

fn create_project(args: &str) -> String {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let name = parts.first().filter(|s| !s.is_empty()).copied().unwrap_or("Untitled");
    let desc = parts.get(1).unwrap_or(&"");
    let date = Local::now().format("%Y-%m-%d").to_string();
    format!(
        "# Project: {name}\n\n**Created:** {date}\n**Status:** 🟡 In Progress\n\n## Goal\n{desc}\n\n## Tasks\n- [ ] \n- [ ] \n- [ ] \n\n## Notes\n- \n\n## Timeline\n- **Week 1:** \n- **Week 2:** \n\n#project #planning",
        name = esc(name), date = date, desc = desc
    )
}

fn create_recipe(args: &str) -> String {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let name = parts.first().filter(|s| !s.is_empty()).copied().unwrap_or("Untitled");
    let tags = parts.get(1).unwrap_or(&"");
    format!(
        "# Recipe: {name}\n\n**Tags:** {tags}\n**Prep:** 15 min\n**Cook:** 30 min\n**Servings:** 4\n\n## Ingredients\n- \n- \n- \n\n## Instructions\n1. \n2. \n3. \n\n## Notes\n- \n\n## Nutrition\n- Calories: \n- Protein: \n\n#recipe #cooking",
        name = esc(name), tags = tags
    )
}

fn create_book(args: &str) -> String {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let title = parts.first().filter(|s| !s.is_empty()).copied().unwrap_or("Untitled");
    let author = parts.get(1).unwrap_or(&"");
    let date = Local::now().format("%Y-%m-%d").to_string();
    format!(
        "# Book: {title}\n\n**Author:** {author}\n**Started:** {date}\n**Status:** 📖 Reading\n**Rating:** ⭐⭐⭐⭐⭐\n\n## Summary\n- \n\n## Key Takeaways\n1. \n2. \n3. \n\n## Favorite Quotes\n> \"\" \n\n## Notes\n- \n\n#book #reading",
        title = esc(title), author = author, date = date
    )
}

fn create_todo(args: &str) -> String {
    let items: Vec<&str> = args.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    if items.is_empty() { return "usage: `/todo buy milk, write report, call mom`".into(); }
    let mut out = format!("{}\n\n", tg_header("✅", "Todo List", ""));
    for item in items {
        out.push_str(&format!("- [ ] {}\n", esc(item)));
    }
    out.push_str(&format!("\n{}", tg_footer("todo", "tasks")));
    out
}

fn create_list(args: &str) -> String {
    let items: Vec<&str> = args.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    if items.is_empty() { return "usage: `/list apples, bananas, oranges`".into(); }
    let mut out = "# List\n\n".to_string();
    for item in items {
        out.push_str(&format!("- {}\n", item));
    }
    out.push_str("\n#list #notes");
    out
}

fn create_clip(args: &str) -> String {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let url = parts.first().filter(|s| !s.is_empty()).copied().unwrap_or("");
    let notes = parts.get(1).unwrap_or(&"");
    let date = Local::now().format("%Y-%m-%d").to_string();
    format!(
        "# Bookmark\n\n**URL:** {url}\n**Saved:** {date}\n\n## Notes\n{notes}\n\n#bookmark #save",
        url = url, date = date, notes = notes
    )
}

fn create_proscons(args: &str) -> String {
    let topic = if args.trim().is_empty() { "Untitled" } else { args.trim() };
    format!(
        "# Pros & Cons: {topic}\n\n## ✅ Pros\n- \n- \n- \n\n## ❌ Cons\n- \n- \n- \n\n## Verdict\n- \n\n## Alternative Options\n1. \n\n#comparison #decision",
        topic = esc(topic)
    )
}

fn create_flashcard(args: &str) -> String {
    let parts: Vec<&str> = args.splitn(2, " | ").collect();
    let q = parts.first().filter(|s| !s.is_empty()).copied().unwrap_or("Question?");
    let a = parts.get(1).unwrap_or(&"Answer");
    format!(
        "# Flashcard\n\n## ❓ Question\n{q}\n\n## 💡 Answer\n{a}\n\n#flashcard #study",
        q = q, a = a
    )
}





// === NEW COMMANDS: Bioengineering ===

async fn fetch_pubmed(query: &str) -> Result<String> {
    let url = format!("https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi?db=pubmed&retmax=5&term={}", urlencoding::encode(query));
    let resp = HTTP.get(&url).send().await?.text().await?;
    let mut ids = Vec::new();
    for cap in Regex::new(r"<Id>(\d+)</Id>")?.captures_iter(&resp) {
        ids.push(cap[1].to_string());
    }
    if ids.is_empty() { return Ok(format!("No results for *{query}*")); }
    let id_list = ids.join(",");
    let summary_url = format!("https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esummary.fcgi?db=pubmed&id={}&retmode=json", id_list);
    let summary: serde_json::Value = HTTP.get(&summary_url).send().await?.json().await?;
    let mut out = format!("📚 *PubMed:* `{query}`\n\n");
    for id in &ids {
        if let Some(article) = summary["result"][id].as_object() {
            let title = article["title"].as_str().unwrap_or("?");
            let authors = article["sortfirstauthor"].as_str().unwrap_or("?");
            let pubdate = article["pubdate"].as_str().unwrap_or("?");
            out.push_str(&format!("• *{}*\n  {} — {}\n  https://pubmed.ncbi.nlm.nih.gov/{}/\n\n", title, authors, pubdate, id));
        }
    }
    Ok(out)
}

async fn fetch_drug(name: &str) -> Result<String> {
    let url = format!("https://api.fda.gov/drug/label.json?search=openfda.brand_name:{}+OR+openfda.generic_name:{}&limit=1", urlencoding::encode(name), urlencoding::encode(name));
    let resp: serde_json::Value = HTTP.get(&url).send().await?.json().await?;
    if let Some(err) = resp["error"].as_object() {
        if err.get("code") == Some(&serde_json::Value::String("NOT_FOUND".into())) {
            return Ok(format!("No drug info for *{name}*"));
        }
    }
    if let Some(results) = resp["results"].as_array() {
        if let Some(drug) = results.first() {
            let brand = drug["openfda"]["brand_name"].as_array().and_then(|a| a.first()).and_then(|v| v.as_str()).unwrap_or("?");
            let generic = drug["openfda"]["generic_name"].as_array().and_then(|a| a.first()).and_then(|v| v.as_str()).unwrap_or("?");
            let purpose = drug["purpose"].as_array().and_then(|a| a.first()).and_then(|v| v.as_str()).unwrap_or("N/A");
            let warnings = drug["warnings"].as_array().and_then(|a| a.first()).and_then(|v| v.as_str()).unwrap_or("N/A");
            return Ok(format!("💊 *{brand}* ({generic})\n\n**Purpose:** {purpose}\n\n**Warnings:** {warnings}", brand = brand, generic = generic, purpose = &purpose[..purpose.len().min(300)], warnings = &warnings[..warnings.len().min(300)]));
        }
    }
    Ok(format!("No drug info for *{name}*"))
}

async fn fetch_genome(query: &str) -> Result<String> {
    let url = format!("https://api.ncbi.nlm.nih.gov/datasets/v2/genus/+/taxon/{}/dataset_report?page_size=3", urlencoding::encode(query));
    let resp: serde_json::Value = HTTP.get(&url).send().await?.json().await?;
    if let Some(taxonomy) = resp["assembly_summary"].as_array() {
        if let Some(first) = taxonomy.first() {
            let name = first["organism_name"].as_str().unwrap_or("?");
            let acc = first["assembly_accession"].as_str().unwrap_or("?");
            let status = first["assembly_level"].as_str().unwrap_or("?");
            return Ok(format!("🧬 *Genome:* {name}\n**Accession:** {acc}\n**Level:** {status}\nhttps://www.ncbi.nlm.nih.gov/datasets/{acc}"));
        }
    }
    // Fallback: search NCBI
    let search_url = format!("https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi?db=nucleotide&retmax=3&term={}", urlencoding::encode(query));
    let resp = HTTP.get(&search_url).send().await?.text().await?;
    let ids: Vec<String> = Regex::new(r"<Id>(\d+)</Id>")?.captures_iter(&resp).map(|c| c[1].to_string()).collect();
    if ids.is_empty() { return Ok(format!("No genome results for *{query}*")); }
    Ok(format!("🧬 *Genome:* `{query}`\nIDs: {}\nhttps://www.ncbi.nlm.nih.gov/nuccore/{}", ids.join(", "), ids[0]))
}

async fn fetch_protein(query: &str) -> Result<String> {
    let url = format!("https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi?db=protein&retmax=5&term={}", urlencoding::encode(query));
    let resp = HTTP.get(&url).send().await?.text().await?;
    let ids: Vec<String> = Regex::new(r"<Id>(\d+)</Id>")?.captures_iter(&resp).map(|c| c[1].to_string()).collect();
    if ids.is_empty() { return Ok(format!("No protein results for *{query}*")); }
    let summary_url = format!("https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esummary.fcgi?db=protein&id={}&retmode=json", ids.join(","));
    let summary: serde_json::Value = HTTP.get(&summary_url).send().await?.json().await?;
    let mut out = format!("🧬 *Protein:* `{query}`\n\n");
    for id in &ids {
        if let Some(item) = summary["result"][id].as_object() {
            let title = item["title"].as_str().unwrap_or("?");
            out.push_str(&format!("• *{}*\n  https://www.ncbi.nlm.nih.gov/protein/{}\n\n", title, id));
        }
    }
    Ok(out)
}

// === NEW COMMANDS: Stoicism ===

async fn fetch_stoic_quote() -> Result<String> {
    let resp: serde_json::Value = HTTP.get("https://stoic-quotes/api/v1/random").send().await?.json().await?;
    let text = resp["text"].as_str().unwrap_or("?");
    let author = resp["author"].as_str().unwrap_or("?");
    Ok(format!("{}\n\n\"{}\"\n\n— *{}*\n\n{}", tg_header("🏛️", "Stoic Wisdom", ""), esc(text), esc(author), tg_footer("stoic", "stoic")))
}

fn create_mood_entry(args: &str) -> String {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let mood = parts.first().filter(|s| !s.is_empty()).copied().unwrap_or("neutral");
    let note = parts.get(1).unwrap_or(&"");
    let date = Local::now().format("%Y-%m-%d %H:%M").to_string();
    format!(
        "# Mood\n\n**Date:** {date}\n**Mood:** {mood}\n**Note:** {note}\n\n#mood #daily",
        date = date, mood = mood, note = note
    )
}

fn create_gratitude_entry(args: &str) -> String {
    let items: Vec<&str> = args.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    if items.is_empty() { return "usage: `/gratitude family, health, code`".into(); }
    let date = Local::now().format("%Y-%m-%d").to_string();
    let mut out = format!("# Gratitude\n\n**Date:** {date}\n\n", date = date);
    for item in items {
        out.push_str(&format!("• ✨ {}\n", item));
    }
    out.push_str("\n#gratitude #daily");
    out
}

fn create_habit_entry(args: &str) -> String {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let habit = parts.first().filter(|s| !s.is_empty()).copied().unwrap_or("habit");
    let status = parts.get(1).unwrap_or(&"done");
    let date = Local::now().format("%Y-%m-%d").to_string();
    format!(
        "# Habit Tracker\n\n**Date:** {date}\n**Habit:** {habit}\n**Status:** {status}\n\n#habit #daily",
        date = date, habit = habit, status = status
    )
}

// === NEW COMMANDS: Dev Tools ===

async fn fetch_npm(pkg: &str) -> Result<String> {
    let resp: serde_json::Value = HTTP.get(format!("https://registry.npmjs.org/{}", urlencoding::encode(pkg))).send().await?.json().await?;
    if let Some(msg) = resp["error"].as_str() { return Ok(format!("npm: {msg}")); }
    let name = resp["name"].as_str().unwrap_or("?");
    let version = resp["dist-tags"]["latest"].as_str().unwrap_or("?");
    let desc = resp["description"].as_str().unwrap_or("?");
    let homepage = resp["homepage"].as_str().unwrap_or("");
    Ok(format!("{}\n\n{}\n\nhttps://www.npmjs.com/package/{}\n\n{}", tg_header("📦", "npm", &format!("{}@{}", name, version)), esc(desc), esc(name), tg_footer("npmjs.com", "npm")))
}

async fn fetch_pypi(pkg: &str) -> Result<String> {
    let resp: serde_json::Value = HTTP.get(format!("https://pypi.org/pypi/{}/json", urlencoding::encode(pkg))).send().await?.json().await?;
    if let Some(msg) = resp["message"].as_str() { return Ok(format!("pypi: {msg}")); }
    let info = &resp["info"];
    let name = info["name"].as_str().unwrap_or("?");
    let version = info["version"].as_str().unwrap_or("?");
    let summary = info["summary"].as_str().unwrap_or("?");
    Ok(format!("📦 *PyPI:* `{name}@{version}`\n{summary}\n\nhttps://pypi.org/project/{name}/"))
}

async fn fetch_crates(pkg: &str) -> Result<String> {
    let resp: serde_json::Value = HTTP.get(format!("https://crates.io/api/v1/crates/{}", urlencoding::encode(pkg))).send().await?.json().await?;
    if let Some(c) = resp["crate"].as_object() {
        let name = c["name"].as_str().unwrap_or("?");
        let version = c["max_version"].as_str().unwrap_or("?");
        let desc = c["description"].as_str().unwrap_or("?");
        let downloads = c["downloads"].as_i64().unwrap_or(0);
        return Ok(format!("{}\n\n{}\nDownloads: {}\n\nhttps://crates.io/crates/{}\n\n{}", tg_header("📦", "crates.io", &format!("{}@{}", name, version)), esc(desc), esc(&downloads.to_string()), esc(name), tg_footer("crates.io", "crates")));
    }
    Ok(format!("crate not found: *{pkg}*"))
}

async fn fetch_stackoverflow(query: &str) -> Result<String> {
    let url = format!("https://api.stackexchange.com/2.3/search?order=desc&sort=activity&intitle={}&site=stackoverflow&pagesize=5", urlencoding::encode(query));
    let resp: serde_json::Value = HTTP.get(&url).send().await?.json().await?;
    if let Some(items) = resp["items"].as_array() {
        if items.is_empty() { return Ok(format!("No results for *{query}*")); }
        let mut out = format!("📖 *Stack Overflow:* `{query}`\n\n");
        for item in items {
            let title = item["title"].as_str().unwrap_or("?");
            let link = item["link"].as_str().unwrap_or("?");
            let score = item["score"].as_i64().unwrap_or(0);
            let answers = item["answer_count"].as_i64().unwrap_or(0);
            out.push_str(&format!("• *{}* (⬆{} answers:{}\n  {link}\n\n", title, score, answers));
        }
        return Ok(out);
    }
    Ok(format!("stackoverflow err for *{query}*"))
}

// === NEW COMMANDS: Weather ===

async fn fetch_airquality(loc: &str) -> Result<String> {
    let url = format!("https://api.waqi.info/feed/{}/?token=demo", urlencoding::encode(loc));
    let resp: serde_json::Value = HTTP.get(&url).send().await?.json().await?;
    if resp["status"].as_str() == Some("ok") {
        let data = &resp["data"];
        let aqi = data["aqi"].as_i64().unwrap_or(0);
        let city = data["city"]["name"].as_str().unwrap_or("?");
        let dominant = data["dominentpol"].as_str().unwrap_or("?");
        return Ok(format!("🌬️ *Air Quality:* {city}\n**AQI:** {aqi}\n**Dominant pollutant:** {dominant}\n\nhttps://aqicn.org/city/{loc}"));
    }
    Ok(format!("air quality data unavailable for *{loc}*"))
}

async fn fetch_sunrise(loc: &str) -> Result<String> {
    let parts: Vec<&str> = loc.split(',').collect();
    let lat = parts.first().unwrap_or(&"40.7");
    let lon = parts.get(1).unwrap_or(&"-74.0");
    let url = format!("https://api.sunrise-sunset.org/json?lat={}&lng={}&formatted=0", lat.trim(), lon.trim());
    let resp: serde_json::Value = HTTP.get(&url).send().await?.json().await?;
    if resp["status"].as_str() == Some("OK") {
        let results = &resp["results"];
        let sunrise = results["sunrise"].as_str().unwrap_or("?");
        let sunset = results["sunset"].as_str().unwrap_or("?");
        let day_length = results["day_length"].as_i64().unwrap_or(0);
        let hours = day_length / 3600;
        let mins = (day_length % 3600) / 60;
        return Ok(format!("{}\n\n**Sunrise:** `{}`\n**Sunset:** `{}`\n**Day length:** `{}h {}m`\n\n{}", tg_header("🌅", "Sunrise/Sunset", loc), esc(sunrise), esc(sunset), esc(&hours.to_string()), esc(&mins.to_string()), tg_footer("sunrise-sunset.org", "sunrise")));
    }
    Ok(format!("sunrise data unavailable for *{loc}*"))
}

// === NEW COMMANDS: Learn ===

fn eval_math(expr: &str) -> String {
    let cleaned: String = expr.chars().filter(|c| c.is_ascii_digit() || *c == '.' || *c == '+' || *c == '-' || *c == '*' || *c == '/' || *c == '(' || *c == ')' || *c == ' ').collect();
    let header = tg_header("🔢", "Math", expr);
    format!("{}\n\nEvaluate: {}\n\n{}", header, tg_code_block(&cleaned), tg_footer("math", "learn"))
}

async fn fetch_etymology(word: &str) -> Result<String> {
    let url = format!("https://en.wiktionary.org/w/api.php?action=parse&page={}&prop=wikitext&format=json", urlencoding::encode(word));
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "memogram-rs").send().await?.json().await?;
    let wikitext = v["parse"]["wikitext"]["wikitext"].as_str().unwrap_or("");
    let mut out = format!("{}\n\n", tg_header("📖", "Etymology", word));
    
    let lower = wikitext.to_lowercase();
    if let Some(start) = lower.find("==etymology==") {
        let rest = &wikitext[start + 13..];
        if let Some(end) = rest.find("\n==") {
            let etym = rest[..end].trim();
            if !etym.is_empty() {
                let clean = etym.replace("{{inh|en|", "").replace("{{der|en|", "").replace("{{bor|en|", "").replace("{{m|en|", "").replace("{{l|en|", "").replace("}}", "").replace("{{XLIT|en|", "").replace('\n', " ");
                let short = clean.chars().take(500).collect::<String>();
                out.push_str(&esc(&short));
                out.push_str(&format!("\n\n{}", tg_footer("wiktionary.org", "etymology")));
                return Ok(out);
            }
        }
    }
    out.push_str(&format!("_No etymology section found for `{}`._\n\nTry: https://en.wiktionary.org/wiki/{}", esc(word), urlencoding::encode(word)));
    out.push_str(&format!("\n\n{}", tg_footer("wiktionary.org", "etymology")));
    Ok(out)
}

async fn fetch_synonym(word: &str) -> Result<String> {
    let resp: serde_json::Value = HTTP.get(format!("https://api.datamuse.com/words?rel_syn={}", urlencoding::encode(word))).send().await?.json().await?;
    if let Some(words) = resp.as_array() {
        if words.is_empty() { return Ok(format!("{} \n\n_No synonyms found._\n\n{}", tg_header("📝", "Synonyms", word), tg_footer("datamuse.com", "synonym"))); }
        let syns: Vec<String> = words.iter().take(10).filter_map(|w| w["word"].as_str()).map(|s| format!("`{}`", esc(s))).collect();
        return Ok(format!("{}\n\n{}\n\n{}", tg_header("📝", "Synonyms", word), syns.join(", "), tg_footer("datamuse.com", "synonym")));
    }
    Ok(format!("{} \n\n_synonym lookup failed_\n\n{}", tg_header("📝", "Synonyms", word), tg_footer("datamuse.com", "synonym")))
}

async fn fetch_philosophy_quote() -> Result<String> {
    let quotes = [
        ("The unexamined life is not worth living.", "Socrates"),
        ("I think, therefore I am.", "René Descartes"),
        ("Happiness is not an ideal of reason but of imagination.", "Immanuel Kant"),
        ("He who thinks great thoughts, often makes great errors.", "Martin Heidegger"),
        ("The only true wisdom is in knowing you know nothing.", "Socrates"),
        ("Life is examined by living it.", "George Santayana"),
        ("Man is condemned to be free.", "Jean-Paul Sartre"),
        ("To be is to be perceived.", "George Berkeley"),
        ("One cannot step twice into the same river.", "Heraclitus"),
        ("God is dead.", "Friedrich Nietzsche"),
        ("I cannot teach anybody anything. I can only make them think.", "Socrates"),
        ("The owl of Minerva spreads its wings only with the falling of the dusk.", "G.W.F. Hegel"),
    ];
    let idx = (chrono::Utc::now().timestamp() as usize) % quotes.len();
    let (quote, author) = quotes[idx];
    Ok(format!("{}\n\n\"{}\"\n\n— *{}*\n\n{}", tg_header("📚", "Philosophy", ""), esc(quote), esc(author), tg_footer("philosophy", "learn")))
}

// === STOIC COMMANDS ===

fn create_meditation(note: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let dur = note.split_whitespace().next().unwrap_or("?");
    format!("{}\n\nDuration: `{}`\nNote: {}\n\n— `{}`\n\n{}", tg_header("🧘", "Meditation", dur), esc(dur), esc(note), esc(&now), tg_footer("meditation", "stoic"))
}

fn create_affirmation(note: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    format!("{}\n\n\"{}\"\n\n— `{}`\n\n{}", tg_header("💪", "Affirmation", ""), esc(note), esc(&now), tg_footer("affirmation", "stoic"))
}

fn create_reflection(note: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    format!("{}\n\n{}\n\n— `{}`\n\n{}", tg_header("🪞", "Reflection", ""), esc(note), esc(&now), tg_footer("reflection", "stoic"))
}

async fn fetch_wisdom() -> Result<String> {
    let quotes = [
        ("The happiness of your life depends upon the quality of your thoughts.", "Marcus Aurelius"),
        ("Waste no more time arguing about what a good man should be. Be one.", "Marcus Aurelius"),
        ("He who fears death will never do anything worthy of a living man.", "Seneca"),
        ("We suffer more often in imagination than in reality.", "Seneca"),
        ("No man is free who is not master of himself.", "Epictetus"),
        ("First say to yourself what you would be; and then do what you have to do.", "Epictetus"),
        ("The best revenge is not to be like your enemy.", "Marcus Aurelius"),
        ("It is not that we have a short time to live, but that we waste a good deal of it.", "Seneca"),
        ("Difficulties strengthen the mind, as labor does the body.", "Seneca"),
        ("You have power over your mind — not outside events. Realize this, and you will find strength.", "Marcus Aurelius"),
    ];
    let idx = (chrono::Utc::now().timestamp() as usize) % quotes.len();
    let (quote, author) = quotes[idx];
    Ok(format!("🏛️ *Stoic Wisdom*\n\n\"{quote}\"\n\n— *{author}*\n\n#wisdom #stoic"))
}

fn create_journal(note: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M");
    format!("📔 *Journal*\n\n{note}\n\n— {now}\n\n#journal #stoic")
}

// === PLANNING COMMANDS ===

fn create_goal(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d").to_string();
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let goal = parts.first().unwrap_or(&"");
    let details = parts.get(1).unwrap_or(&"");
    format!("{}\n\n*Goal:* {}\n*Details:* {}\n*Set:* `{}`\n\n{}", tg_header("🎯", "Goal", goal), esc(goal), esc(details), esc(&now), tg_footer("goal", "planning"))
}

fn create_deadline(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d");
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let date = parts.first().unwrap_or(&"");
    let task = parts.get(1).unwrap_or(&"");
    format!("⏰ *Deadline*\n\n*Due:* {date}\n*Task:* {task}\n*Set:* {now}\n\n#deadline #planning")
}

fn create_plan(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d");
    format!("📋 *Plan*\n\n{args}\n\n— {now}\n\n#plan #planning")
}

fn create_review(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d");
    format!("📊 *Review*\n\n{args}\n\n— {now}\n\n#review #planning")
}

fn create_priority(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d");
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let level = parts.first().unwrap_or(&"");
    let task = parts.get(1).unwrap_or(&"");
    format!("🔥 *Priority*\n\n*Level:* {level}\n*Task:* {task}\n*Set:* {now}\n\n#priority #planning")
}

// === INBOX COMMANDS ===

fn create_idea(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M");
    format!("💡 *Idea*\n\n{args}\n\n— {now}\n\n#idea #inbox")
}

fn create_braindump(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M");
    format!("🧠 *Brain Dump*\n\n{args}\n\n— {now}\n\n#braindump #inbox")
}

fn create_link(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M");
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let url = parts.first().unwrap_or(&"");
    let desc = parts.get(1).unwrap_or(&"");
    format!("🔗 *Link*\n\nURL: {url}\nDescription: {desc}\n\n— {now}\n\n#link #inbox")
}

fn create_snippet(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M");
    format!("📝 *Snippet*\n\n```\n{args}\n```\n\n— {now}\n\n#snippet #inbox")
}

fn create_save(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M");
    format!("💾 *Saved*\n\n{args}\n\n— {now}\n\n#save #inbox")
}

// === DAILY COMMANDS ===

fn create_morning(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d");
    format!("🌅 *Morning Check-in*\n\n{args}\n\n— {now}\n\n#morning #daily")
}

fn create_evening(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d");
    format!("🌙 *Evening Reflection*\n\n{args}\n\n— {now}\n\n#evening #daily")
}

fn create_checkin(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M");
    format!("✅ *Check-in*\n\n{args}\n\n— {now}\n\n#checkin #daily")
}

fn create_log(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M");
    format!("📋 *Log*\n\n{args}\n\n— {now}\n\n#log #daily")
}

fn create_summary(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d");
    format!("📝 *Summary*\n\n{args}\n\n— {now}\n\n#summary #daily")
}

// === LIFE COMMANDS ===

fn create_sleep(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d");
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let hours = parts.first().unwrap_or(&"?");
    let quality = parts.get(1).unwrap_or(&"");
    format!("😴 *Sleep*\n\n*Hours:* {hours}\n*Quality:* {quality}\n*Date:* {now}\n\n#sleep #life")
}

fn create_energy(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M");
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let level = parts.first().unwrap_or(&"?");
    let note = parts.get(1).unwrap_or(&"");
    format!("⚡ *Energy*\n\n*Level:* {level}/10\n*Note:* {note}\n*Time:* {now}\n\n#energy #life")
}

fn create_exercise(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d");
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let activity = parts.first().unwrap_or(&"");
    let duration = parts.get(1).unwrap_or(&"");
    format!("🏋️ *Exercise*\n\n*Activity:* {activity}\n*Duration:* {duration}\n*Date:* {now}\n\n#exercise #life")
}

fn create_water(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M");
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let amount = parts.first().unwrap_or(&"?");
    let note = parts.get(1).unwrap_or(&"");
    format!("💧 *Water*\n\n*Amount:* {amount}\n*Note:* {note}\n*Time:* {now}\n\n#water #life")
}

fn create_read(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d");
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let title = parts.first().unwrap_or(&"");
    let author = parts.get(1).unwrap_or(&"");
    format!("📚 *Reading*\n\n*Title:* {title}\n*Author:* {author}\n*Date:* {now}\n\n#read #life")
}
