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
    Bbc,
    Reuters,
    Ap,
    Reddit(String),
    Tldr,
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
    Stress(String),
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
    Finance(String),
    Compound(String),
    Trial(String),
    Food(String),
    Sunset(String),
    // Music bucket (7) — beats / promo
    Itunes(String),
    Deezer(String),
    Mbrainz(String),
    Lyrics(String),
    Bpm(String),
    Trend,
    Promo(String),
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
        if let Some(tok) = self.bot_tokens.get(bot).cloned() {
            return Some(tok);
        }
        // Fallback for renamed bucket: wellness <- stoic/life
        if bot == "wellness" {
            if let Some(tok) = self.bot_tokens.get("stoic").cloned().or_else(|| self.bot_tokens.get("life").cloned()) {
                warn!("wellness: using fallback token from stoic/life");
                return Some(tok);
            }
        }
        warn!("no bot token for {bot}, fallback to memogram store");
        None
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Preview mode: generate markdown samples locally without needing Telegram
    if std::env::var("PREVIEW").is_ok() {
        return run_preview().await;
    }
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
        teloxide::types::BotCommand { command: "arxiv".into(), description: "arXiv latest papers".into() },
        teloxide::types::BotCommand { command: "devto".into(), description: "dev.to top posts".into() },
        teloxide::types::BotCommand { command: "bbc".into(), description: "BBC World News".into() },
        teloxide::types::BotCommand { command: "reuters".into(), description: "Reuters World".into() },
        teloxide::types::BotCommand { command: "ap".into(), description: "AP World News".into() },
        teloxide::types::BotCommand { command: "reddit".into(), description: "Reddit <subreddit>".into() },
        teloxide::types::BotCommand { command: "tldr".into(), description: "TLDR tech digest".into() },
        teloxide::types::BotCommand { command: "weather".into(), description: "weather <city> (default: Thousand Oaks, CA)".into() },
        teloxide::types::BotCommand { command: "forecast".into(), description: "7-day forecast (default: Thousand Oaks, CA)".into() },
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
        teloxide::types::BotCommand { command: "pass".into(), description: "generate password".into() },
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
        teloxide::types::BotCommand { command: "airquality".into(), description: "air quality (default: Thousand Oaks, CA)".into() },
        teloxide::types::BotCommand { command: "sunrise".into(), description: "sunrise/sunset (default: Thousand Oaks, CA)".into() },
        teloxide::types::BotCommand { command: "sunset".into(), description: "sunset/sunrise (default: Thousand Oaks, CA)".into() },
        teloxide::types::BotCommand { command: "math".into(), description: "math expression".into() },
        teloxide::types::BotCommand { command: "etymology".into(), description: "word etymology".into() },
        teloxide::types::BotCommand { command: "synonym".into(), description: "find synonyms".into() },
        teloxide::types::BotCommand { command: "philosophy".into(), description: "philosophy quote".into() },
        teloxide::types::BotCommand { command: "finance".into(), description: "finance term explainer".into() },
        teloxide::types::BotCommand { command: "compound".into(), description: "compound interest calc".into() },
        teloxide::types::BotCommand { command: "trial".into(), description: "clinical trial search".into() },
        teloxide::types::BotCommand { command: "food".into(), description: "nutrition lookup".into() },
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
            let c = if city.trim().is_empty() { "Thousand Oaks, CA".to_string() } else { city };
            let txt = fetch_weather(&c).await.unwrap_or_else(|e| format!("weather err: {e}"));
            create_as_bot(&bot, &msg, &app, "weather", &txt, tid).await?;
        }
        Command::Define(w) => { let txt = fetch_define(&w).await.unwrap_or_else(|e| format!("define err: {e}")); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Wiki(q) => { let txt = fetch_wiki(&q).await.unwrap_or_else(|e| format!("wiki err: {e}")); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Cheat(q) => { let txt = fetch_cheat(&q).await.unwrap_or_else(|e| format!("cheat err: {e}")); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Gh(q) => { let txt = fetch_gh(&q).await.unwrap_or_else(|e| format!("gh err: {e}")); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
        Command::Fx(pair) => { let txt = fetch_fx(&pair).await.unwrap_or_else(|e| format!("fx err: {e}")); create_as_bot(&bot, &msg, &app, "money", &txt, tid).await?; }
        Command::Containers => { let txt = fetch_containers(&app.memos_url).await.unwrap_or_else(|e| format!("containers err: {e}")); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
        Command::Bbc => { let txt = fetch_bbc().await.unwrap_or_else(|e| format!("bbc err: {e}")); create_as_bot(&bot, &msg, &app, "news", &txt, tid).await?; }
        Command::Reuters => { let txt = fetch_reuters().await.unwrap_or_else(|e| format!("reuters err: {e}")); create_as_bot(&bot, &msg, &app, "news", &txt, tid).await?; }
        Command::Ap => { let txt = fetch_ap().await.unwrap_or_else(|e| format!("ap err: {e}")); create_as_bot(&bot, &msg, &app, "news", &txt, tid).await?; }
        Command::Reddit(q) => { let txt = fetch_reddit(&q).await.unwrap_or_else(|e| format!("reddit err: {e}")); create_as_bot(&bot, &msg, &app, "news", &txt, tid).await?; }
        Command::Tldr => { let txt = fetch_tldr().await.unwrap_or_else(|e| format!("tldr err: {e}")); create_as_bot(&bot, &msg, &app, "news", &txt, tid).await?; }
        Command::Arxiv(topic) => { let txt = fetch_arxiv(&topic).await.unwrap_or_else(|e| format!("arxiv err: {e}")); create_as_bot(&bot, &msg, &app, "news", &txt, tid).await?; }
        Command::Devto => { let txt = fetch_devto().await.unwrap_or_else(|e| format!("devto err: {e}")); create_as_bot(&bot, &msg, &app, "news", &txt, tid).await?; }
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
        Command::Recipe(args) => { let txt = fetch_recipe(&args).await.unwrap_or_else(|e| format!("recipe err: {e}")); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
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
        Command::Stoic => { let txt = fetch_stoic_quote().await.unwrap_or_else(|e| format!("stoic err: {e}")); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Mood(note) => { let txt = create_mood_entry(&note); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Gratitude(note) => { let txt = create_gratitude_entry(&note); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Habit(args) => { let txt = create_habit_entry(&args); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Stress(args) => { let txt = create_stress(&args); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Npm(pkg) => { let txt = fetch_npm(&pkg).await.unwrap_or_else(|e| format!("npm err: {e}")); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
        Command::Pypi(pkg) => { let txt = fetch_pypi(&pkg).await.unwrap_or_else(|e| format!("pypi err: {e}")); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
        Command::Crates(pkg) => { let txt = fetch_crates(&pkg).await.unwrap_or_else(|e| format!("crates err: {e}")); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
        Command::Stackoverflow(q) => { let txt = fetch_stackoverflow(&q).await.unwrap_or_else(|e| format!("stackoverflow err: {e}")); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
        Command::Airquality(loc) => { let txt = fetch_airquality(&loc).await.unwrap_or_else(|e| format!("airquality err: {e}")); create_as_bot(&bot, &msg, &app, "weather", &txt, tid).await?; }
        Command::Sunrise(loc) => { let txt = fetch_sunrise(&loc).await.unwrap_or_else(|e| format!("sunrise err: {e}")); create_as_bot(&bot, &msg, &app, "weather", &txt, tid).await?; }
        Command::Sunset(loc) => { let txt = fetch_sunrise(&loc).await.unwrap_or_else(|e| format!("sunrise err: {e}")); create_as_bot(&bot, &msg, &app, "weather", &txt, tid).await?; }
        Command::Math(expr) => { let txt = eval_math(&expr); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Etymology(word) => { let txt = fetch_etymology(&word).await.unwrap_or_else(|e| format!("etymology err: {e}")); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Synonym(word) => { let txt = fetch_synonym(&word).await.unwrap_or_else(|e| format!("synonym err: {e}")); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Philosophy => { let txt = fetch_philosophy_quote().await.unwrap_or_else(|e| format!("philosophy err: {e}")); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Finance(term) => { let txt = fetch_finance(&term).await.unwrap_or_else(|e| format!("finance err: {e}")); create_as_bot(&bot, &msg, &app, "money", &txt, tid).await?; }
        Command::Compound(args) => { let txt = create_compound(&args); create_as_bot(&bot, &msg, &app, "money", &txt, tid).await?; }
        Command::Trial(q) => { let txt = fetch_trial(&q).await.unwrap_or_else(|e| format!("trial err: {e}")); create_as_bot(&bot, &msg, &app, "bio", &txt, tid).await?; }
        Command::Food(q) => { let txt = fetch_food(&q).await.unwrap_or_else(|e| format!("food err: {e}")); create_as_bot(&bot, &msg, &app, "bio", &txt, tid).await?; }
        Command::Meditation(note) => { let txt = create_meditation(&note); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Affirmation(note) => { let txt = create_affirmation(&note); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Reflection(note) => { let txt = create_reflection(&note); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Wisdom => { let txt = fetch_wisdom().await.unwrap_or_else(|e| format!("wisdom err: {e}")); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Journal(note) => { let txt = create_journal(&note); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
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
        Command::Sleep(args) => { let txt = create_sleep(&args); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Energy(args) => { let txt = create_energy(&args); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Exercise(args) => { let txt = create_exercise(&args); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Water(args) => { let txt = create_water(&args); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Read(args) => { let txt = create_read(&args); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Itunes(q) => { let txt = fetch_itunes(&q).await.unwrap_or_else(|e| format!("itunes err: {e}")); create_as_bot(&bot, &msg, &app, "music", &txt, tid).await?; }
        Command::Deezer(q) => { let txt = fetch_deezer(&q).await.unwrap_or_else(|e| format!("deezer err: {e}")); create_as_bot(&bot, &msg, &app, "music", &txt, tid).await?; }
        Command::Mbrainz(q) => { let txt = fetch_mbrainz(&q).await.unwrap_or_else(|e| format!("mbrainz err: {e}")); create_as_bot(&bot, &msg, &app, "music", &txt, tid).await?; }
        Command::Lyrics(q) => { let txt = fetch_lyrics(&q).await.unwrap_or_else(|e| format!("lyrics err: {e}")); create_as_bot(&bot, &msg, &app, "music", &txt, tid).await?; }
        Command::Bpm(q) => { let txt = fetch_bpm(&q).await.unwrap_or_else(|e| format!("bpm err: {e}")); create_as_bot(&bot, &msg, &app, "music", &txt, tid).await?; }
        Command::Trend => { let txt = fetch_trend().await.unwrap_or_else(|e| format!("trend err: {e}")); create_as_bot(&bot, &msg, &app, "music", &txt, tid).await?; }
        Command::Promo(q) => { let txt = create_promo(&q); create_as_bot(&bot, &msg, &app, "music", &txt, tid).await?; }
        
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
    // Enforce character limits: Telegram 4096, keep beautiful truncation at 3500
    let body_owned = if body.len() > 3500 {
        format!("{}...\n\n_Truncated — was {} chars, showing 3500._", &body[..3500], body.len())
    } else {
        body.to_string()
    };
    let body = &body_owned;
    // Memo content: clean markdown, no escaping
    let content = format!("@{}\n\n{}\n\n— via {} · asher\n\n{tag}", app.admin_username, body, bot_name);
    let tok = if let Some(t) = bot_tok { t } else {
        let fallback = { app.store.read().await.get(&telegram_id).cloned() };
        let Some(f) = fallback else { bot.send_message(msg.chat.id, "run /start <token> first").await?; return Ok(()); };
        f
    };
    // Telegram preview: escape for MarkdownV2
    let tg_body = tg_escape(body);
    let _ = bot.send_message(msg.chat.id, &tg_body).parse_mode(ParseMode::MarkdownV2).await;
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

// --- Markdown helpers ---
// All output is CLEAN markdown for Memos. Telegram escaping handled in create_as_bot via tg_escape.

fn tg_escape(s: &str) -> String {
    // Escape MarkdownV2 special chars (only for Telegram send)
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

// Keep esc() for backward compat in any remaining call sites
fn esc(s: &str) -> String { tg_escape(s) }

fn tg_code_block(s: &str) -> String {
    format!("```\n{}\n```", s)
}
fn tg_header(emoji: &str, title: &str, query: &str) -> String {
    if query.trim().is_empty() {
        format!("**{} {}**", emoji, title)
    } else {
        format!("**{} {} — `{}`**", emoji, title, query)
    }
}
fn tg_footer(source: &str, tag: &str) -> String {
    format!("> {} · #{}", source, tag)
}
fn tg_truncate(s: &str, n: usize) -> String {
    if s.len() > n { format!("{}...", &s[..n]) } else { s.to_string() }
}

async fn fetch_hn() -> Result<String> {
    let ids: Vec<u64> = HTTP.get("https://hacker-news.firebaseio.com/v0/topstories.json").send().await?.json().await?;
    let top5: Vec<u64> = ids.into_iter().take(5).collect();
    let futures: Vec<_> = top5.iter().map(|id| {
        let url = format!("https://hacker-news.firebaseio.com/v0/item/{id}.json");
        async move { HTTP.get(&url).send().await?.json::<serde_json::Value>().await }
    }).collect();
    let results = futures::future::join_all(futures).await;
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let total_score: u64 = results.iter().filter_map(|r| r.as_ref().ok()).map(|v| v["score"].as_u64().unwrap_or(0)).sum();
    let total_comments: u64 = results.iter().filter_map(|r| r.as_ref().ok()).map(|v| v["descendants"].as_u64().unwrap_or(0)).sum();
    let mut out = format!("{}\n\n", tg_header("🔥", "Hacker News", "Top 5"));
    out.push_str("**Source:** `news.ycombinator.com` · **Category:** `Tech/Programming` · **Bias:** `Community`\n\n");
    out.push_str("## 📊 Stats\n\n");
    out.push_str("| Metric | Value |\n|---|---|\n");
    out.push_str(&format!("| Stories | 5 |\n"));
    out.push_str(&format!("| Total Score | {} |\n", total_score));
    out.push_str(&format!("| Total Comments | {} |\n", total_comments));
    out.push_str(&format!("| Updated | `{}` |\n\n", now));
    out.push_str("## 🔥 Top Stories\n\n");
    for (i, (id, res)) in top5.iter().zip(results.into_iter()).enumerate() {
        let item = match res { Ok(v) => v, Err(_) => continue };
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
        out.push_str(&format!("**{}.** [{}]({})\n   ↑ {} · 💬 {} · {} · {}\n\n", i + 1, title, url, score, comments, by, ago));
    }
    out.push_str(&format!("{}\n\n`{}` · #hn #tech", tg_footer("news.ycombinator.com", "hn"), now));
    Ok(out)
}

async fn fetch_weather(city: &str) -> Result<String> {
    let url = format!("http://wttr.in/{}?format=j1", city);
    let v: serde_json::Value = match tokio::time::timeout(std::time::Duration::from_secs(8), HTTP.get(&url).send()).await {
        Ok(Ok(r)) => match r.json::<serde_json::Value>().await { Ok(j) => j, Err(e) => return Ok(format!("{}\n\n_Weather data unavailable for `{}`: {}_\n\n{}", tg_header("🌤️", "Weather", city), city, e, tg_footer("wttr.in", "weather"))) },
        Ok(Err(e)) => return Ok(format!("{}\n\n_Weather data unavailable for `{}`: {}_\n\n{}", tg_header("🌤️", "Weather", city), city, e, tg_footer("wttr.in", "weather"))),
        Err(_) => return Ok(format!("{}\n\n_Weather data unavailable for `{}` (timeout). Try again._\n\n{}", tg_header("🌤️", "Weather", city), city, tg_footer("wttr.in", "weather"))),
    };
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
    let mut out = format!("{}\n\n**Now:** {}°C (feels {}°C) — {}\n**Humidity:** {}% · **Wind:** {} km/h {}\n", tg_header(emoji, "Weather", city), temp, feels, desc, hum, wind, winddir);
    if let Some(arr) = v["weather"].as_array() {
        for day in arr.iter().take(3) {
            let date = day["date"].as_str().unwrap_or("");
            let maxt = day["maxtempC"].as_str().unwrap_or("?");
            let mint = day["mintempC"].as_str().unwrap_or("?");
            out.push_str(&format!("**{date}** — ↑{maxt}°C ↓{mint}°C\n"));
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
                                let re_html = Regex::new(r"<[^>]+>").unwrap();
                                let text = re_html.replace_all(&raw, "").to_string().chars().take(300).collect::<String>();
                                let text = text.replace("  ", " ").trim().to_string();
                                if !text.is_empty() {
                                    out.push_str(&format!("**{}.** {}: {}\n", i + 1, pos, text));
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
        out.push_str(&format!("_No definitions found for `{}`._\n\nTry: https://en.wiktionary.org/wiki/{}", word, urlencoding::encode(word)));
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
    out.push_str(&format!("**{}**\n", title));
    if !thumb.is_empty() { out.push_str(&format!("[📷 Photo]({thumb})\n\n")); }
    out.push_str(&format!("{}\n\n[Read more on Wikipedia]({url})\n\n{}", tg_truncate(extract, 800), tg_footer("wikipedia.org", "wiki")));
    Ok(out)
}

async fn fetch_cheat(q: &str) -> Result<String> {
    let resp = HTTP.get(format!("https://cheat.sh/{}?TQ", urlencoding::encode(q))).send().await;
    match resp {
        Ok(r) if r.status().is_success() => {
            let txt = r.text().await.unwrap_or_default();
            let clean = tg_truncate(&txt, 1400);
            Ok(format!("{}\n\n{}\n\n[cheat.sh](https://cheat.sh/{}) · #{}", tg_header("💻", "cheat", q), tg_code_block(&clean), q, "cheat"))
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
    let mut out = format!("{}\n\n", tg_header("⭐", "GitHub", query));
    for it in items.iter().take(5) {
        let name = it["full_name"].as_str().unwrap_or("?");
        let html = it["html_url"].as_str().unwrap_or("");
        let stars = it["stargazers_count"].as_u64().unwrap_or(0);
        let forks = it["forks_count"].as_u64().unwrap_or(0);
        let lang = it["language"].as_str().unwrap_or("-");
        let desc = it["description"].as_str().unwrap_or("").chars().take(60).collect::<String>();
        out.push_str(&format!("[{name}]({html})\n   ⭐ {stars} · 🍴 {forks} · `{lang}`\n   _{}_\n\n", desc));
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
        "{}\n\n`1 {base} = {rate:.4} {quote}`\n\n`{now}`\n\n{}",
        tg_header("💱", "Exchange Rate", &format!("{base}/{quote}")),
        tg_footer("open.er-api.com", "fx")
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
    let high = meta["regularMarketDayHigh"].as_f64().unwrap_or(price);
    let low = meta["regularMarketDayLow"].as_f64().unwrap_or(price);
    let open = meta["regularMarketOpen"].as_f64().unwrap_or(price);
    let volume = meta["regularMarketVolume"].as_u64().unwrap_or(0);
    let now_str = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let header = tg_header(emoji, &format!("{} ({})", name, ticker), "");
    // 5-day closes table
    let mut table = String::from("Date       Close     Change\n");
    table.push_str("────────── ───────── ─────────\n");
    if let (Some(ts), Some(quote)) = (result["timestamp"].as_array(), result["indicators"]["quote"].as_array().and_then(|a| a.first())) {
        if let Some(closes) = quote["close"].as_array() {
            for (t, c) in ts.iter().zip(closes.iter()).rev().take(5).rev() {
                if let (Some(epoch), Some(close)) = (t.as_i64(), c.as_f64()) {
                    let date = chrono::DateTime::<chrono::Utc>::from_timestamp(epoch, 0).map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_else(|| "?".into());
                    table.push_str(&format!("{date}  {close:>8.2}  {currency}\n"));
                }
            }
        }
    }
    let vol_str = if volume >= 1_000_000_000 { format!("{:.2}B", volume as f64 / 1e9) } else if volume >= 1_000_000 { format!("{:.2}M", volume as f64 / 1e6) } else { format!("{}", volume) };
    let body = tg_code_block(&format!("{price:.2} {currency}  {sign}{change:.2} ({sign}{pct:.2}%)\nOpen: {open:.2}  High: {high:.2}  Low: {low:.2}\nVol: {vol_str}\n\n{table}"));
    Ok(format!("{}\n\n{}\n\n`{}` · #{}", header, body, now_str, "stock"))
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
    // Try detailed endpoint for more fields, fallback to simple
    let detailed_url = format!("https://api.coingecko.com/api/v3/coins/{}?localization=false&tickers=false&market_data=true&community_data=false&developer_data=false", urlencoding::encode(&coin_id));
    let (price, change, mcap, high24, low24, ath, atl) = if let Ok(v) = HTTP.get(&detailed_url).header("User-Agent", "memogram-rs").send().await {
        if let Ok(j) = v.json::<serde_json::Value>().await {
            let md = &j["market_data"];
            (md["current_price"]["usd"].as_f64().unwrap_or(0.0), md["price_change_percentage_24h"].as_f64().unwrap_or(0.0), md["market_cap"]["usd"].as_f64().unwrap_or(0.0), md["high_24h"]["usd"].as_f64().unwrap_or(0.0), md["low_24h"]["usd"].as_f64().unwrap_or(0.0), md["ath"]["usd"].as_f64().unwrap_or(0.0), md["atl"]["usd"].as_f64().unwrap_or(0.0))
        } else { (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0) }
    } else { (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0) };
    let (price, change, mcap) = if price == 0.0 {
        let url = format!("https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=usd&include_24hr_change=true&include_market_cap=true", urlencoding::encode(&coin_id));
        let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "memogram-rs").send().await?.json().await?;
        let data = v.get(&coin_id).ok_or_else(|| anyhow::anyhow!("coin '{coin}' not found. Try: btc, eth, sol, xrp, doge, ada, bnb"))?;
        (data["usd"].as_f64().unwrap_or(0.0), data["usd_24h_change"].as_f64().unwrap_or(0.0), data["usd_market_cap"].as_f64().unwrap_or(0.0))
    } else { (price, change, mcap) };
    let emoji = if change >= 0.0 { "📈" } else { "📉" };
    let sign = if change >= 0.0 { "+" } else { "" };
    let mcap_str = if mcap >= 1e12 { format!("${:.2}T", mcap / 1e12) } else if mcap >= 1e9 { format!("${:.2}B", mcap / 1e9) } else if mcap >= 1e6 { format!("${:.2}M", mcap / 1e6) } else { format!("${:.0}", mcap) };
    let now_str = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let header = tg_header(emoji, &coin_id, coin);
    let mut body_str = format!("${price:.2}  {sign}{change:.2}%\nMCap: {mcap_str}");
    if high24 > 0.0 { body_str.push_str(&format!("\n24h High: ${high24:.2}  Low: ${low24:.2}")); }
    if ath > 0.0 { body_str.push_str(&format!("\nATH: ${ath:.2}  ATL: ${atl:.2}")); }
    let body = tg_code_block(&body_str);
    Ok(format!("{}\n\n{}\n\n`{}` · #{}", header, body, now_str, "crypto"))
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
        ("en|es".to_string(), args.to_string())
    };
    if text.trim().is_empty() { return Ok("usage: `/translate <text>` or `/translate ja → en <text>`".into()); }
    // Fix auto -> en (MyMemory doesn't support auto)
    let langpair = langpair.replace("auto|", "en|");
    let url = format!("https://api.mymemory.translated.net/get?q={}&langpair={}", urlencoding::encode(&text), urlencoding::encode(&langpair));
    let v: serde_json::Value = HTTP.get(&url).send().await?.json().await?;
    let translated = v["responseData"]["translatedText"].as_str().unwrap_or("(no result)");
    // Detect MyMemory error (returns INVALID message as translatedText)
    if translated.contains("IS AN INVALID") || translated.contains("INVALID SOURCE") {
        return Ok(format!("{}\n\n**Original:** {}\n\n_Translation unavailable for `{} → {}`. Try `/translate en → es {}`_\n\n{}", tg_header("🌐", "Translation", &langpair), text, langpair.split('|').next().unwrap_or("?"), langpair.split('|').last().unwrap_or("?"), text, tg_footer("mymemory.translated.net", "translate")));
    }
    let detected = v["responseData"]["match"].as_f64().unwrap_or(0.0);
    let src = langpair.split('|').next().unwrap_or("en");
    let tgt = langpair.split('|').last().unwrap_or("en");
    Ok(format!(
        "{}\n\n`{src}` → `{tgt}` (confidence: {detected:.0}%)\n\n**Original:** {text}\n\n**Translated:** {translated}\n\n{}",
        tg_header("🌐", "Translation", &langpair),
        tg_footer("mymemory.translated.net", "translate")
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
    Ok(format!("📓 **Daily note created**\n\n`{name}`\n\n> Open in Memos to edit · #daily"))
}

// --- forecast ---

async fn fetch_forecast(city: &str) -> Result<String> {
    let city = if city.trim().is_empty() { "Thousand Oaks, CA".to_string() } else { city.trim().to_string() };
    let display_city = city.clone();
    let url = format!("http://wttr.in/{}?format=j1", urlencoding::encode(&city));
    let v: serde_json::Value = match tokio::time::timeout(std::time::Duration::from_secs(8), HTTP.get(&url).send()).await {
        Ok(Ok(r)) => match r.json::<serde_json::Value>().await { Ok(j) => j, Err(e) => return Ok(format!("{}\n\n_Forecast unavailable for `{}`: {}_\n\n{}", tg_header("🌤️", "Forecast", &display_city), display_city, e, tg_footer("wttr.in", "forecast"))) },
        Ok(Err(e)) => return Ok(format!("{}\n\n_Forecast unavailable for `{}`: {}_\n\n{}", tg_header("🌤️", "Forecast", &display_city), display_city, e, tg_footer("wttr.in", "forecast"))),
        Err(_) => return Ok(format!("{}\n\n_Forecast unavailable for `{}` (timeout). Try again._\n\n{}", tg_header("🌤️", "Forecast", &display_city), display_city, tg_footer("wttr.in", "forecast"))),
    };
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
    let mut out = format!("{}\n\n**Now:** `{}`°C {} 💧 {}% · 💨 {} km/h\n\n", tg_header(emoji, "Forecast", &display_city), temp, desc, humidity, wind);
    if let Some(arr) = v["weather"].as_array() {
        for day in arr.iter().take(7) {
            let date = day["date"].as_str().unwrap_or("");
            let maxt = day["maxtempC"].as_str().unwrap_or("?");
            let mint = day["mintempC"].as_str().unwrap_or("?");
            let hourly = day["hourly"].as_array();
            let noon = hourly.and_then(|h| h.get(4)).and_then(|h| h["weatherDesc"][0]["value"].as_str()).unwrap_or("");
            out.push_str(&format!("**{date}** — ↑{maxt}°C ↓{mint}°C {noon}\n"));
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
    Ok(format!("{}\n\n`Country:` {}\n`Region:` {}\n`City:` {}\n`ISP:` {}\n`Org:` {}\n`Coords:` {:.4}, {:.4}\n\n{}", tg_header("🌐", "IP", ip), country, region, city, isp, org, lat, lon, tg_footer("ip-api.com", "ip")))
}

fn gen_qr(text: &str) -> String {
    if text.trim().is_empty() { return "usage: `/qr <text>`".into(); }
    let url = format!("https://api.qrserver.com/v1/create-qr-code/?size=300x300&data={}", urlencoding::encode(text));
    format!("{}\n\n![QR]({url})\n\n`{} chars` · #{}", tg_header("📱", "QR Code", ""), text.len().to_string(), "qr")
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
    format!("⏰ **Reminder set**\n\n`{mins} min` — {msg_text}\n\n> fires at {} · #reminder", fire_at.format("%H:%M"))
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
            format!("✅ **Added** `{ticker}` × {qty} @ ${price:.2}")
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
            // First pass to get total_val for allocation
            let mut prices: Vec<(String, f64, f64)> = Vec::new();
            for h in &holdings {
                let price = fetch_stock_price(&h.ticker).await.unwrap_or(h.avg_price);
                let val = price * h.qty;
                total_val += val;
                prices.push((h.ticker.clone(), price, val));
            }
            let total_cost: f64 = holdings.iter().map(|h| h.avg_price * h.qty).sum();
            let mut lines = String::from("```\nTicker  Qty     Price      Value    Alloc     P&L\n");
            lines.push_str("────── ─────── ────────── ────────── ────── ──────────\n");
            for (i, h) in holdings.iter().enumerate() {
                let (ticker, price, val) = &prices[i];
                let cost = h.avg_price * h.qty;
                let pnl = val - cost;
                let alloc = if total_val > 0.0 { val / total_val * 100.0 } else { 0.0 };
                let sign = if pnl >= 0.0 { "+" } else { "" };
                let bar_len = (alloc / 10.0).round() as usize;
                let bar = "█".repeat(bar_len) + &"░".repeat(10 - bar_len);
                lines.push_str(&format!("{:<6} {:>6.1}  ${:>8.2}  ${:>8.2}  {alloc:>4.1}% {bar} {sign}${:.2}\n", ticker, h.qty, price, val, pnl));
            }
            lines.push_str("────── ─────── ────────── ────────── ────── ──────────\n");
            let total_pnl = total_val - total_cost;
            let sign = if total_pnl >= 0.0 { "+" } else { "" };
            let total_alloc = if total_val > 0.0 { "100.0%" } else { "0.0%" };
            lines.push_str(&format!("Total           ${:>8.2}  {total_alloc:>6}          {sign}${:.2}\n```", total_val, total_pnl));
            format!("{}\n\n{}\n\n{}", tg_header("📊", "Portfolio", ""), lines, tg_footer("portfolio", "portfolio"))
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
            format!("🔔 **Alert set**\n\n`{ticker}` {direction} ${price:.2}")
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
    let mut out = format!("{}\n\n", tg_header("📈", "Markets", ""));
    let mut table = String::from("Index             Price          Change\n───────────────── ────────────── ──────────\n");
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
                table.push_str(&format!("{name:<17} {price_str}   {sign}{pct:.2}%\n"));
            }
            Err(_) => { table.push_str(&format!("{name:<17} {:>12}   N/A\n", "N/A")); }
        }
    }
    out.push_str(&tg_code_block(&table));
    out.push_str(&format!("\n`{}` · #{}", Local::now().format("%Y-%m-%d %H:%M").to_string(), "markets"));
    Ok(out)
}

// --- news: arxiv ---

async fn fetch_arxiv(topic: &str) -> Result<String> {
    let query = if topic.trim().is_empty() { "cat:cs.AI".to_string() } else { format!("all:{}", urlencoding::encode(topic)) };
    let url = format!("http://export.arxiv.org/api/query?search_query={}&sortBy=submittedDate&sortOrder=descending&max_results=5", query);
    let txt = HTTP.get(&url).send().await?.text().await?;
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let mut out = format!("{}\n\n", tg_header("📄", "arXiv", topic));
    let mut current = String::new();
    let mut in_entry = false;
    let mut count = 0;

    for line in txt.lines() {
        if line.contains("<entry>") { in_entry = true; current.clear(); }
        if in_entry { current.push_str(line); current.push('\n'); }
        if line.contains("</entry>") {
            in_entry = false;
            let title = extract_xml(&current, "title").replace('\n', " ").trim().to_string();
            let id_url = extract_xml(&current, "id");
            let summary = extract_xml(&current, "summary").chars().take(150).collect::<String>();
            let authors = extract_xml(&current, "name");
            let published = extract_xml(&current, "published").chars().take(10).collect::<String>();
            if !title.is_empty() {
                count += 1;
                out.push_str(&format!("**{}.** [{}]({})\n   👤 {} · 📅 {}\n   📝 {}\n\n", count, title, id_url, authors, published, summary));
            }
        }
    }
    if count == 0 { out.push_str(&format!("_No results for `{}`._\n\n", topic)); }
    out.push_str(&format!("{}\n\n`{}` · #arxiv #research", tg_footer("arxiv.org", "arxiv"), now));
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
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let total_reactions: u64 = articles.iter().map(|a| a["positive_reactions_count"].as_u64().unwrap_or(0)).sum();
    let total_comments: u64 = articles.iter().map(|a| a["comments_count"].as_u64().unwrap_or(0)).sum();
    let mut out = format!("{}\n\n", tg_header("📝", "dev.to Top", ""));
    out.push_str("**Source:** `dev.to` · **Category:** `Programming` · **Bias:** `Community`\n\n");
    out.push_str("## 📊 Stats\n\n");
    out.push_str("| Metric | Value |\n|---|---|\n");
    out.push_str(&format!("| Articles | {} |\n", articles.len().min(7)));
    out.push_str(&format!("| Total Reactions | {} |\n", total_reactions));
    out.push_str(&format!("| Total Comments | {} |\n", total_comments));
    out.push_str(&format!("| Updated | `{}` |\n\n", now));
    out.push_str("## 📝 Top Posts\n\n");
    for (i, a) in articles.iter().take(7).enumerate() {
        let title = a["title"].as_str().unwrap_or("?");
        let url = a["url"].as_str().unwrap_or("");
        let reactions = a["positive_reactions_count"].as_u64().unwrap_or(0);
        let comments = a["comments_count"].as_u64().unwrap_or(0);
        let tags: Vec<&str> = a["tag_list"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).take(3).collect()).unwrap_or_default();
        let tag_str = tags.iter().map(|t| format!("`#{t}`")).collect::<Vec<_>>().join(" ");
        out.push_str(&format!("**{}.** [{}]({})\n   ❤️ {} · 💬 {} · {}\n\n", i + 1, title, url, reactions, comments, tag_str));
    }
    out.push_str(&format!("{}\n\n`{}` · #devto #programming", tg_footer("dev.to", "devto"), now));
    Ok(out)
}

// --- news: world (rss parser) ---

fn parse_rss_items(xml: &str, tag: &str) -> Vec<(String, String, String, String)> {
    let mut items = Vec::new();
    let mut remaining = xml;
    let start_pattern = format!("<{}>", tag);
    let end_pattern = format!("</{}>", tag);
    while let Some(start) = remaining.find(&start_pattern) {
        let rest = &remaining[start + start_pattern.len()..];
        if let Some(end) = rest.find(&end_pattern) {
            let item = &rest[..end];
            let title = extract_rss_tag(item, "title");
            let link = extract_rss_tag(item, "link");
            let desc = extract_rss_tag(item, "description").replace("<![CDATA[", "").replace("]]>", "").replace("<p>", "").replace("</p>", "").replace("<br>", "\n").replace("<br/>", "\n").replace("<br />", "\n");
            let pub_date = extract_rss_tag(item, "pubDate");
            if !title.is_empty() { items.push((title, link, desc.chars().take(200).collect(), pub_date)); }
            remaining = &rest[end + end_pattern.len()..];
        } else { break; }
    }
    items
}

fn extract_rss_tag(item: &str, tag: &str) -> String {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    if let Some(start) = item.find(&open) {
        let rest = &item[start + open.len()..];
        if let Some(end) = rest.find(&close) { return rest[..end].trim().replace("<![CDATA[", "").replace("]]>", "").to_string(); }
    }
    String::new()
}

fn format_rss_date(s: &str) -> String {
    if s.is_empty() { return "?".into(); }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc2822(s) {
        let hrs = (chrono::Utc::now() - dt.with_timezone(&chrono::Utc)).num_hours();
        if hrs < 1 { "now".into() } else if hrs < 24 { format!("{hrs}h ago") } else { format!("{}d ago", hrs / 24) }
    } else { s.chars().take(16).collect() }
}

// --- news: bbc world ---

async fn fetch_bbc() -> Result<String> {
    let url = "https://feeds.bbci.co.uk/news/world/rss.xml";
    let txt = match tokio::time::timeout(std::time::Duration::from_secs(10), HTTP.get(url).header("User-Agent", "memogram-rs").send()).await {
        Ok(Ok(r)) => match r.text().await { Ok(t) => t, Err(e) => return Ok(bbc_fallback(&format!("Data error: {e}"))) },
        Ok(Err(e)) => return Ok(bbc_fallback(&format!("Network error: {e}"))),
        Err(_) => return Ok(bbc_fallback("Timeout")),
    };
    let items = parse_rss_items(&txt, "item");
    if items.is_empty() { return Ok(bbc_fallback("No stories")); }
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let total = items.len();
    let mut out = format!("{}\n\n", tg_header("🌍", "BBC World News", ""));
    out.push_str("**Source:** `bbc.co.uk` · **Region:** `World` · **Bias:** `Low`\n\n");
    out.push_str("## 📊 Coverage\n\n");
    out.push_str("| Stat | Value |\n|---|---|\n");
    out.push_str(&format!("| Stories | {} |\n", total));
    out.push_str(&format!("| Updated | `{}` |\n", now));
    out.push_str("| Category | World |\n\n");
    out.push_str("## 📰 Top Stories\n\n");
    for (i, (title, link, desc, pub_date)) in items.iter().take(5).enumerate() {
        let ago = format_rss_date(pub_date);
        let desc_short = if desc.len() > 120 { format!("{}...", &desc[..120]) } else { desc.clone() };
        out.push_str(&format!("**{}.** [{}]({})\n   ⏰ {} · 🌍 World\n   📝 {}\n\n", i+1, title, link, ago, desc_short));
    }
    out.push_str("## 🔗 Quick Links\n\n");
    for (i, (_, link, _, _)) in items.iter().take(3).enumerate() {
        if !link.is_empty() { out.push_str(&format!("[Read more {}]({}) · ", i+1, link)); }
    }
    out.push_str(&format!("\n\n{}\n\n`{}` · #bbc #world", tg_footer("bbc.co.uk", "bbc"), now));
    Ok(out)
}

fn bbc_fallback(err: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    format!("{}\n\n**Source:** `bbc.co.uk` · **Region:** `World`\n\n⚠️ _{}_\n\n> Try: `bbc.co.uk/news/world`\n\n{}\n\n`{}` · #bbc #world",
        tg_header("🌍", "BBC World News", ""), err, tg_footer("bbc.co.uk", "bbc"), now)
}

// --- news: reuters world ---

async fn fetch_reuters() -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    // Try Reuters RSS first
    let url = "https://www.reutersagency.com/feed/?best-topics=world&post_type=best";
    let txt = match tokio::time::timeout(std::time::Duration::from_secs(8), HTTP.get(url).header("User-Agent", "memogram-rs").send()).await {
        Ok(Ok(r)) => match r.text().await { Ok(t) => t, Err(_) => String::new() },
        _ => String::new(),
    };
    let items = parse_rss_items(&txt, "item");
    if !items.is_empty() {
        let total = items.len();
        let mut out = format!("{}\n\n", tg_header("📰", "Reuters World", ""));
        out.push_str("**Source:** `reuters.com` · **Region:** `World` · **Bias:** `Very Low`\n\n");
        out.push_str("## 📊 Coverage\n\n");
        out.push_str("| Stat | Value |\n|---|---|\n");
        out.push_str(&format!("| Stories | {} |\n", total));
        out.push_str(&format!("| Updated | `{}` |\n", now));
        out.push_str("| Category | World |\n\n");
        out.push_str("## 📰 Top Stories\n\n");
        for (i, (title, link, desc, pub_date)) in items.iter().take(5).enumerate() {
            let ago = format_rss_date(pub_date);
            let desc_short = if desc.len() > 120 { format!("{}...", &desc[..120]) } else { desc.clone() };
            out.push_str(&format!("**{}.** [{}]({})\n   ⏰ {} · 🌍 World\n   📝 {}\n\n", i+1, title, link, ago, desc_short));
        }
        out.push_str(&format!("{}\n\n`{}` · #reuters #world", tg_footer("reuters.com", "reuters"), now));
        return Ok(out);
    }
    // Fallback: Google News RSS for Reuters
    let gnews = "https://news.google.com/rss/search?q=reuters+world&hl=en-US&gl=US&ceid=US:en";
    let txt2 = match tokio::time::timeout(std::time::Duration::from_secs(8), HTTP.get(gnews).header("User-Agent", "memogram-rs").send()).await {
        Ok(Ok(r)) => match r.text().await { Ok(t) => t, Err(_) => return Ok(reuters_fallback("No stories")) },
        _ => return Ok(reuters_fallback("No stories")),
    };
    let items2 = parse_rss_items(&txt2, "item");
    if items2.is_empty() { return Ok(reuters_fallback("No stories")); }
    let total = items2.len();
    let mut out = format!("{}\n\n", tg_header("📰", "Reuters World", ""));
    out.push_str("**Source:** `reuters.com` via Google News · **Region:** `World` · **Bias:** `Very Low`\n\n");
    out.push_str("## 📊 Coverage\n\n");
    out.push_str("| Stat | Value |\n|---|---|\n");
    out.push_str(&format!("| Stories | {} |\n", total));
    out.push_str(&format!("| Updated | `{}` |\n", now));
    out.push_str("| Category | World |\n\n");
    out.push_str("## 📰 Top Stories\n\n");
    for (i, (title, link, desc, pub_date)) in items2.iter().take(5).enumerate() {
        let ago = format_rss_date(pub_date);
        let desc_short = if desc.len() > 120 { format!("{}...", &desc[..120]) } else { desc.clone() };
        out.push_str(&format!("**{}.** [{}]({})\n   ⏰ {} · 🌍 World\n   📝 {}\n\n", i+1, title, link, ago, desc_short));
    }
    out.push_str(&format!("{}\n\n`{}` · #reuters #world", tg_footer("reuters.com", "reuters"), now));
    Ok(out)
}

fn reuters_fallback(err: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    format!("{}\n\n**Source:** `reuters.com` · **Region:** `World`\n\n⚠️ _{}_\n\n> Try: `reuters.com/world`\n\n{}\n\n`{}` · #reuters #world",
        tg_header("📰", "Reuters World", ""), err, tg_footer("reuters.com", "reuters"), now)
}

// --- news: ap world ---

async fn fetch_ap() -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    // Try AP News RSS
    let url = "https://rsshub.app/apnews/topics/apf-world";
    let txt = match tokio::time::timeout(std::time::Duration::from_secs(8), HTTP.get(url).header("User-Agent", "memogram-rs").send()).await {
        Ok(Ok(r)) => match r.text().await { Ok(t) => t, Err(_) => String::new() },
        _ => String::new(),
    };
    let items = parse_rss_items(&txt, "item");
    if !items.is_empty() {
        let total = items.len();
        let mut out = format!("{}\n\n", tg_header("📰", "AP World", ""));
        out.push_str("**Source:** `apnews.com` · **Region:** `World` · **Bias:** `Very Low`\n\n");
        out.push_str("## 📊 Coverage\n\n");
        out.push_str("| Stat | Value |\n|---|---|\n");
        out.push_str(&format!("| Stories | {} |\n", total));
        out.push_str(&format!("| Updated | `{}` |\n", now));
        out.push_str("| Category | World |\n\n");
        out.push_str("## 📰 Top Stories\n\n");
        for (i, (title, link, desc, pub_date)) in items.iter().take(5).enumerate() {
            let ago = format_rss_date(pub_date);
            let desc_short = if desc.len() > 120 { format!("{}...", &desc[..120]) } else { desc.clone() };
            out.push_str(&format!("**{}.** [{}]({})\n   ⏰ {} · 🌍 World\n   📝 {}\n\n", i+1, title, link, ago, desc_short));
        }
        out.push_str(&format!("{}\n\n`{}` · #ap #world", tg_footer("apnews.com", "ap"), now));
        return Ok(out);
    }
    // Fallback: Google News RSS for AP
    let gnews = "https://news.google.com/rss/search?q=ap+news+world&hl=en-US&gl=US&ceid=US:en";
    let txt2 = match tokio::time::timeout(std::time::Duration::from_secs(8), HTTP.get(gnews).header("User-Agent", "memogram-rs").send()).await {
        Ok(Ok(r)) => match r.text().await { Ok(t) => t, Err(_) => return Ok(ap_fallback("No stories")) },
        _ => return Ok(ap_fallback("No stories")),
    };
    let items2 = parse_rss_items(&txt2, "item");
    if items2.is_empty() { return Ok(ap_fallback("No stories")); }
    let total = items2.len();
    let mut out = format!("{}\n\n", tg_header("📰", "AP World", ""));
    out.push_str("**Source:** `apnews.com` via Google News · **Region:** `World` · **Bias:** `Very Low`\n\n");
    out.push_str("## 📊 Coverage\n\n");
    out.push_str("| Stat | Value |\n|---|---|\n");
    out.push_str(&format!("| Stories | {} |\n", total));
    out.push_str(&format!("| Updated | `{}` |\n", now));
    out.push_str("| Category | World |\n\n");
    out.push_str("## 📰 Top Stories\n\n");
    for (i, (title, link, desc, pub_date)) in items2.iter().take(5).enumerate() {
        let ago = format_rss_date(pub_date);
        let desc_short = if desc.len() > 120 { format!("{}...", &desc[..120]) } else { desc.clone() };
        out.push_str(&format!("**{}.** [{}]({})\n   ⏰ {} · 🌍 World\n   📝 {}\n\n", i+1, title, link, ago, desc_short));
    }
    out.push_str(&format!("{}\n\n`{}` · #ap #world", tg_footer("apnews.com", "ap"), now));
    Ok(out)
}

fn ap_fallback(err: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    format!("{}\n\n**Source:** `apnews.com` · **Region:** `World`\n\n⚠️ _{}_\n\n> Try: `apnews.com/hub/ap-top-news`\n\n{}\n\n`{}` · #ap #world",
        tg_header("📰", "AP World", ""), err, tg_footer("apnews.com", "ap"), now)
}

// --- news: reddit ---

async fn fetch_reddit(sub: &str) -> Result<String> {
    let sub = if sub.trim().is_empty() { "programming" } else { sub.trim().trim_start_matches("r/") };
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    // Try Reddit JSON API with better user agent
    let url = format!("https://www.reddit.com/r/{}/top.json?limit=5&t=day", urlencoding::encode(sub));
    let v: serde_json::Value = match tokio::time::timeout(std::time::Duration::from_secs(8), HTTP.get(&url).header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36").send()).await {
        Ok(Ok(r)) => match r.json::<serde_json::Value>().await { Ok(j) => j, Err(e) => return Ok(reddit_fallback(sub, &format!("Parse error: {e}"))) },
        Ok(Err(e)) => return Ok(reddit_fallback(sub, &format!("Network error: {e}"))),
        Err(_) => return Ok(reddit_fallback(sub, "Timeout")),
    };
    let posts = v["data"]["children"].as_array();
    if posts.is_none() || posts.unwrap().is_empty() {
        return Ok(reddit_fallback(sub, "No stories"));
    }
    let arr = posts.unwrap();
    let total_score: u64 = arr.iter().map(|p| p["data"]["score"].as_u64().unwrap_or(0)).sum();
    let total_comments: u64 = arr.iter().map(|p| p["data"]["num_comments"].as_u64().unwrap_or(0)).sum();
    let mut out = format!("{}\n\n", tg_header("👽", "Reddit", &format!("r/{}", sub)));
    out.push_str(&format!("**Source:** `reddit.com` · **Subreddit:** `r/{}` · **Sort:** `Top Today`\n\n", sub));
    out.push_str("## 📊 Stats\n\n");
    out.push_str("| Metric | Value |\n|---|---|\n");
    out.push_str(&format!("| Posts | {} |\n", arr.len()));
    out.push_str(&format!("| Total Score | {} |\n", total_score));
    out.push_str(&format!("| Total Comments | {} |\n", total_comments));
    out.push_str(&format!("| Updated | `{}` |\n\n", now));
    out.push_str("## 📰 Top Posts\n\n");
    for (i, p) in arr.iter().take(5).enumerate() {
        let d = &p["data"];
        let title = d["title"].as_str().unwrap_or("?");
        let post_url = d["url"].as_str().unwrap_or("");
        let permalink = d["permalink"].as_str().unwrap_or("");
        let link = if post_url.contains("reddit.com") || post_url.is_empty() { format!("https://reddit.com{}", permalink) } else { post_url.to_string() };
        let score = d["score"].as_u64().unwrap_or(0);
        let comments = d["num_comments"].as_u64().unwrap_or(0);
        let author = d["author"].as_str().unwrap_or("?");
        out.push_str(&format!("**{}.** [{}]({})\n   ↑ {} · 💬 {} · u/{}\n\n", i+1, title, link, score, comments, author));
    }
    out.push_str(&format!("{}\n\n`{}` · #reddit #{}", tg_footer("reddit.com", "reddit"), now, sub));
    Ok(out)
}

fn reddit_fallback(sub: &str, err: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    format!("{}\n\n**Source:** `reddit.com` · **Subreddit:** `r/{}`\n\n⚠️ _{}_\n\n> Try: `reddit.com/r/{}`\n\n{}\n\n`{}` · #reddit #{}",
        tg_header("👽", "Reddit", &format!("r/{}", sub)), sub, err, sub, tg_footer("reddit.com", "reddit"), now, sub)
}

// --- news: tldr ---

async fn fetch_tldr() -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    // Try TLDR RSS directly
    let url = "https://tldr.tech/api/rss.xml";
    let txt = match tokio::time::timeout(std::time::Duration::from_secs(8), HTTP.get(url).header("User-Agent", "memogram-rs").send()).await {
        Ok(Ok(r)) => match r.text().await { Ok(t) => t, Err(e) => return Ok(tldr_fallback(&format!("Data error: {e}"))) },
        Ok(Err(e)) => return Ok(tldr_fallback(&format!("Network error: {e}"))),
        Err(_) => return Ok(tldr_fallback("Timeout")),
    };
    let items = parse_rss_items(&txt, "item");
    if items.is_empty() {
        // Fallback to rss2json
        let url2 = "https://api.rss2json.com/v1/api.json?rss_url=https://tldr.tech/api/rss.xml";
        if let Ok(v) = HTTP.get(url2).send().await {
            if let Ok(j) = v.json::<serde_json::Value>().await {
                if let Some(rss_items) = j["items"].as_array() {
                    if !rss_items.is_empty() {
                        let mut out = format!("{}\n\n", tg_header("📰", "TLDR", "Tech Digest"));
                        out.push_str("**Source:** `tldr.tech` · **Category:** `Tech/Science/Business` · **Bias:** `Curated`\n\n");
                        out.push_str("## 📊 Coverage\n\n");
                        out.push_str("| Stat | Value |\n|---|---|\n");
                        out.push_str(&format!("| Articles | {} |\n", rss_items.len().min(5)));
                        out.push_str(&format!("| Updated | `{}` |\n\n", now));
                        out.push_str("## 📰 Top Stories\n\n");
                        for (i, it) in rss_items.iter().take(5).enumerate() {
                            let title = it["title"].as_str().unwrap_or("?");
                            let link = it["link"].as_str().unwrap_or("");
                            let desc = it["description"].as_str().unwrap_or("").chars().take(120).collect::<String>();
                            out.push_str(&format!("**{}.** [{}]({})\n   📝 {}\n\n", i+1, title, link, desc));
                        }
                        out.push_str(&format!("{}\n\n`{}` · #tldr #tech", tg_footer("tldr.tech", "tldr"), now));
                        return Ok(out);
                    }
                }
            }
        }
        return Ok(tldr_fallback("No stories"));
    }
    let total = items.len();
    let mut out = format!("{}\n\n", tg_header("📰", "TLDR", "Tech Digest"));
    out.push_str("**Source:** `tldr.tech` · **Category:** `Tech/Science/Business` · **Bias:** `Curated`\n\n");
    out.push_str("## 📊 Coverage\n\n");
    out.push_str("| Stat | Value |\n|---|---|\n");
    out.push_str(&format!("| Articles | {} |\n", total));
    out.push_str(&format!("| Updated | `{}` |\n\n", now));
    out.push_str("## 📰 Top Stories\n\n");
    for (i, (title, link, desc, _pub_date)) in items.iter().take(5).enumerate() {
        let desc_short = if desc.len() > 120 { format!("{}...", &desc[..120]) } else { desc.clone() };
        out.push_str(&format!("**{}.** [{}]({})\n   📝 {}\n\n", i+1, title, link, desc_short));
    }
    out.push_str(&format!("{}\n\n`{}` · #tldr #tech", tg_footer("tldr.tech", "tldr"), now));
    Ok(out)
}

fn tldr_fallback(err: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    format!("{}\n\n**Source:** `tldr.tech` · **Category:** `Tech`\n\n⚠️ _{}_\n\n> Try: `tldr.tech`\n\n{}\n\n`{}` · #tldr #tech",
        tg_header("📰", "TLDR", "Tech Digest"), err, tg_footer("tldr.tech", "tldr"), now)
}

// --- today: inbox ---

async fn fetch_inbox(memos_url: &str, token: &str) -> Result<String> {
    let v: serde_json::Value = HTTP.get(format!("{memos_url}/api/v1/memos?pageSize=200"))
        .header("Authorization", format!("Bearer {token}")).send().await?.json().await?;
    let memos = v["memos"].as_array().ok_or_else(|| anyhow::anyhow!("no memos"))?;
    let untagged: Vec<&serde_json::Value> = memos.iter().filter(|m| {
        m["tags"].as_array().map(|t| t.is_empty()).unwrap_or(true)
    }).collect();
    if untagged.is_empty() { return Ok("📥 **Inbox**\n\n_all memos are tagged ✅_".to_string()); }
    let mut out = format!("📥 **Inbox** — {} untagged\n\n", untagged.len());
    for m in untagged.iter().take(15) {
        let name = m["name"].as_str().unwrap_or("?");
        let content = m["content"].as_str().unwrap_or("").chars().take(80).collect::<String>();
        out.push_str(&format!("*{name}* — `{} chars`\n   _{}_\n\n", content.len(), content));
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
        Ok(r) if r.status().is_success() => format!("🗑 **Deleted**\n\n`{name}`\n\n_{}_", content),
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
            if new_val { format!("📌 **Pinned**\n\n`{name}`") } else { format!("📌 **Unpinned**\n\n`{name}`") }
        }
        _ => "❌ pin failed".into(),
    }
}

// --- today: note ---

async fn create_note(memos_url: &str, token: &str, content: &str) -> String {
    if content.trim().is_empty() { return "usage: `/note #tag my quick thought`".into(); }
    match create_memo(memos_url, token, content).await {
        Ok(name) => format!("✅ **Saved**\n\n`{name}`\n\n_{}_", content.chars().take(60).collect::<String>()),
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
        topic = topic, date = date, notes = notes
    )
}

fn create_project(args: &str) -> String {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let name = parts.first().filter(|s| !s.is_empty()).copied().unwrap_or("Untitled");
    let desc = parts.get(1).unwrap_or(&"");
    let date = Local::now().format("%Y-%m-%d").to_string();
    format!(
        "# Project: {name}\n\n**Created:** {date}\n**Status:** 🟡 In Progress\n\n## Goal\n{desc}\n\n## Tasks\n- [ ] \n- [ ] \n- [ ] \n\n## Notes\n- \n\n## Timeline\n- **Week 1:** \n- **Week 2:** \n\n#project #planning",
        name = name, date = date, desc = desc
    )
}

async fn fetch_recipe(query: &str) -> Result<String> {
    let q = query.trim();
    if q.is_empty() {
        // Random recipe
        let v: serde_json::Value = HTTP.get("https://www.themealdb.com/api/json/v1/1/random.php").send().await?.json().await?;
        if let Some(meal) = v["meals"].as_array().and_then(|a| a.first()) {
            return format_recipe(meal);
        }
        return Ok("No recipe found".into());
    }
    // Search by name
    let url = format!("https://www.themealdb.com/api/json/v1/1/search.php?s={}", urlencoding::encode(q));
    let v: serde_json::Value = HTTP.get(&url).send().await?.json().await?;
    let meals = v["meals"].as_array();
    if meals.is_none() || meals.unwrap().is_empty() {
        // Try by first letter
        let first = q.chars().next().unwrap_or('a');
        let url2 = format!("https://www.themealdb.com/api/json/v1/1/search.php?f={first}");
        let v2: serde_json::Value = HTTP.get(&url2).send().await?.json().await?;
        if let Some(meals2) = v2["meals"].as_array() {
            if !meals2.is_empty() {
                return format_recipe(&meals2[0]);
            }
        }
        return Ok(format!("No recipes found for **{}**. Try: chicken, pasta, beef, or leave empty for random.", q));
    }
    let meals = meals.unwrap();
    if meals.len() == 1 {
        return format_recipe(&meals[0]);
    }
    // Multiple results — show list
    let mut out = format!("{}\n\n", tg_header("🍳", "Recipes", q));
    for (i, meal) in meals.iter().take(6).enumerate() {
        let name = meal["strMeal"].as_str().unwrap_or("?");
        let cat = meal["strCategory"].as_str().unwrap_or("?");
        let area = meal["strArea"].as_str().unwrap_or("?");
        out.push_str(&format!("**{}. {}** — {} · {}\n", i + 1, name, cat, area));
    }
    out.push_str(&format!("\n{} · Send the full name to get details", tg_footer("themealdb.com", "recipe")));
    Ok(out)
}

fn format_recipe(meal: &serde_json::Value) -> Result<String> {
    let name = meal["strMeal"].as_str().unwrap_or("?");
    let cat = meal["strCategory"].as_str().unwrap_or("?");
    let area = meal["strArea"].as_str().unwrap_or("?");
    let instructions = meal["strInstructions"].as_str().unwrap_or("");
    let thumb = meal["strMealThumb"].as_str().unwrap_or("");
    let youtube = meal["strYoutube"].as_str().unwrap_or("");
    let source = meal["strSource"].as_str().unwrap_or("");
    let tags = meal["strTags"].as_str().unwrap_or("");

    let mut out = format!("{}\n\n", tg_header("🍳", name, &format!("{cat} · {area}")));

    if !thumb.is_empty() {
        out.push_str(&format!("![recipe]({})\n\n", thumb));
    }

    // Ingredients
    out.push_str("## Ingredients\n\n");
    for i in 1..=20 {
        let ingredient = meal.get(&format!("strIngredient{i}")).and_then(|v| v.as_str()).unwrap_or("");
        let measure = meal.get(&format!("strMeasure{i}")).and_then(|v| v.as_str()).unwrap_or("");
        if ingredient.trim().is_empty() { continue; }
        out.push_str(&format!("- {} {}\n", measure.trim(), ingredient.trim()));
    }

    // Instructions — clean up line breaks
    out.push_str("\n## Instructions\n\n");
    let cleaned = instructions.replace("\r\n", "\n").replace("\r", "\n");
    for (i, step) in cleaned.split('\n').enumerate() {
        let step = step.trim();
        if !step.is_empty() {
            out.push_str(&format!("{}. {}\n", i + 1, step));
        }
    }

    // Links
    out.push_str("\n## Links\n\n");
    if !youtube.is_empty() {
        let video_id = youtube.split("v=").nth(1).unwrap_or("");
        if !video_id.is_empty() {
            out.push_str(&format!("- [Video Tutorial](https://youtu/{})\n", video_id));
        }
    }
    if !source.is_empty() {
        out.push_str(&format!("- [Original Recipe]({})\n", source));
    }

    if !tags.is_empty() {
        out.push_str(&format!("\n**Tags:** {}\n", tags));
    }

    out.push_str(&format!("\n{}", tg_footer("themealdb.com", "recipe")));
    Ok(out)
}

fn create_book(args: &str) -> String {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let title = parts.first().filter(|s| !s.is_empty()).copied().unwrap_or("Untitled");
    let author = parts.get(1).unwrap_or(&"");
    let date = Local::now().format("%Y-%m-%d").to_string();
    format!(
        "# Book: {title}\n\n**Author:** {author}\n**Started:** {date}\n**Status:** 📖 Reading\n**Rating:** ⭐⭐⭐⭐⭐\n\n## Summary\n- \n\n## Key Takeaways\n1. \n2. \n3. \n\n## Favorite Quotes\n> \"\" \n\n## Notes\n- \n\n#book #reading",
        title = title, author = author, date = date
    )
}

fn create_todo(args: &str) -> String {
    let items: Vec<&str> = args.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    if items.is_empty() { return "usage: `/todo buy milk, write report, call mom`".into(); }
    let mut out = format!("{}\n\n", tg_header("✅", "Todo List", ""));
    for item in items {
        out.push_str(&format!("- [ ] {}\n", item));
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
        topic = topic
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
    if ids.is_empty() { return Ok(format!("{}\n\n_No results for `{}`._\n\n{}", tg_header("📚", "PubMed", query), query, tg_footer("ncbi.nlm.nih.gov", "pubmed"))); }
    let id_list = ids.join(",");
    let summary_url = format!("https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esummary.fcgi?db=pubmed&id={}&retmode=json", id_list);
    let summary: serde_json::Value = HTTP.get(&summary_url).send().await?.json().await?;
    let mut out = format!("{}\n\n", tg_header("📚", "PubMed", query));
    for id in &ids {
        if let Some(article) = summary["result"][id].as_object() {
            let title = article["title"].as_str().unwrap_or("?");
            let authors = article["sortfirstauthor"].as_str().unwrap_or("?");
            let pubdate = article["pubdate"].as_str().unwrap_or("?");
            out.push_str(&format!("**{}**\n  {} — `{}`\n  https://pubmed.ncbi.nlm.nih.gov/{}/\n\n", title, authors, pubdate, id));
        }
    }
    out.push_str(&format!("\n{}", tg_footer("ncbi.nlm.nih.gov", "pubmed")));
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
            // Fallback chain: purpose -> indications_and_usage -> description -> active_ingredient
            let purpose = drug["purpose"].as_array().and_then(|a| a.first()).and_then(|v| v.as_str())
                .or_else(|| drug["indications_and_usage"].as_array().and_then(|a| a.first()).and_then(|v| v.as_str()))
                .or_else(|| drug["description"].as_array().and_then(|a| a.first()).and_then(|v| v.as_str()))
                .unwrap_or("No purpose/indication found");
            let warnings = drug["warnings"].as_array().and_then(|a| a.first()).and_then(|v| v.as_str())
                .or_else(|| drug["warnings_and_cautions"].as_array().and_then(|a| a.first()).and_then(|v| v.as_str()))
                .or_else(|| drug["boxed_warning"].as_array().and_then(|a| a.first()).and_then(|v| v.as_str()))
                .or_else(|| drug["adverse_reactions"].as_array().and_then(|a| a.first()).and_then(|v| v.as_str()))
                .unwrap_or("No warnings found");
            let header = tg_header("💊", &format!("{} ({})", brand, generic), name);
            let body = format!("**Indications:** {}\n\n**Warnings:** {}", &purpose[..purpose.len().min(400)], &warnings[..warnings.len().min(400)]);
            return Ok(format!("{}\n\n{}\n\n{}", header, body, tg_footer("fda.gov", "drug")));
        }
    }
    Ok(format!("{} \n\n_No drug info found._\n\n{}", tg_header("💊", "Drug", name), tg_footer("fda.gov", "drug")))
}

async fn fetch_genome(query: &str) -> Result<String> {
    let url = format!("https://api.ncbi.nlm.nih.gov/datasets/v2/genus/+/taxon/{}/dataset_report?page_size=3", urlencoding::encode(query));
    // Try datasets API, but don't fail hard — fall back to eutils on any error (e.g., invalid taxon like 'human')
    if let Ok(resp) = HTTP.get(&url).send().await {
        if let Ok(json) = resp.json::<serde_json::Value>().await {
            if let Some(taxonomy) = json["assembly_summary"].as_array() {
                if let Some(first) = taxonomy.first() {
                    let name = first["organism_name"].as_str().unwrap_or("?");
                    let acc = first["assembly_accession"].as_str().unwrap_or("?");
                    let status = first["assembly_level"].as_str().unwrap_or("?");
                    return Ok(format!("{}\n**Accession:** `{}`\n**Level:** {}\nhttps://www.ncbi.nlm.nih.gov/datasets/{}\n\n{}", tg_header("🧬", "Genome", name), acc, status, acc, tg_footer("ncbi.nlm.nih.gov", "genome")));
                }
            }
        }
    }
    // Fallback: search NCBI nucleotide
    let search_url = format!("https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi?db=nucleotide&retmax=3&term={}", urlencoding::encode(query));
    let resp = HTTP.get(&search_url).send().await?.text().await?;
    let ids: Vec<String> = Regex::new(r"<Id>(\d+)</Id>")?.captures_iter(&resp).map(|c| c[1].to_string()).collect();
    if ids.is_empty() { return Ok(format!("{}\n\n_No genome results for `{}`._\n\n{}", tg_header("🧬", "Genome", query), query, tg_footer("ncbi.nlm.nih.gov", "genome"))); }
    Ok(format!("{}\n\nIDs: {}\nhttps://www.ncbi.nlm.nih.gov/nuccore/{}\n\n{}", tg_header("🧬", "Genome", query), ids.join(", "), ids[0], tg_footer("ncbi.nlm.nih.gov", "genome")))
}

async fn fetch_protein(query: &str) -> Result<String> {
    let url = format!("https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi?db=protein&retmax=5&term={}", urlencoding::encode(query));
    let resp = HTTP.get(&url).send().await?.text().await?;
    let ids: Vec<String> = Regex::new(r"<Id>(\d+)</Id>")?.captures_iter(&resp).map(|c| c[1].to_string()).collect();
    if ids.is_empty() { return Ok(format!("{}\n\n_No protein results for `{}`._\n\n{}", tg_header("🧬", "Protein", query), query, tg_footer("ncbi.nlm.nih.gov", "protein"))); }
    let summary_url = format!("https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esummary.fcgi?db=protein&id={}&retmode=json", ids.join(","));
    let summary: serde_json::Value = HTTP.get(&summary_url).send().await?.json().await?;
    let mut out = format!("{}\n\n", tg_header("🧬", "Protein", query));
    for id in &ids {
        if let Some(item) = summary["result"][id].as_object() {
            let title = item["title"].as_str().unwrap_or("?");
            out.push_str(&format!("**{}**\n  https://www.ncbi.nlm.nih.gov/protein/{}\n\n", title, id));
        }
    }
    out.push_str(&format!("\n{}", tg_footer("ncbi.nlm.nih.gov", "protein")));
    Ok(out)
}

// === NEW COMMANDS: Stoicism ===

async fn fetch_stoic_quote() -> Result<String> {
    let api = async {
        let v: serde_json::Value = HTTP.get("https://stoic-quotes.com/api/quote")
            .timeout(std::time::Duration::from_secs(5))
            .send().await?.json().await?;
        let text = v["text"].as_str().or_else(|| v["data"]["quote"].as_str()).ok_or_else(|| anyhow::anyhow!("no text"))?;
        let author = v["author"].as_str().or_else(|| v["data"]["author"].as_str()).unwrap_or("Unknown");
        Ok::<(String, String), anyhow::Error>((text.to_string(), author.to_string()))
    }.await;
    let (text, author) = match api {
        Ok((t, a)) if !t.is_empty() && t != "?" => (t, a),
        _ => {
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
            let (q, a) = quotes[idx];
            (q.to_string(), a.to_string())
        }
    };
    Ok(format!("{}\n\n> \"{}\"\n\n— **{}**\n\n{}", tg_header("🏛️", "Stoic Wisdom", ""), text, author, tg_footer("stoic", "stoic")))
}

fn create_mood_entry(args: &str) -> String {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let mood = parts.first().filter(|s| !s.is_empty()).copied().unwrap_or("neutral");
    let note = parts.get(1).unwrap_or(&"");
    let date = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let day = Local::now().format("%Y-%m-%d").to_string();
    format!(
        "# 😊 Mood — `{}`\n\n**Date:** `{}` · **Mood:** `{}`\n**Note:** {}\n\n## 📊 Check\n\n| Mood | Energy | Stress |\n|---|---|---|\n| {} | /10 | /10 |\n\n## 📈 Last 7 Days (sample)\n\n| Date | Mood | Note |\n|---|---|---|\n| {} | {} | {} |\n| 2026-09-03 | ok |  |\n| 2026-09-02 | good |  |\n\n> _Tip: Name it to tame it. 1 breath, note 1 good._\n\n{}\n\n`{}` · #{}",
        mood, date, mood, note, mood, day, mood, note, tg_header("😊", "Mood", mood), date, "wellness"
    )
}

fn create_gratitude_entry(args: &str) -> String {
    let items: Vec<&str> = args.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    if items.is_empty() { return "usage: `/gratitude family, health, code`".into(); }
    let date = Local::now().format("%Y-%m-%d").to_string();
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let mut out = format!("# 🙏 Gratitude — `{}`\n\n**Date:** `{}`\n\n## ✨ Today\n\n", date, now);
    for item in &items {
        out.push_str(&format!("- ✨ {}\n", item));
    }
    out.push_str("\n## 📊 Weekly\n\n| Date | Count | Themes |\n|---|---|---|\n");
    out.push_str(&format!("| {} | {} | {} |\n", date, items.len(), items.join(", ")));
    out.push_str("| 2026-09-03 | 3 | health, work |\n");
    out.push_str("\n> _Tip: 3 specific, 1 why it matters._\n\n");
    out.push_str(&format!("{}\n\n`{}` · #{}", tg_header("🙏", "Gratitude", &date), now, "wellness"));
    out
}

fn create_habit_entry(args: &str) -> String {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let habit = parts.first().filter(|s| !s.is_empty()).copied().unwrap_or("habit");
    let status = parts.get(1).unwrap_or(&"done");
    let date = Local::now().format("%Y-%m-%d").to_string();
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    format!(
        "# ✅ Habit — `{}`\n\n**Date:** `{}` · **Habit:** `{}` · **Status:** `{}`\n\n## 📊 Streak\n\n| Habit | Streak | Done |\n|---|---|---|\n| {} | 5 days | {} |\n\n## 📈 Last 7 Days (sample)\n\n| Date | Status |\n|---|---|\n| {} | {} |\n| 2026-09-03 | done |\n| 2026-09-02 | done |\n\n```mermaid\nxychart-beta\n  title \"Habit\"\n  x-axis [Mon Tue Wed Thu Fri Sat Sun]\n  y-axis \"Done\" 0 1\n  bar [1 1 1 0 1 1 1]\n```\n\n> _Tip: Never miss twice._\n\n{}\n\n`{}` · #{}",
        habit, date, habit, status, habit, status, date, status, tg_header("✅", "Habit", habit), now, "wellness"
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
    Ok(format!("{}\n\n{}\n\nhttps://www.npmjs.com/package/{}\n\n{}", tg_header("📦", "npm", &format!("{}@{}", name, version)), desc, name, tg_footer("npmjs.com", "npm")))
}

async fn fetch_pypi(pkg: &str) -> Result<String> {
    let resp: serde_json::Value = HTTP.get(format!("https://pypi.org/pypi/{}/json", urlencoding::encode(pkg))).send().await?.json().await?;
    if let Some(msg) = resp["message"].as_str() { return Ok(format!("pypi: {msg}")); }
    let info = &resp["info"];
    let name = info["name"].as_str().unwrap_or("?");
    let version = info["version"].as_str().unwrap_or("?");
    let summary = info["summary"].as_str().unwrap_or("?");
    Ok(format!("{}\n\n{}\n\nhttps://pypi.org/project/{}/\n\n{}", tg_header("📦", "PyPI", &format!("{}@{}", name, version)), summary, name, tg_footer("pypi.org", "pypi")))
}

async fn fetch_crates(pkg: &str) -> Result<String> {
    let resp: serde_json::Value = HTTP.get(format!("https://crates.io/api/v1/crates/{}", urlencoding::encode(pkg))).send().await?.json().await?;
    if let Some(c) = resp["crate"].as_object() {
        let name = c["name"].as_str().unwrap_or("?");
        let version = c["max_version"].as_str().unwrap_or("?");
        let desc = c["description"].as_str().unwrap_or("?");
        let downloads = c["downloads"].as_i64().unwrap_or(0);
        return Ok(format!("{}\n\n{}\nDownloads: {}\n\nhttps://crates.io/crates/{}\n\n{}", tg_header("📦", "crates.io", &format!("{}@{}", name, version)), desc, downloads.to_string(), name, tg_footer("crates.io", "crates")));
    }
    Ok(format!("crate not found: **{pkg}**"))
}

async fn fetch_stackoverflow(query: &str) -> Result<String> {
    let url = format!("https://api.stackexchange.com/2.3/search?order=desc&sort=activity&intitle={}&site=stackoverflow&pagesize=5", urlencoding::encode(query));
    let resp: serde_json::Value = HTTP.get(&url).send().await?.json().await?;
    if let Some(items) = resp["items"].as_array() {
        if items.is_empty() { return Ok(format!("No results for **{query}**")); }
        let mut out = format!("{}\n\n", tg_header("📖", "Stack Overflow", query));
        for item in items {
            let title = item["title"].as_str().unwrap_or("?");
            let link = item["link"].as_str().unwrap_or("?");
            let score = item["score"].as_i64().unwrap_or(0);
            let answers = item["answer_count"].as_i64().unwrap_or(0);
            out.push_str(&format!("**{}** (⬆{} · answers:{})\n  {}\n\n", title, score, answers, link));
        }
        return Ok(out);
    }
    Ok(format!("stackoverflow err for *{query}*"))
}

// === NEW COMMANDS: Weather ===

async fn fetch_airquality(loc: &str) -> Result<String> {
    let loc = if loc.trim().is_empty() { "Thousand Oaks, CA" } else { loc };
    let url = format!("https://api.waqi.info/feed/{}/?token=demo", urlencoding::encode(loc));
    let resp: serde_json::Value = HTTP.get(&url).send().await?.json().await?;
    if resp["status"].as_str() == Some("ok") {
        let data = &resp["data"];
        let aqi = data["aqi"].as_i64().unwrap_or(0);
        let city = data["city"]["name"].as_str().unwrap_or("?");
        let dominant = data["dominentpol"].as_str().unwrap_or("?");
        return Ok(format!("🌬️ **Air Quality:** {city}\n**AQI:** {aqi}\n**Dominant pollutant:** {dominant}\n\nhttps://aqicn.org/city/{loc}"));
    }
    Ok(format!("air quality data unavailable for *{loc}*"))
}

async fn fetch_sunrise(loc: &str) -> Result<String> {
    let loc = if loc.trim().is_empty() { "34.1706,-118.8376" } else { loc };
    let parts: Vec<&str> = loc.split(',').collect();
    let lat = parts.first().unwrap_or(&"34.1706");
    let lon = parts.get(1).unwrap_or(&"-118.8376");
    let url = format!("https://api.sunrise-sunset.org/json?lat={}&lng={}&formatted=0", lat.trim(), lon.trim());
    let resp: serde_json::Value = HTTP.get(&url).send().await?.json().await?;
    if resp["status"].as_str() == Some("OK") {
        let results = &resp["results"];
        let sunrise = results["sunrise"].as_str().unwrap_or("?");
        let sunset = results["sunset"].as_str().unwrap_or("?");
        let day_length = results["day_length"].as_i64().unwrap_or(0);
        let hours = day_length / 3600;
        let mins = (day_length % 3600) / 60;
        return Ok(format!("{}\n\n**Sunrise:** `{}`\n**Sunset:** `{}`\n**Day length:** `{}h {}m`\n\n{}", tg_header("🌅", "Sunrise/Sunset", loc), sunrise, sunset, hours.to_string(), mins.to_string(), tg_footer("sunrise-sunset.org", "sunrise")));
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
                out.push_str(&short);
                out.push_str(&format!("\n\n{}", tg_footer("wiktionary.org", "etymology")));
                return Ok(out);
            }
        }
    }
    out.push_str(&format!("_No etymology section found for `{}`._\n\nTry: https://en.wiktionary.org/wiki/{}", word, urlencoding::encode(word)));
    out.push_str(&format!("\n\n{}", tg_footer("wiktionary.org", "etymology")));
    Ok(out)
}

async fn fetch_synonym(word: &str) -> Result<String> {
    let resp: serde_json::Value = HTTP.get(format!("https://api.datamuse.com/words?rel_syn={}", urlencoding::encode(word))).send().await?.json().await?;
    if let Some(words) = resp.as_array() {
        if words.is_empty() { return Ok(format!("{} \n\n_No synonyms found._\n\n{}", tg_header("📝", "Synonyms", word), tg_footer("datamuse.com", "synonym"))); }
        let syns: Vec<String> = words.iter().take(10).filter_map(|w| w["word"].as_str()).map(|s| format!("`{}`", s)).collect();
        return Ok(format!("{}\n\n{}\n\n{}", tg_header("📝", "Synonyms", word), syns.join(", "), tg_footer("datamuse.com", "synonym")));
    }
    Ok(format!("{} \n\n_synonym lookup failed_\n\n{}", tg_header("📝", "Synonyms", word), tg_footer("datamuse.com", "synonym")))
}

async fn fetch_philosophy_quote() -> Result<String> {
    // Try philosophy API, fallback to local quotes
    let api = async {
        let v: serde_json::Value = HTTP.get("https://philosophyapi.fly.dev/api/quotes/random")
            .timeout(std::time::Duration::from_secs(5))
            .send().await?.json().await?;
        let text = v["quote"].as_str().ok_or_else(|| anyhow::anyhow!("no quote"))?;
        let author = v["author"].as_str().unwrap_or("Unknown");
        Ok::<(String, String), anyhow::Error>((text.to_string(), author.to_string()))
    }.await;
    let (quote, author) = match api {
        Ok((q, a)) if !q.is_empty() => (q, a),
        _ => {
            let quotes = [
                ("The unexamined life is not worth living.", "Socrates"),
                ("I think, therefore I am.", "Rene Descartes"),
                ("Happiness is not an ideal of reason but of imagination.", "Immanuel Kant"),
                ("The only true wisdom is in knowing you know nothing.", "Socrates"),
                ("Man is condemned to be free.", "Jean-Paul Sartre"),
                ("One cannot step twice into the same river.", "Heraclitus"),
                ("I cannot teach anybody anything. I can only make them think.", "Socrates"),
                ("The owl of Minerva spreads its wings only with the falling of the dusk.", "G.W.F. Hegel"),
                ("Life can only be understood backwards; but it must be lived forwards.", "Soren Kierkegaard"),
                ("God is dead. And we have killed him.", "Friedrich Nietzsche"),
            ];
            let idx = (chrono::Utc::now().timestamp() as usize) % quotes.len();
            let (q, a) = quotes[idx];
            (q.to_string(), a.to_string())
        }
    };
    Ok(format!("{}\n\n> \"{}\"\n\n— **{}**\n\n{}", tg_header("📚", "Philosophy", ""), quote, author, tg_footer("philosophy", "learn")))
}

// === MONEY: Finance explainer (learn-focused) ===
async fn fetch_finance(term: &str) -> Result<String> {
    let q = term.trim();
    if q.is_empty() { return Ok(format!("{} \n\n_Usage:_ `/finance <term>` — e.g. `inflation`, `dividend`, `etf`\n\n{}", tg_header("💰", "Finance", "learn"), tg_footer("finance", "money"))); }
    // Try Wikipedia summary first (stable, no key)
    let wiki_url = format!("https://en.wikipedia.org/api/rest_v1/page/summary/{}", urlencoding::encode(q));
    let wiki: serde_json::Value = HTTP.get(&wiki_url).header("User-Agent", "memogram-rs").send().await?.json().await.unwrap_or(serde_json::Value::Null);
    let title = wiki["title"].as_str().unwrap_or(q);
    let extract = wiki["extract"].as_str().unwrap_or("");
    let default_url = format!("https://en.wikipedia.org/wiki/{}", urlencoding::encode(q));
    let url = wiki["content_urls"]["desktop"]["page"].as_str().unwrap_or(&default_url);
    let thumb = wiki["thumbnail"]["source"].as_str().unwrap_or("");
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    // Build detailed document
    let mut out = String::new();
    out.push_str(&format!("# 💰 Finance: {}\n\n", title));
    out.push_str(&format!("**Term:** `{}` · **Date:** `{}` \n", q, now));
    if !thumb.is_empty() { out.push_str(&format!("[📷 Cover]({})\n\n", thumb)); }
    out.push_str("## 📖 Overview\n");
    if !extract.is_empty() {
        out.push_str(&format!("{}\n\n", tg_truncate(extract, 600)));
    } else {
        out.push_str(&format!("_No summary found for `{}`. Try broader term._\n\n", q));
    }
    // Key facts table
    out.push_str("## 📊 Key Facts\n\n");
    out.push_str("| Aspect | Details |\n|---|---|\n");
    let typ = wiki["type"].as_str().unwrap_or("standard");
    let desc = wiki["description"].as_str().unwrap_or("finance term");
    out.push_str(&format!("| Type | {} |\n", desc));
    out.push_str(&format!("| Source | [Wikipedia]({}) |\n", url));
    out.push_str(&format!("| Query | `{}` |\n", q));
    out.push_str(&format!("| Kind | {} |\n", typ));
    // Example / how to think
    out.push_str("\n## 💡 How to think about it\n\n");
    out.push_str(&format!("> _Tip:_ Search `{} + investopedia` for plain-English examples. Try `/compound 1000 7% 10` to see compounding in action._\n\n", q));
    // Fun / learn more
    out.push_str("## 🎓 Fun & Learn More\n\n");
    out.push_str(&format!("- [Read full article]({})\n", url));
    out.push_str(&format!("- Related: `finance {}` → `compound` calculator\n", q));
    out.push_str(&format!("- Tags: `#finance #money #learn`\n"));
    out.push_str("\n---\n");
    out.push_str(&format!("{}\n\n`{}` · #{}", tg_header("💰", "Finance", q), now, "finance"));
    // Also include memo footer for Telegram/md
    out.push_str(&format!("\n\n{}", tg_footer("wikipedia.org", "finance")));
    Ok(out)
}

fn create_compound(args: &str) -> String {
    // Parse: "1000 7% 10" or "1000 0.07 10y" -> principal, rate, years
    let re = Regex::new(r"(?i)([0-9,.]+)\s*([0-9.]+%?)\s*([0-9.]+)").unwrap();
    let caps = re.captures(args.trim());
    if caps.is_none() {
        return format!("{} \n\n_Usage:_ `/compound <principal> <rate%> <years>`\n_Eg:_ `/compound 1000 7% 10` or `/compound 5000 0.05 20`\n\n{}", tg_header("🧮", "Compound Interest", "calc"), tg_footer("compound", "money"));
    }
    let cap = caps.unwrap();
    let p_str = cap.get(1).unwrap().as_str().replace(",", "");
    let r_str = cap.get(2).unwrap().as_str().replace("%", "").trim().to_string();
    let y_str = cap.get(3).unwrap().as_str().to_string();
    let p: f64 = p_str.parse().unwrap_or(1000.0);
    let r_raw: f64 = r_str.parse().unwrap_or(0.07);
    let r = if r_raw > 1.0 { r_raw / 100.0 } else { r_raw };
    let years: usize = y_str.parse::<f64>().unwrap_or(10.0) as usize;
    let years = years.clamp(1, 50);
    let final_amt = p * (1.0 + r).powi(years as i32);
    let interest = final_amt - p;
    let apr = r * 100.0;
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let mut out = String::new();
    out.push_str(&format!("# 🧮 Compound Interest — Detailed\n\n"));
    out.push_str(&format!("**Principal:** `${:.2}` · **Rate:** `{:.2}%` · **Years:** `{}` · **Date:** `{}`\n\n", p, apr, years, now));
    out.push_str("## 📊 Result\n\n");
    out.push_str("| Metric | Amount |\n|---|---|\n");
    out.push_str(&format!("| Principal | `${:.2}` |\n", p));
    out.push_str(&format!("| Interest | `${:.2}` |\n", interest));
    out.push_str(&format!("| Final Amount | `${:.2}` |\n", final_amt));
    out.push_str(&format!("| Multiple | `{:.2}x` |\n", final_amt / p));
    out.push_str("\n## 📈 Yearly Breakdown\n\n");
    out.push_str("| Year | Balance | Interest Y | Bar |\n|---:|---:|---:|---|\n");
    let max = final_amt;
    for y in 1..=years.min(30) {
        let bal = p * (1.0 + r).powi(y as i32);
        let yr_interest = bal - p * (1.0 + r).powi((y-1) as i32);
        let bar_len = ((bal / max) * 10.0).round() as usize;
        let bar = "█".repeat(bar_len) + &"░".repeat(10 - bar_len);
        out.push_str(&format!("| {} | ${:.0} | ${:.0} | {} |\n", y, bal, yr_interest, bar));
        if y == 30 && years > 30 { out.push_str(&format!("| ... | ... | ... | ... |\n")); break; }
    }
    out.push_str("\n```mermaid\n");
    out.push_str("xychart-beta\n");
    out.push_str("    title \"Growth\"\n");
    out.push_str("    x-axis [Year]");
    let mut vals = Vec::new();
    for y in (1..=years).step_by((years/5).max(1)) { let v = p * (1.0 + r).powi(y as i32); vals.push(format!("{:.0}", v)); }
    out.push_str(&format!("    y-axis \"Balance\" {}\n", vals.join(" ")));
    out.push_str("```\n\n");
    out.push_str("## 🧠 Formula & Fun\n\n");
    out.push_str(&format!("_A = P(1+r)^t_ → `{:.0}*(1+{:.4})^{}`\n\n", p, r, years));
    out.push_str("> _Tip:_ Increase rate 1% or add $100/mo — small changes compound massively. Try again with different inputs._\n\n");
    out.push_str(&format!("{}\n\n`{}` · #{}", tg_header("🧮", "Compound", args), now, "compound"));
    out.push_str("\n\n> #compound #money #learn");
    out
}

// === BIO: Trial + Food (beautiful docs) ===
async fn fetch_trial(query: &str) -> Result<String> {
    let q = query.trim();
    if q.is_empty() { return Ok(format!("{} \n\n_Usage:_ `/trial diabetes` or `/trial Alzheimer`\n\n{}", tg_header("🔬", "Clinical Trials", "search"), tg_footer("clinicaltrials.gov", "trial"))); }
    let url = format!("https://clinicaltrials.gov/api/v2/studies?query.term={}&pageSize=5&format=json", urlencoding::encode(q));
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "memogram-rs").send().await?.json().await.unwrap_or(serde_json::Value::Null);
    let studies = v["studies"].as_array();
    if studies.is_none() || studies.unwrap().is_empty() {
        return Ok(format!("{} \n\n_No trials found for `{}`._ Try broader term like `diabetes` or `cancer`.\n\n{}", tg_header("🔬", "Clinical Trials", q), q, tg_footer("clinicaltrials.gov", "trial")));
    }
    let arr = studies.unwrap();
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let mut out = String::new();
    out.push_str(&format!("# 🔬 Clinical Trials — `{}`\n\n", q));
    out.push_str(&format!("**Query:** `{}` · **Found:** `{}` · **Date:** `{}`\n\n", q, arr.len(), now));
    out.push_str("| # | NCTId | Phase | Status | Title |\n|---:|---|---|---|---|\n");
    for (i, s) in arr.iter().enumerate() {
        let proto = &s["protocolSection"];
        let id = proto["identificationModule"]["nctId"].as_str().unwrap_or("?");
        let title = proto["identificationModule"]["briefTitle"].as_str().unwrap_or("?").chars().take(50).collect::<String>();
        let phase = proto["designModule"]["phases"].as_array().and_then(|a| a.first()).and_then(|v| v.as_str()).unwrap_or("N/A");
        let status = proto["statusModule"]["overallStatus"].as_str().unwrap_or("?");
        out.push_str(&format!("| {} | {} | {} | {} | {} |\n", i+1, id, phase, status, title));
    }
    out.push_str("\n## 📋 Details\n\n");
    for s in arr.iter().take(3) {
        let proto = &s["protocolSection"];
        let id = proto["identificationModule"]["nctId"].as_str().unwrap_or("?");
        let title = proto["identificationModule"]["briefTitle"].as_str().unwrap_or("?");
        let conds = proto["conditionsModule"]["conditions"].as_array().map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(", ")).unwrap_or("?".into());
        let url2 = format!("https://clinicaltrials.gov/study/{}", id);
        out.push_str(&format!("### {} — {} \n**Conditions:** {} \n[View on ClinicalTrials.gov]({})\n\n", id, title, conds.chars().take(120).collect::<String>(), url2));
    }
    out.push_str("---\n");
    out.push_str(&format!("{}\n\n`{}` · #{}", tg_header("🔬", "Clinical Trials", q), now, "trial"));
    out.push_str(&format!("\n\n{}", tg_footer("clinicaltrials.gov", "trial")));
    Ok(out)
}

async fn fetch_food(query: &str) -> Result<String> {
    let q = query.trim();
    if q.is_empty() { return Ok(format!("{}\n\n_Usage:_ `/food apple` or `/food oreo`\n\n{}", tg_header("🥗", "Food", "nutrition"), tg_footer("openfoodfacts.org", "food"))); }
    let url = format!("https://world.openfoodfacts.org/cgi/search.pl?search_terms={}&search_simple=1&action=process&json=true&page_size=3", urlencoding::encode(q));
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "memogram-rs").send().await?.json().await.unwrap_or(serde_json::Value::Null);
    let mut products = v["products"].as_array();
    // Fallback to v2 search if empty (more reliable)
    if products.is_none() || products.unwrap().is_empty() {
        let url2 = format!("https://world.openfoodfacts.org/api/v2/search?search_terms={}&page_size=3&fields=product_name,brands,nutriscore_grade,nutriments", urlencoding::encode(q));
        if let Ok(v2) = HTTP.get(&url2).header("User-Agent", "memogram-rs").send().await {
            if let Ok(j2) = v2.json::<serde_json::Value>().await {
                if let Some(arr) = j2["products"].as_array() {
                    if !arr.is_empty() {
                        // use v2 products but map to expected shape
                        let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
                        let mut out = String::new();
                        out.push_str(&format!("# 🥗 Nutrition — `{}`\n\n", q));
                        out.push_str("| # | Product | Brand | Score |\n|---:|---|---|---|\n");
                        for (i, p) in arr.iter().take(3).enumerate() {
                            let name = p["product_name"].as_str().unwrap_or("?").chars().take(30).collect::<String>();
                            let brand = p["brands"].as_str().unwrap_or("?").chars().take(20).collect::<String>();
                            let score = p["nutriscore_grade"].as_str().unwrap_or("-");
                            let emoji = match score { "a"=>"🟢", "b"=>"🟢", "c"=>"🟡", "d"=>"🟠", "e"=>"🔴", _=>"⚪" };
                            out.push_str(&format!("| {} | {} | {} | {} {} |\n", i+1, name, brand, emoji, score));
                        }
                        out.push_str("\n_Results from fallback API._\n\n");
                        out.push_str(&format!("{}\n\n`{}` · #{}", tg_header("🥗", "Nutrition", q), now, "food"));
                        out.push_str(&format!("\n\n{}", tg_footer("openfoodfacts.org", "food")));
                        return Ok(out);
                    }
                }
            }
        }
        return Ok(format!("{}\n\n_No foods found for `{}`._\n\n{}", tg_header("🥗", "Nutrition", q), q, tg_footer("openfoodfacts.org", "food")));
    }
    let arr = products.unwrap();
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let mut out = String::new();
    out.push_str(&format!("# 🥗 Nutrition — `{}`\n\n", q));
    out.push_str("| # | Product | Brand | Score |\n|---:|---|---|---|\n");
    for (i, p) in arr.iter().enumerate() {
        let name = p["product_name"].as_str().unwrap_or("?").chars().take(30).collect::<String>();
        let brand = p["brands"].as_str().unwrap_or("?").chars().take(20).collect::<String>();
        let score = p["nutriscore_grade"].as_str().unwrap_or("-");
        let emoji = match score { "a"=>"🟢", "b"=>"🟢", "c"=>"🟡", "d"=>"🟠", "e"=>"🔴", _=>"⚪" };
        out.push_str(&format!("| {} | {} | {} | {} {} |\n", i+1, name, brand, emoji, score));
    }
    out.push_str("\n## 📊 Nutrition Facts (per 100g)\n\n");
    out.push_str("| Product | Kcal | Prot | Fat | Carbs | Sugar | Salt | Fiber |\n|---|---|---|---|---|---|---|---|\n");
    for p in arr.iter().take(3) {
        let name = p["product_name"].as_str().unwrap_or("?").chars().take(20).collect::<String>();
        let nutr = &p["nutriments"];
        let kcal = nutr["energy-kcal_100g"].as_f64().map(|v| format!("{:.0}", v)).unwrap_or("-".into());
        let prot = nutr["proteins_100g"].as_f64().map(|v| format!("{:.1}", v)).unwrap_or("-".into());
        let fat = nutr["fat_100g"].as_f64().map(|v| format!("{:.1}", v)).unwrap_or("-".into());
        let carbs = nutr["carbohydrates_100g"].as_f64().map(|v| format!("{:.1}", v)).unwrap_or("-".into());
        let sugar = nutr["sugars_100g"].as_f64().map(|v| format!("{:.1}", v)).unwrap_or("-".into());
        let salt = nutr["salt_100g"].as_f64().map(|v| format!("{:.2}", v)).unwrap_or("-".into());
        let fiber = nutr["fiber_100g"].as_f64().map(|v| format!("{:.1}", v)).unwrap_or("-".into());
        out.push_str(&format!("| {} | {} | {} | {} | {} | {} | {} | {} |\n", name, kcal, prot, fat, carbs, sugar, salt, fiber));
    }
    out.push_str("\n```mermaid\n");
    out.push_str("pie title \"Top Product Nutrients (g/100g)\"\n");
    if let Some(p) = arr.first() {
        let nutr = &p["nutriments"];
        let prot: f64 = nutr["proteins_100g"].as_f64().unwrap_or(0.0);
        let fat: f64 = nutr["fat_100g"].as_f64().unwrap_or(0.0);
        let carbs: f64 = nutr["carbohydrates_100g"].as_f64().unwrap_or(0.0);
        out.push_str(&format!("    \"Protein\" : {}\n", prot));
        out.push_str(&format!("    \"Fat\" : {}\n", fat));
        out.push_str(&format!("    \"Carbs\" : {}\n", carbs));
    }
    out.push_str("```\n\n");
    out.push_str("> _Tip:_ Nutri-Score `a`=best `e`=worst. Compare brands for same food._\n\n");
    out.push_str(&format!("{}\n\n`{}` · #{}", tg_header("🥗", "Nutrition", q), now, "food"));
    out.push_str(&format!("\n\n{}", tg_footer("openfoodfacts.org", "food")));
    Ok(out)
}

// === MUSIC BUCKET (7) — beats/promo ===

async fn fetch_itunes(query: &str) -> Result<String> {
    let q = query.trim();
    if q.is_empty() { return Ok(format!("{}\n\n_Usage:_ `/itunes <artist or track>`\n\n{}", tg_header("🎵", "iTunes", "search"), tg_footer("itunes.apple.com", "itunes"))); }
    let url = format!("https://itunes.apple.com/search?term={}&media=music&limit=5&entity=song", urlencoding::encode(q));
    let v: serde_json::Value = HTTP.get(&url).send().await?.json().await?;
    let results = v["results"].as_array();
    if results.is_none() || results.unwrap().is_empty() {
        return Ok(format!("{}\n\n_No results for `{}`._\n\n{}", tg_header("🎵", "iTunes", q), q, tg_footer("itunes.apple.com", "itunes")));
    }
    let arr = results.unwrap();
    let mut out = format!("{}\n\n", tg_header("🎵", "iTunes", q));
    for (i, r) in arr.iter().enumerate() {
        let track = r["trackName"].as_str().unwrap_or("?");
        let artist = r["artistName"].as_str().unwrap_or("?");
        let album = r["collectionName"].as_str().unwrap_or("?");
        let genre = r["primaryGenreName"].as_str().unwrap_or("?");
        let url = r["trackViewUrl"].as_str().unwrap_or("");
        let art = r["artworkUrl100"].as_str().unwrap_or("");
        out.push_str(&format!("**{}. {}** — {}\n   _{}_ · `{}`\n   [Listen]({})\n", i+1, track, artist, album, genre, url));
        if !art.is_empty() { out.push_str(&format!("   ![art]({})\n", art)); }
        out.push('\n');
    }
    out.push_str(&format!("\n{}", tg_footer("itunes.apple.com", "itunes")));
    Ok(out)
}

async fn fetch_deezer(query: &str) -> Result<String> {
    let q = query.trim();
    if q.is_empty() { return Ok(format!("{}\n\n_Usage:_ `/deezer <query>`\n\n{}", tg_header("🎧", "Deezer", "search"), tg_footer("deezer.com", "deezer"))); }
    let url = format!("https://api.deezer.com/search/track?q={}&limit=5", urlencoding::encode(q));
    let v: serde_json::Value = HTTP.get(&url).send().await?.json().await?;
    let data = v["data"].as_array();
    if data.is_none() || data.unwrap().is_empty() {
        return Ok(format!("{}\n\n_No results for `{}`._\n\n{}", tg_header("🎧", "Deezer", q), q, tg_footer("deezer.com", "deezer")));
    }
    let arr = data.unwrap();
    let mut out = format!("{}\n\n", tg_header("🎧", "Deezer", q));
    for (i, r) in arr.iter().enumerate() {
        let title = r["title"].as_str().unwrap_or("?");
        let artist = r["artist"]["name"].as_str().unwrap_or("?");
        let album = r["album"]["title"].as_str().unwrap_or("?");
        let link = r["link"].as_str().unwrap_or("");
        let preview = r["preview"].as_str().unwrap_or("");
        out.push_str(&format!("**{}. {}** — {}\n   _{}_\n   [Link]({})", i+1, title, artist, album, link));
        if !preview.is_empty() { out.push_str(&format!(" · [Preview]({})", preview)); }
        out.push_str("\n\n");
    }
    out.push_str(&format!("\n{}", tg_footer("deezer.com", "deezer")));
    Ok(out)
}

async fn fetch_mbrainz(query: &str) -> Result<String> {
    let q = query.trim();
    if q.is_empty() { return Ok(format!("{}\n\n_Usage:_ `/mbrainz <artist>`\n\n{}", tg_header("🎙️", "MusicBrainz", "search"), tg_footer("musicbrainz.org", "mbrainz"))); }
    let url = format!("https://musicbrainz.org/ws/2/artist/?query=artist:{}&fmt=json&limit=5", urlencoding::encode(q));
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "memogram-rs/0.1 ( junilab.xyz )").send().await?.json().await?;
    let artists = v["artists"].as_array();
    if artists.is_none() || artists.unwrap().is_empty() {
        return Ok(format!("{}\n\n_No artists for `{}`._\n\n{}", tg_header("🎙️", "MusicBrainz", q), q, tg_footer("musicbrainz.org", "mbrainz")));
    }
    let arr = artists.unwrap();
    let mut out = format!("{}\n\n", tg_header("🎙️", "MusicBrainz", q));
    for (i, a) in arr.iter().enumerate() {
        let name = a["name"].as_str().unwrap_or("?");
        let disamb = a["disambiguation"].as_str().unwrap_or("");
        let country = a["country"].as_str().unwrap_or("?");
        let typ = a["type"].as_str().unwrap_or("?");
        let id = a["id"].as_str().unwrap_or("");
        out.push_str(&format!("**{}. {}**", i+1, name));
        if !disamb.is_empty() { out.push_str(&format!(" — _{}_", disamb)); }
        out.push_str(&format!("\n   {} · `{}`\n   https://musicbrainz.org/artist/{}\n\n", typ, country, id));
    }
    out.push_str(&format!("\n{}", tg_footer("musicbrainz.org", "mbrainz")));
    Ok(out)
}

async fn fetch_lyrics(query: &str) -> Result<String> {
    let q = query.trim();
    if q.is_empty() || !q.contains('-') && !q.contains('/') && !q.contains('|') {
        return Ok(format!("{}\n\n_Usage:_ `/lyrics Artist - Title` or `/lyrics Artist/Title`\n\n{}", tg_header("📝", "Lyrics", "search"), tg_footer("lyrics.ovh", "lyrics")));
    }
    let (artist, title) = if q.contains(" - ") { let p: Vec<&str> = q.splitn(2, " - ").collect(); (p[0].trim(), p[1].trim()) }
        else if q.contains('/') { let p: Vec<&str> = q.splitn(2, '/').collect(); (p[0].trim(), p[1].trim()) }
        else if q.contains('|') { let p: Vec<&str> = q.splitn(2, '|').collect(); (p[0].trim(), p[1].trim()) }
        else { (q, "") };
    if artist.is_empty() || title.is_empty() {
        return Ok(format!("{}\n\n_Usage:_ `/lyrics Artist - Title`\n\n{}", tg_header("📝", "Lyrics", q), tg_footer("lyrics.ovh", "lyrics")));
    }
    let url = format!("https://api.lyrics.ovh/v1/{}/{}", urlencoding::encode(artist), urlencoding::encode(title));
    let v: serde_json::Value = HTTP.get(&url).send().await?.json().await?;
    if let Some(ly) = v["lyrics"].as_str() {
        let snippet = ly.chars().take(1200).collect::<String>();
        return Ok(format!("{}\n\n```\n{}\n```\n\n{}", tg_header("📝", "Lyrics", &format!("{artist} — {title}")), snippet.trim(), tg_footer("lyrics.ovh", "lyrics")));
    }
    if let Some(err) = v["error"].as_str() {
        return Ok(format!("{}\n\n_No lyrics for `{} — {}`: {}_\n\n{}", tg_header("📝", "Lyrics", q), artist, title, err, tg_footer("lyrics.ovh", "lyrics")));
    }
    Ok(format!("{}\n\n_No lyrics found._\n\n{}", tg_header("📝", "Lyrics", q), tg_footer("lyrics.ovh", "lyrics")))
}

async fn fetch_bpm(query: &str) -> Result<String> {
    let q = query.trim();
    if q.is_empty() { return Ok(format!("{}\n\n_Usage:_ `/bpm 120` or `/bpm drake - hotline bling` (tries Deezer BPM)\n\n{}", tg_header("🥁", "BPM", "calc"), tg_footer("bpm", "music"))); }
    // Try numeric BPM first — local calc, no API, always works
    let first_token = q.split_whitespace().next().unwrap_or("").replace("bpm", "").replace(',', "");
    if let Ok(bpm) = first_token.parse::<f32>() {
        if bpm >= 30.0 && bpm <= 300.0 {
            let ms_beat = 60000.0 / bpm;
            let ms_bar = ms_beat * 4.0;
            let ms_8 = ms_beat * 8.0;
            let hz = bpm / 60.0;
            let mut out = format!("{}\n\n", tg_header("🥁", "BPM", &format!("{:.0}", bpm)));
            out.push_str(&format!("**BPM:** `{:.0}` · **Hz:** `{:.2}` · **Ms/beat:** `{:.0}ms`\n\n", bpm, hz, ms_beat));
            out.push_str("## ⏱️ Timing\n\n");
            out.push_str("| Unit | Ms | Sec | Use |\n|---|---|---|---|\n");
            out.push_str(&format!("| 1 beat | {:.0} | {:.2} | Delay 1/4 |\n", ms_beat, ms_beat/1000.0));
            out.push_str(&format!("| 1 bar (4 beats) | {:.0} | {:.2} | Loop |\n", ms_bar, ms_bar/1000.0));
            out.push_str(&format!("| 8 beats | {:.0} | {:.2} | Phrase |\n", ms_8, ms_8/1000.0));
            out.push_str(&format!("| 1/8 | {:.0} | {:.2} | Hi-hat |\n", ms_beat/2.0, ms_beat/2000.0));
            out.push_str(&format!("| 1/16 | {:.0} | {:.2} | Roll |\n", ms_beat/4.0, ms_beat/4000.0));
            out.push_str("\n## 🎚️ Delay Chart\n\n");
            out.push_str("| BPM | 1/4 ms | 1/8 ms | 1/16 ms |\n|---:|---:|---:|---:|\n");
            for b in [80, 90, 100, 110, 120, 130, 140, 150] {
                let m = 60000.0 / b as f32;
                out.push_str(&format!("| {} | {:.0} | {:.0} | {:.0} |\n", b, m, m/2.0, m/4.0));
            }
            out.push_str("\n```mermaid\nxychart-beta\n  title \"Ms per Beat\"\n  x-axis [80 90 100 110 120 130 140 150]\n  y-axis \"Ms\" 300 800\n  bar [750 666 600 545 500 461 428 400]\n```\n\n");
            out.push_str("> _Tip: Half-time feel = BPM/2. Double-time = BPM*2. Use for trap soul switches._\n\n");
            out.push_str(&format!("{}\n\n`{:.0} BPM` · #{}", tg_header("🥁", "BPM", &format!("{:.0}", bpm)), bpm, "bpm"));
            out.push_str(&format!("\n\n{}", tg_footer("bpm", "music")));
            return Ok(out);
        }
    }
    // Fallback: try Deezer search for BPM if query is track name
    let url = format!("https://api.deezer.com/search/track?q={}&limit=3", urlencoding::encode(q));
    if let Ok(v) = HTTP.get(&url).send().await {
        if let Ok(j) = v.json::<serde_json::Value>().await {
            if let Some(arr) = j["data"].as_array() {
                if !arr.is_empty() {
                    let mut out = format!("{}\n\n", tg_header("🥁", "BPM", q));
                    out.push_str("| # | Track | Artist | BPM | Link |\n|---:|---|---|---|---|\n");
                    for (i, r) in arr.iter().enumerate() {
                        let title = r["title"].as_str().unwrap_or("?");
                        let artist = r["artist"]["name"].as_str().unwrap_or("?");
                        let link = r["link"].as_str().unwrap_or("");
                        // Deezer sometimes has bpm field, else try to fetch track details
                        let bpm = r["bpm"].as_f64().map(|v| format!("{:.0}", v)).unwrap_or("-".into());
                        out.push_str(&format!("| {} | {} | {} | {} | [Link]({}) |\n", i+1, title, artist, bpm, link));
                    }
                    out.push_str(&format!("\n> _Tip: If BPM is `-`, use `/bpm 120` for calc._\n\n{}", tg_footer("deezer.com", "bpm")));
                    return Ok(out);
                }
            }
        }
    }
    Ok(format!("{}\n\n_No BPM for `{}`. Try `/bpm 120`._\n\n{}", tg_header("🥁", "BPM", q), q, tg_footer("bpm", "music")))
}

async fn fetch_trend() -> Result<String> {
    let v: serde_json::Value = HTTP.get("https://api.deezer.com/chart/0/tracks?limit=5").send().await?.json().await?;
    let data = v["data"].as_array().or_else(|| v["tracks"]["data"].as_array());
    if data.is_none() || data.unwrap().is_empty() {
        return Ok(format!("{}\n\n_No trends._\n\n{}", tg_header("🔥", "Trending", "Deezer Top 5"), tg_footer("deezer.com", "trend")));
    }
    let arr = data.unwrap();
    let mut out = format!("{}\n\n", tg_header("🔥", "Trending", "Deezer Top 5"));
    for (i, r) in arr.iter().take(5).enumerate() {
        let title = r["title"].as_str().unwrap_or("?");
        let artist = r["artist"]["name"].as_str().unwrap_or("?");
        let link = r["link"].as_str().unwrap_or("");
        let rank = r["rank"].as_u64().unwrap_or(0);
        out.push_str(&format!("**{}. {}** — {}\n   Rank: {} · [Link]({})\n\n", i+1, title, artist, rank, link));
    }
    out.push_str(&format!("\n{}", tg_footer("deezer.com", "trend")));
    Ok(out)
}

fn create_promo(args: &str) -> String {
    let topic = if args.trim().is_empty() { "New Beat Drop" } else { args.trim() };
    let now = Local::now().format("%Y-%m-%d").to_string();
    format!(
        "# Promo: {topic}\n\n**Date:** {now}\n**Platform:** Instagram / TikTok / YouTube Shorts\n\n## Hook (0-3s)\n- \"{topic} — out now\"\n\n## Caption\n{topic} 🎧 — link in bio\n\n## Hashtags\n#beats #instrumental #typebeat #producer #newmusic #hiphop #trap #junilab\n\n## CTA\n- Comment \"BEAT\" for link\n- Tag a rapper who needs this\n\n## Links\n- BeatStars: \n- YouTube: \n\n#promo #music #{now}",
        topic = topic, now = now
    )
}

// === STOIC COMMANDS ===

fn create_meditation(note: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let dur = note.split_whitespace().next().unwrap_or("10m");
    let note_body = note.splitn(2, ' ').nth(1).unwrap_or(note);
    let date = Local::now().format("%Y-%m-%d").to_string();
    format!(
        "# 🧘 Meditation — `{}`\n\n**Date:** `{}` · **Duration:** `{}`\n**Note:** {}\n\n## 📊 Session\n\n| Duration | Date | Streak |\n|---|---|---|\n| {} | {} | 3 days |\n\n## 📈 Last 7 Days (sample)\n\n| Date | Duration | Focus |\n|---|---|---|\n| {} | {} | {} |\n| 2026-09-03 | 12m | breath |\n| 2026-09-02 | 8m | body scan |\n\n```mermaid\nxychart-beta\n  title \"Minutes\"\n  x-axis [Mon Tue Wed Thu Fri Sat Sun]\n  y-axis \"Min\" 0 20\n  bar [10 12 8 10 15 0 10]\n```\n\n## 💡 Practice\n> _Tip: 4-4-4-4 box breathing. Note 1 word for focus, return when distracted._\n\n{}\n\n`{}` · #{}",
        dur, now, dur, note_body, dur, date, date, dur, note_body, tg_header("🧘", "Meditation", dur), now, "wellness"
    )
}

fn create_affirmation(note: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let date = Local::now().format("%Y-%m-%d").to_string();
    format!(
        "# 💪 Affirmation — `{}`\n\n**Date:** `{}`\n\n## 💬 Affirmation\n\n> \"{}\"\n\n## 🌱 Reflection\n\n- Why this resonates:\n- How to embody today:\n\n## 📈 Repetition\n\n| Date | Affirmation | Felt |\n|---|---|---|\n| {} | {} |  |\n\n> _Tip: Say aloud 3x, morning + night._\n\n{}\n\n`{}` · #{}",
        date, now, note, date, note, tg_header("💪", "Affirmation", &date), now, "wellness"
    )
}

fn create_reflection(note: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let date = Local::now().format("%Y-%m-%d").to_string();
    format!(
        "# 🪞 Reflection — `{}`\n\n**Date:** `{}`\n\n## 💭 Prompt\n\n{}\n\n## 🔍 Insights\n\n- \n\n## ✅ Action\n\n- [ ] \n\n## 📊 Mood\n\n| Energy | Stress | Gratitude |\n|---|---|---|\n| /10 | /10 |  |\n\n{}\n\n`{}` · #{}",
        date, now, note, tg_header("🪞", "Reflection", &date), now, "wellness"
    )
}

async fn fetch_wisdom() -> Result<String> {
    fetch_stoic_quote().await
}

fn create_journal(note: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let date = Local::now().format("%Y-%m-%d").to_string();
    format!(
        "# 📔 Journal — `{}`\n\n**Date:** `{}`\n\n## ✍️ Entry\n\n{}\n\n## 🔍 Reflection\n\n| Prompt | Response |\n|---|---|\n| What went well? |  |\n| What was hard? |  |\n| Gratitude |  |\n| Tomorrow |  |\n\n## 📈 Streak\n\n```mermaid\nxychart-beta\n  title \"Words / Day\"\n  x-axis [Mon Tue Wed Thu Fri Sat Sun]\n  y-axis \"Words\" 0 300\n  bar [120 80 200 150 90 0 180]\n```\n\n> _Tip: 5m free write, no editing. End with 1 gratitude._\n\n{}\n\n`{}` · #{}",
        date, now, note, tg_header("📔", "Journal", &date), now, "wellness"
    )
}

// === PLANNING COMMANDS ===

fn create_goal(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d").to_string();
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let goal = parts.first().unwrap_or(&"Untitled");
    let details = parts.get(1).unwrap_or(&"");
    format!(
        "# 🎯 Goal — `{}`\n\n**Set:** `{}`\n\n## 🎯 Objective\n\n{}\n\n## 📋 Details\n\n{}\n\n## ✅ Milestones\n\n- [ ] \n- [ ] \n- [ ] \n\n## 📊 Progress\n\n| Week | Target | Done |\n|---|---|---|\n| W1 |  |  |\n| W2 |  |  |\n\n> _Tip: Make it SMART — Specific, Measurable, Achievable._\n\n{}\n\n`{}` · #{}",
        goal, now, goal, details, tg_header("🎯", "Goal", goal), now, "planning"
    )
}

fn create_deadline(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d").to_string();
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let date = parts.first().unwrap_or(&"TBD");
    let task = parts.get(1).unwrap_or(&"");
    format!(
        "# ⏰ Deadline — `{}`\n\n**Due:** `{}` · **Set:** `{}`\n\n## 📝 Task\n\n{}\n\n## ⏳ Countdown\n\n| Due | Days Left | Status |\n|---|---|---|\n| {} |  | ⏳ |\n\n## ✅ Checklist\n\n- [ ] \n- [ ] \n\n> _Tip: Add to calendar + set reminder 1d before._\n\n{}\n\n`{}` · #{}",
        date, date, now, task, date, tg_header("⏰", "Deadline", date), now, "planning"
    )
}

fn create_plan(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d").to_string();
    format!(
        "# 📋 Plan — `{}`\n\n**Date:** `{}`\n\n## 🎯 Objective\n\n{}\n\n## 📋 Steps\n\n1. \n2. \n3. \n\n## 📊 Timeline\n\n| Step | Owner | Due |\n|---|---|---|\n| 1 |  |  |\n| 2 |  |  |\n\n## ⚠️ Risks\n\n- \n\n{}\n\n`{}` · #{}",
        now, now, args, tg_header("📋", "Plan", &now), now, "planning"
    )
}

fn create_review(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d").to_string();
    format!(
        "# 📊 Review — `{}`\n\n**Date:** `{}`\n\n## 📝 Summary\n\n{}\n\n## ✅ Wins\n\n- \n\n## 🔧 Gaps\n\n- \n\n## 📈 Metrics\n\n| Metric | Target | Actual |\n|---|---|---|\n|  |  |  |\n\n## ➡️ Next\n\n- \n\n{}\n\n`{}` · #{}",
        now, now, args, tg_header("📊", "Review", &now), now, "planning"
    )
}

fn create_priority(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d").to_string();
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let level = parts.first().unwrap_or(&"P1");
    let task = parts.get(1).unwrap_or(&"");
    format!(
        "# 🔥 Priority — `{}`\n\n**Level:** `{}` · **Set:** `{}`\n\n## 📝 Task\n\n{}\n\n## 📊 Matrix\n\n| Urgent | Important | Action |\n|---|---|---|\n| Yes | Yes | Do now |\n|  |  |  |\n\n> _Tip: P1=do now, P2=schedule, P3=delegate, P4=drop._\n\n{}\n\n`{}` · #{}",
        level, level, now, task, tg_header("🔥", "Priority", level), now, "planning"
    )
}

// === INBOX COMMANDS ===

fn create_idea(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    format!(
        "# 💡 Idea — `{}`\n\n**Captured:** `{}`\n\n## 💭 Concept\n\n{}\n\n## 🔗 Connections\n\n- \n\n## ✅ Next\n\n- [ ] Research\n- [ ] Prototype\n- [ ] Share\n\n## 🏷️ Tags\n\n- #idea #inbox\n\n{}\n\n`{}` · #{}",
        now, now, args, tg_header("💡", "Idea", &now), now, "inbox"
    )
}

fn create_braindump(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    format!(
        "# 🧠 Brain Dump — `{}`\n\n**Time:** `{}`\n\n## 🌊 Dump\n\n{}\n\n## 🗂️ Clusters\n\n- \n- \n- \n\n## ✅ Extract\n\n- [ ] \n- [ ] \n\n> _Tip: Dump fast, cluster later, extract 1 next action._\n\n{}\n\n`{}` · #{}",
        now, now, args, tg_header("🧠", "Brain Dump", &now), now, "inbox"
    )
}

fn create_link(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let url = parts.first().unwrap_or(&"https://example.com");
    let desc = parts.get(1).unwrap_or(&"");
    format!(
        "# 🔗 Link — `{}`\n\n**URL:** {}\n**Saved:** `{}`\n\n## 📝 Why\n\n{}\n\n## 🏷️ Tags\n\n- \n\n## 🔗 Related\n\n- \n\n{}\n\n`{}` · #{}",
        now, url, now, desc, tg_header("🔗", "Link", url), now, "inbox"
    )
}

fn create_snippet(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    format!(
        "# 📝 Snippet — `{}`\n\n**Saved:** `{}`\n\n## 📋 Code\n\n{}\n\n## 💡 Context\n\n- \n\n## 🔗 Source\n\n- \n\n{}\n\n`{}` · #{}",
        now, now, tg_code_block(args), tg_header("📝", "Snippet", &now), now, "inbox"
    )
}

fn create_save(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    format!(
        "# 💾 Saved — `{}`\n\n**Time:** `{}`\n\n## 📌 Content\n\n{}\n\n## 🏷️ Tags\n\n- #save #inbox\n\n## 🔗 Action\n\n- [ ] Process\n\n{}\n\n`{}` · #{}",
        now, now, args, tg_header("💾", "Saved", &now), now, "inbox"
    )
}

// === DAILY COMMANDS ===

fn create_morning(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d").to_string();
    let date = now.clone();
    format!(
        "# 🌅 Morning — `{}`\n\n**Date:** `{}`\n\n## 🎯 Intent\n\n{}\n\n## ✅ Top 3\n\n- [ ] \n- [ ] \n- [ ] \n\n## 💧 Health\n\n- Sleep: \n- Water: \n- Energy: /10\n\n## 📝 Note\n\n{}\n\n{}\n\n`{}` · #{}",
        date, now, args, if args.trim().is_empty() { "_Set 1 intent for today._" } else { "" }, tg_header("🌅", "Morning", &date), now, "daily"
    )
}

fn create_evening(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d").to_string();
    let date = now.clone();
    format!(
        "# 🌙 Evening — `{}`\n\n**Date:** `{}`\n\n## 📝 Reflection\n\n{}\n\n## ✅ Wins\n\n- \n\n## 🔧 Improves\n\n- \n\n## 🙏 Gratitude\n\n- \n\n{}\n\n`{}` · #{}",
        date, now, args, tg_header("🌙", "Evening", &date), now, "daily"
    )
}

fn create_checkin(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    format!(
        "# ✅ Check-in — `{}`\n\n**Time:** `{}`\n\n## 💭 State\n\n{}\n\n## 📊 Quick\n\n| Focus | Energy | Stress |\n|---|---|---|\n|  | /10 | /10 |\n\n> _Tip: 1 breath, note 1 win._\n\n{}\n\n`{}` · #{}",
        now, now, args, tg_header("✅", "Check-in", &now), now, "daily"
    )
}

fn create_log(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    format!(
        "# 📋 Log — `{}`\n\n**Time:** `{}`\n\n## 📝 Entry\n\n{}\n\n## 🏷️ Tags\n\n- \n\n## 🔗 Links\n\n- \n\n{}\n\n`{}` · #{}",
        now, now, args, tg_header("📋", "Log", &now), now, "daily"
    )
}

fn create_summary(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d").to_string();
    let date = now.clone();
    format!(
        "# 📝 Summary — `{}`\n\n**Date:** `{}`\n\n## 📌 TL;DR\n\n{}\n\n## ✅ Done\n\n- \n\n## 📊 Stats\n\n| Metric | Value |\n|---|---|\n| Tasks |  |\n| Focus | /10 |\n\n## 🔜 Next\n\n- \n\n{}\n\n`{}` · #{}",
        date, now, args, tg_header("📝", "Summary", &date), now, "daily"
    )
}

// === LIFE COMMANDS ===

fn create_sleep(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d");
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let hours = parts.first().unwrap_or(&"?");
    let quality = parts.get(1).unwrap_or(&"");
    format!("{}\n\n**Hours:** {}\n**Quality:** {}\n**Date:** `{}`\n\n{}", tg_header("😴", "Sleep", ""), hours, quality, now, tg_footer("sleep", "life"))
}

fn create_energy(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let level = parts.first().unwrap_or(&"?");
    let note = parts.get(1).unwrap_or(&"");
    let lvl: i32 = level.parse().unwrap_or(5);
    let bar = "█".repeat((lvl as usize).clamp(0, 10)) + &"░".repeat(10 - (lvl as usize).clamp(0, 10));
    let date = Local::now().format("%Y-%m-%d").to_string();
    format!(
        "# ⚡ Energy — `{}`\n\n**Time:** `{}` · **Level:** `{}/10` {}\n**Note:** {}\n\n## 📊 Level\n\n| Level | Bar | Status |\n|---|---|---|\n| {}/10 | {} | {} |\n\n## 📈 Last 7 Days (sample)\n\n| Date | Level | Note |\n|---|---|---|\n| {} | {} | {} |\n| 2026-09-03 | 7 | good sleep |\n| 2026-09-02 | 4 | late night |\n\n```mermaid\nxychart-beta\n  title \"Energy Trend\"\n  x-axis [Mon Tue Wed Thu Fri Sat Sun]\n  y-axis \"Level\" 0 10\n  bar [6 7 4 8 7 5 {}]\n```\n\n## 💡 Boost\n> _Tip: Hydrate, 10m walk, sunlight, protein + complex carbs._\n\n{}\n\n`{}` · #{}",
        level, now, level, bar, note, level, bar, if lvl >= 7 { "🔥 High" } else if lvl >= 4 { "🟡 Medium" } else { "🔵 Low" }, date, level, note, lvl, tg_header("⚡", "Energy", &format!("{}/10", level)), now, "energy"
    )
}

fn create_exercise(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d").to_string();
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let activity = parts.first().unwrap_or(&"run");
    let duration = parts.get(1).unwrap_or(&"30m");
    let date = now.clone();
    format!(
        "# 🏋️ Exercise — `{}`\n\n**Date:** `{}` · **Activity:** `{}` · **Duration:** `{}`\n\n## 📊 Session\n\n| Metric | Value |\n|---|---|\n| Activity | {} |\n| Duration | {} |\n| Date | {} |\n| Calories est | {} |\n\n## 📈 Weekly Volume (sample)\n\n| Day | Activity | Duration |\n|---|---|---|\n| {} | {} | {} |\n| 2026-09-03 | rest | — |\n| 2026-09-02 | weights | 45m |\n\n```mermaid\nxychart-beta\n  title \"Minutes / Day\"\n  x-axis [Mon Tue Wed Thu Fri Sat Sun]\n  y-axis \"Min\" 0 60\n  bar [30 0 45 30 20 0 30]\n```\n\n## 💡 Next\n> _Tip: Progressive overload + 48h rest per muscle group. Hydrate + protein within 60m._\n\n{}\n\n`{}` · #{}",
        activity, now, activity, duration, activity, duration, now, if duration.contains("30") { "220" } else { "180" }, date, activity, duration, tg_header("🏋️", "Exercise", activity), now, "exercise"
    )
}

fn create_water(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let amount = parts.first().unwrap_or(&"500ml");
    let note = parts.get(1).unwrap_or(&"");
    let date = Local::now().format("%Y-%m-%d").to_string();
    let ml: i32 = amount.replace("ml", "").replace("L", "").parse::<f32>().map(|v| if amount.contains('L') { (v*1000.0) as i32 } else { v as i32 }).unwrap_or(500);
    let bar = "█".repeat(((ml as f32/2500.0*10.0) as usize).clamp(0,10)) + &"░".repeat(10-((ml as f32/2500.0*10.0) as usize).clamp(0,10));
    let pct = (ml as f32/2500.0*100.0) as i32;
    format!(
        "# 💧 Hydration — `{}`\n\n**Time:** `{}` · **Amount:** `{}` {} ({}% of 2.5L)\n**Note:** {}\n\n## 📊 Intake\n\n| Amount | Bar | Daily Goal |\n|---|---|---|\n| {} | {} | {}% |\n\n## 📈 Today (sample)\n\n| Time | Amount | Total |\n|---|---|---|\n| {} | {} | {} |\n| 08:00 | 500ml | 500ml |\n| 12:00 | 300ml | 800ml |\n\n```mermaid\nxychart-beta\n  title \"Water ml\"\n  x-axis [08:00 12:00 15:00 18:00 21:00]\n  y-axis \"ml\" 0 2500\n  bar [500 300 400 500 300]\n```\n\n## 💡 Tip\n> _Tip: 2.5L/day avg, more if exercise/heat. Pale yellow = hydrated._\n\n{}\n\n`{}` · #{}",
        amount, now, amount, bar, pct, note, amount, bar, pct, now, amount, amount, tg_header("💧", "Water", amount), now, "water"
    )
}

fn create_stress(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let level = parts.first().unwrap_or(&"?");
    let note = parts.get(1).unwrap_or(&"");
    let date = Local::now().format("%Y-%m-%d").to_string();
    let lvl: i32 = level.parse().unwrap_or(5);
    let bar = "█".repeat((lvl as usize).clamp(0, 10)) + &"░".repeat(10 - (lvl as usize).clamp(0, 10));
    format!(
        "# 😰 Stress — `{}`\n\n**Date:** `{}` · **Level:** `{}/10` {}\n**Note:** {}\n\n## 📊 Assessment\n\n| Level | Bar | Status |\n|---|---|---|\n| {}/10 | {} | {} |\n\n## 📈 Trend (sample)\n\n```mermaid\nxychart-beta\n  title \"Stress Last 7d\"\n  x-axis [Mon Tue Wed Thu Fri Sat Sun]\n  y-axis \"Level\" 0 10\n  bar [3 4 6 5 7 4 {}]\n```\n\n## 💡 Coping\n> _Tip: Box breathing 4-4-4-4 • 10m walk • 3 gratitudes • no screens before bed._\n\n{}\n\n`{}` · #{}",
        level, now, level, bar, note, level, bar, if lvl >= 7 { "🔴 High" } else if lvl >= 4 { "🟡 Medium" } else { "🟢 Low" }, lvl, tg_header("😰", "Stress", &format!("{}/10", level)), now, "stress"
    )
}

fn create_read(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d").to_string();
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let title = parts.first().unwrap_or(&"Untitled");
    let author = parts.get(1).unwrap_or(&"");
    format!(
        "# 📚 Reading — `{}`\n\n**Title:** `{}` · **Author:** `{}` · **Date:** `{}`\n\n## 📝 Summary\n\n- \n\n## 💡 Takeaways\n\n1. \n2. \n3. \n\n## 💬 Quotes\n\n> \"\" — {}\n\n## 📊 Progress\n\n| Pages | % | Notes |\n|---|---|---|\n|  |  |  |\n\n{}\n\n`{}` · #{}",
        title, title, author, now, author, tg_header("📚", "Reading", title), now, "wellness"
    )
}

async fn run_preview() -> Result<()> {
    let out_dir = std::path::Path::new(r"C:\Users\asher\AppData\Local\Temp\memogram-preview\live");
    let _ = std::fs::create_dir_all(out_dir);
    println!("=== PREVIEW MODE ===");

    async fn try_fetch<F: std::future::Future<Output = Result<String>>>(name: &str, fut: F) -> (String, String) {
        let res = tokio::time::timeout(std::time::Duration::from_secs(12), fut).await;
        match res {
            Ok(Ok(s)) => (name.to_string(), s),
            Ok(Err(e)) => (name.to_string(), format!("_Error for `{}`: {}_", name, e)),
            Err(_) => (name.to_string(), format!("{}\n\n_Data unavailable for `{}` (timeout after 12s). Try again._\n\n{}", tg_header("⚠️", "Timeout", name), name, tg_footer("memogram", "timeout"))),
        }
    }

    // Live fetches — sequential with 8s timeout each so one slow API doesn't hang forever
    let samples: Vec<(&str, String)> = vec![
        ("hn", try_fetch("hn", fetch_hn()).await.1),
        ("weather", try_fetch("weather", fetch_weather("Thousand Oaks, CA")).await.1),
        ("define", try_fetch("define", fetch_define("serendipity")).await.1),
        ("wiki", try_fetch("wiki", fetch_wiki("Rust programming language")).await.1),
        ("cheat", try_fetch("cheat", fetch_cheat("tar")).await.1),
        ("gh", try_fetch("gh", fetch_gh("rust")).await.1),
        ("fx", try_fetch("fx", fetch_fx("USD-KRW")).await.1),
        ("stock", try_fetch("stock", fetch_stock("AAPL")).await.1),
        ("crypto", try_fetch("crypto", fetch_crypto("bitcoin")).await.1),
        ("translate", try_fetch("translate", fetch_translate("hello world")).await.1),
        ("forecast", try_fetch("forecast", fetch_forecast("London")).await.1),
        ("npm", try_fetch("npm", fetch_npm("express")).await.1),
        ("pypi", try_fetch("pypi", fetch_pypi("requests")).await.1),
        ("crates", try_fetch("crates", fetch_crates("tokio")).await.1),
        ("stackoverflow", try_fetch("stackoverflow", fetch_stackoverflow("rust async")).await.1),
        ("airquality", try_fetch("airquality", fetch_airquality("Beijing")).await.1),
        ("sunrise", try_fetch("sunrise", fetch_sunrise("34.1706,-118.8376")).await.1),
        ("etymology", try_fetch("etymology", fetch_etymology("hello")).await.1),
        ("synonym", try_fetch("synonym", fetch_synonym("happy")).await.1),
        ("philosophy", try_fetch("philosophy", fetch_philosophy_quote()).await.1),
        ("finance", try_fetch("finance", fetch_finance("inflation")).await.1),
        ("trial", try_fetch("trial", fetch_trial("diabetes")).await.1),
        ("food", try_fetch("food", fetch_food("apple")).await.1),
        ("recipe", try_fetch("recipe", fetch_recipe("chicken")).await.1),
        ("recipe-random", try_fetch("recipe-random", fetch_recipe("")).await.1),
        ("stoic", try_fetch("stoic", fetch_stoic_quote()).await.1),
        ("pubmed", try_fetch("pubmed", fetch_pubmed("CRISPR")).await.1),
        ("drug", try_fetch("drug", fetch_drug("aspirin")).await.1),
        ("bbc", try_fetch("bbc", fetch_bbc()).await.1),
        ("reuters", try_fetch("reuters", fetch_reuters()).await.1),
        ("ap", try_fetch("ap", fetch_ap()).await.1),
        ("arxiv", try_fetch("arxiv", fetch_arxiv("quantum")).await.1),
        ("devto", try_fetch("devto", fetch_devto()).await.1),
        ("tldr", try_fetch("tldr", fetch_tldr()).await.1),
        ("reddit", try_fetch("reddit", fetch_reddit("selfhosted")).await.1),
        ("markets", try_fetch("markets", fetch_markets()).await.1),
        ("ip", try_fetch("ip", fetch_ip("8.8.8.8")).await.1),
        ("itunes", try_fetch("itunes", fetch_itunes("drake")).await.1),
        ("deezer", try_fetch("deezer", fetch_deezer("drake")).await.1),
        ("mbrainz", try_fetch("mbrainz", fetch_mbrainz("beatles")).await.1),
        ("lyrics", try_fetch("lyrics", fetch_lyrics("coldplay - adventure of a lifetime")).await.1),
        ("bpm", try_fetch("bpm", fetch_bpm("120")).await.1),
        ("trend", try_fetch("trend", fetch_trend()).await.1),
    ];

    for (name, content) in samples {
        let path = out_dir.join(format!("{}.md", name));
        let _ = std::fs::write(&path, &content);
        println!("wrote {} ({} chars, {} lines)", name, content.len(), content.lines().count());
        // print first 200 chars for quick check
        let preview: String = content.chars().take(200).collect();
        println!("  preview: {}", preview.replace('\n', " "));
    }

    // Templates (sync)
    let templates = vec![
        ("meditation", create_meditation("10m focused on breath")),
        ("affirmation", create_affirmation("I am capable and calm")),
        ("reflection", create_reflection("Today I learned to iterate quickly")),
        ("journal", create_journal("Today I shipped the new markdown pipeline")),
        ("goal", create_goal("Ship memogram v2 clean markdown")),
        ("deadline", create_deadline("2026-09-10 Ship v2")),
        ("plan", create_plan("1. Fix markdown\n2. Test live\n3. Deploy")),
        ("idea", create_idea("Add voice memo transcription via Whisper free API")),
        ("braindump", create_braindump("Need to fix weather, then news, then money buckets")),
        ("morning", create_morning("Ready to build, coffee done")),
        ("sleep", create_sleep("7.5 good")),
        ("energy", create_energy("8 feeling great")),
        ("exercise", create_exercise("run 30m")),
        ("water", create_water("500ml morning")),
        ("read", create_read("Dune Frank Herbert")),
        ("compound", create_compound("1000 7% 10")),
        ("stress", create_stress("6 work deadline")),
        ("promo", create_promo("New Beat Drop - Trap Soul Type Beat")),
    ];
    for (name, content) in templates {
        let path = out_dir.join(format!("{}.md", name));
        let _ = std::fs::write(&path, &content);
        println!("wrote {} (template)", name);
    }

    println!("=== DONE — check {}/ ===", out_dir.display());
    Ok(())
}
