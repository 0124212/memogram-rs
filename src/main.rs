use anyhow::Result;
use base64::Engine;
use chrono::{Local, Datelike};
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use rand::prelude::*;
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
    Daily,
    Streak,
    Hn,
    Weather(String),
    Define(String),
    Wiki(String),
    Gh(String),
    Fx(String),
    Containers,
    Stock(String),
    Crypto(String),
    Translate(String),
    Remind(String),
    Portfolio(String),
    Alerts(String),
    Markets,
    Arxiv(String),
    Inbox,
    Undo,
    Pin,
    Book(String),
    Goal(String),
    Deadline(String),
    Summarize(String),
    Save(String),
    Pubmed(String),
    Ip(String),
    Finance(String),
    Compound(String),
    Trial(String),
    Food(String),
    Paper(String),
    Hustle(String),
    Digest,
    Youtube(String),
    Learn(String),
    Workout(String),
    Health(String),
    Nutrition(String),
    Meal(String),
    Breathe(String),
    Calories(String),
    Dns(String),
    Json(String),
    Regex(String),
    Uuid,
    Wind(String),
    Uv(String),
    Moon,
    Pollen(String),
    Snow(String),
    Tide(String),
    Project(String),
    Todo(String),
    Lobsters,
    Ph,
    Weekly,
    Species(String),
    Prereqs(String),
    Mcat(String),
    Scholar(String),
    Reddit(String),
    News(String),
}

#[derive(Clone)]
struct App {
    memos_url: String,
    admin_username: String,
    allowed: Option<Vec<String>>,
    store: Arc<RwLock<HashMap<i64, String>>>,
    store_path: String,
    bot_tokens: HashMap<String, String>,
    bark_url: String,
    ntfy_url: String,
    vikunja_url: String,
    vikunja_token: String,
    api_ninjas_key: String,
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
    let bark_url = env::var("BARK_URL").ok().unwrap_or_default();
    let ntfy_url = env::var("NTFY_URL").ok().unwrap_or_default();
    let vikunja_url = env::var("VIKUNJA_URL").ok().unwrap_or_default();
    let vikunja_token = env::var("VIKUNJA_TOKEN").ok().unwrap_or_default();
    let api_ninjas_key = env::var("API_NINJAS_KEY").ok().unwrap_or_default();
    let store = Arc::new(RwLock::new(load_store(&store_path).await));
    let app = App { memos_url, admin_username, allowed, store: store.clone(), store_path, bot_tokens, bark_url, ntfy_url, vikunja_url, vikunja_token, api_ninjas_key };

    info!("memogram-rs starting url={} store={} bots={:?}", app.memos_url, app.store_path, app.bot_tokens.keys().collect::<Vec<_>>());

    let _ = bot.set_my_commands(vec![
        teloxide::types::BotCommand { command: "start".into(), description: "link Telegram → Memos".into() },
        teloxide::types::BotCommand { command: "search".into(), description: "search memos".into() },
        teloxide::types::BotCommand { command: "hn".into(), description: "HackerNews top 5".into() },
        teloxide::types::BotCommand { command: "arxiv".into(), description: "arXiv latest papers".into() },
        teloxide::types::BotCommand { command: "weather".into(), description: "weather <city>".into() },
        teloxide::types::BotCommand { command: "define".into(), description: "define <word>".into() },
        teloxide::types::BotCommand { command: "wiki".into(), description: "wiki <query>".into() },
        teloxide::types::BotCommand { command: "gh".into(), description: "GitHub search/repo".into() },
        teloxide::types::BotCommand { command: "fx".into(), description: "fx <pair>".into() },
        teloxide::types::BotCommand { command: "stock".into(), description: "stock <ticker>".into() },
        teloxide::types::BotCommand { command: "crypto".into(), description: "crypto <coin>".into() },
        teloxide::types::BotCommand { command: "portfolio".into(), description: "track holdings".into() },
        teloxide::types::BotCommand { command: "alerts".into(), description: "price alerts".into() },
        teloxide::types::BotCommand { command: "markets".into(), description: "market indices".into() },
        teloxide::types::BotCommand { command: "translate".into(), description: "translate text".into() },
        teloxide::types::BotCommand { command: "containers".into(), description: "service health".into() },
        teloxide::types::BotCommand { command: "json".into(), description: "pretty-print JSON".into() },
        teloxide::types::BotCommand { command: "regex".into(), description: "regex tester".into() },
        teloxide::types::BotCommand { command: "uuid".into(), description: "generate UUID".into() },
        teloxide::types::BotCommand { command: "dns".into(), description: "DNS lookup".into() },
        teloxide::types::BotCommand { command: "daily".into(), description: "create daily note".into() },
        teloxide::types::BotCommand { command: "streak".into(), description: "writing streak".into() },
        teloxide::types::BotCommand { command: "digest".into(), description: "today's memo summary".into() },
        teloxide::types::BotCommand { command: "inbox".into(), description: "untagged memos".into() },
        teloxide::types::BotCommand { command: "undo".into(), description: "delete last memo".into() },
        teloxide::types::BotCommand { command: "pin".into(), description: "pin/unpin last memo".into() },
        teloxide::types::BotCommand { command: "book".into(), description: "book from Open Library".into() },
        teloxide::types::BotCommand { command: "goal".into(), description: "set a goal (Vikunja)".into() },
        teloxide::types::BotCommand { command: "deadline".into(), description: "deadline (Vikunja)".into() },
        teloxide::types::BotCommand { command: "weekly".into(), description: "weekly review (Vikunja)".into() },
        teloxide::types::BotCommand { command: "save".into(), description: "save anything".into() },
        teloxide::types::BotCommand { command: "remind".into(), description: "remind <min> <msg>".into() },
        teloxide::types::BotCommand { command: "help".into(), description: "help".into() },
        teloxide::types::BotCommand { command: "finance".into(), description: "finance term explainer".into() },
        teloxide::types::BotCommand { command: "compound".into(), description: "compound interest calc".into() },
        teloxide::types::BotCommand { command: "hustle".into(), description: "side hustle ideas".into() },
        teloxide::types::BotCommand { command: "food".into(), description: "nutrition lookup".into() },
        teloxide::types::BotCommand { command: "workout".into(), description: "workout plan <muscle>".into() },
        teloxide::types::BotCommand { command: "health".into(), description: "health dashboard <w> <h> <age>".into() },
        teloxide::types::BotCommand { command: "nutrition".into(), description: "full nutrient breakdown".into() },
        teloxide::types::BotCommand { command: "meal".into(), description: "recipe card <cuisine>".into() },
        teloxide::types::BotCommand { command: "breathe".into(), description: "breathing exercise".into() },
        teloxide::types::BotCommand { command: "calories".into(), description: "calories burned <activity> <min>".into() },
        teloxide::types::BotCommand { command: "pubmed".into(), description: "PubMed papers".into() },
        teloxide::types::BotCommand { command: "trial".into(), description: "clinical trial search".into() },
        teloxide::types::BotCommand { command: "paper".into(), description: "paper deep-dive".into() },
        teloxide::types::BotCommand { command: "youtube".into(), description: "summarize video".into() },
        teloxide::types::BotCommand { command: "learn".into(), description: "learning overview".into() },
        teloxide::types::BotCommand { command: "wind".into(), description: "wind forecast".into() },
        teloxide::types::BotCommand { command: "uv".into(), description: "UV index".into() },
        teloxide::types::BotCommand { command: "moon".into(), description: "moon phase".into() },
        teloxide::types::BotCommand { command: "pollen".into(), description: "pollen forecast".into() },
        teloxide::types::BotCommand { command: "snow".into(), description: "snow report".into() },
        teloxide::types::BotCommand { command: "tide".into(), description: "tide schedule".into() },
        teloxide::types::BotCommand { command: "project".into(), description: "Vikunja project".into() },
        teloxide::types::BotCommand { command: "todo".into(), description: "Vikunja task".into() },
        teloxide::types::BotCommand { command: "lobsters".into(), description: "lobste.rs top stories".into() },
        teloxide::types::BotCommand { command: "ph".into(), description: "Product Hunt today".into() },
        teloxide::types::BotCommand { command: "species".into(), description: "taxonomy lookup".into() },
        teloxide::types::BotCommand { command: "prereqs".into(), description: "health prof prereqs".into() },
        teloxide::types::BotCommand { command: "mcat".into(), description: "MCAT study guide".into() },
        teloxide::types::BotCommand { command: "scholar".into(), description: "Google Scholar".into() },
        teloxide::types::BotCommand { command: "reddit".into(), description: "subreddit top posts".into() },
        teloxide::types::BotCommand { command: "news".into(), description: "news on any topic".into() },
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
        Command::Gh(q) => { let txt = fetch_gh(&q).await.unwrap_or_else(|e| format!("gh err: {e}")); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
        Command::Fx(pair) => { let txt = fetch_fx(&pair).await.unwrap_or_else(|e| format!("fx err: {e}")); create_as_bot(&bot, &msg, &app, "money", &txt, tid).await?; }
        Command::Containers => { let txt = fetch_containers(&app.memos_url).await.unwrap_or_else(|e| format!("containers err: {e}")); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
        Command::Arxiv(topic) => { let txt = fetch_arxiv(&topic).await.unwrap_or_else(|e| format!("arxiv err: {e}")); create_as_bot(&bot, &msg, &app, "news", &txt, tid).await?; }
        Command::Stock(ticker) => { let txt = fetch_stock(&ticker).await.unwrap_or_else(|e| format!("stock err: {e}")); create_as_bot(&bot, &msg, &app, "money", &txt, tid).await?; }
        Command::Crypto(coin) => { let txt = fetch_crypto(&coin).await.unwrap_or_else(|e| format!("crypto err: {e}")); create_as_bot(&bot, &msg, &app, "money", &txt, tid).await?; }
        Command::Translate(args) => { let txt = fetch_translate(&args).await.unwrap_or_else(|e| format!("translate err: {e}")); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Daily => {
            let token = { app.store.read().await.get(&tid).cloned() };
            let Some(tok) = token else { bot.send_message(msg.chat.id, "run /start <token> first").await?; return Ok(()); };
            let txt = fetch_daily(&app.memos_url, &tok).await.unwrap_or_else(|e| format!("daily err: {e}"));
            create_as_bot(&bot, &msg, &app, "planning", &txt, tid).await?;
        }
        Command::Streak => {
            let token = { app.store.read().await.get(&tid).cloned() };
            let Some(tok) = token else { bot.send_message(msg.chat.id, "run /start <token> first").await?; return Ok(()); };
            let txt = fetch_streak(&app.memos_url, &tok).await.unwrap_or_else(|e| format!("streak err: {e}"));
            create_as_bot(&bot, &msg, &app, "daily", &txt, tid).await?;
        }
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
        Command::Book(args) => { let txt = fetch_book(&args).await.unwrap_or_else(|e| format!("book err: {e}")); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Pubmed(q) => { let txt = fetch_pubmed(&q).await.unwrap_or_else(|e| format!("pubmed err: {e}")); create_as_bot(&bot, &msg, &app, "bio", &txt, tid).await?; }
        Command::Ip(ip) => { let txt = fetch_ip(&ip).await.unwrap_or_else(|e| format!("ip err: {e}")); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
        Command::Finance(term) => { let txt = fetch_finance(&term).await.unwrap_or_else(|e| format!("finance err: {e}")); create_as_bot(&bot, &msg, &app, "money", &txt, tid).await?; }
        Command::Compound(args) => { let txt = create_compound(&args); create_as_bot(&bot, &msg, &app, "money", &txt, tid).await?; }
        Command::Trial(q) => { let txt = fetch_trial(&q).await.unwrap_or_else(|e| format!("trial err: {e}")); create_as_bot(&bot, &msg, &app, "bio", &txt, tid).await?; }
        Command::Food(q) => { let txt = fetch_food(&q).await.unwrap_or_else(|e| format!("food err: {e}")); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Workout(q) => { let txt = fetch_workout(&q).await.unwrap_or_else(|e| format!("workout err: {e}")); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Health(args) => { let txt = fetch_health(&args).await.unwrap_or_else(|e| format!("health err: {e}")); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Nutrition(q) => { let txt = fetch_nutrition(&q, &app.api_ninjas_key).await.unwrap_or_else(|e| format!("nutrition err: {e}")); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Meal(q) => { let txt = fetch_meal(&q).await.unwrap_or_else(|e| format!("meal err: {e}")); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Breathe(args) => { let txt = create_breathe(&args); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Calories(args) => { let txt = fetch_calories(&args, &app.api_ninjas_key).await.unwrap_or_else(|e| format!("calories err: {e}")); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Goal(args) => { let txt = vikunja_goal(&args, &app).await; create_as_bot(&bot, &msg, &app, "planning", &txt, tid).await?; }
        Command::Deadline(args) => { let txt = vikunja_deadline(&args, &app).await; create_as_bot(&bot, &msg, &app, "planning", &txt, tid).await?; }
        Command::Summarize(url) => { let txt = fetch_summarize(&url).await.unwrap_or_else(|e| format!("summarize err: {e}")); create_as_bot(&bot, &msg, &app, "inbox", &txt, tid).await?; }
        Command::Save(args) => { let txt = fetch_save(&args).await.unwrap_or_else(|e| format!("save err: {e}")); create_as_bot(&bot, &msg, &app, "inbox", &txt, tid).await?; }
        
        Command::Paper(q) => { let txt = fetch_paper(&q).await.unwrap_or_else(|e| format!("paper err: {e}")); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Hustle(q) => { let txt = fetch_hustle(&q).await.unwrap_or_else(|e| format!("hustle err: {e}")); create_as_bot(&bot, &msg, &app, "money", &txt, tid).await?; }
        Command::Digest => {
            let token = { app.store.read().await.get(&tid).cloned() };
            let Some(tok) = token else { bot.send_message(msg.chat.id, "run /start <token> first").await?; return Ok(()); };
            let txt = fetch_digest(&app.memos_url, &tok).await.unwrap_or_else(|e| format!("digest err: {e}"));
            create_as_bot(&bot, &msg, &app, "daily", &txt, tid).await?;
        }
        Command::Youtube(url) => { let txt = fetch_youtube(&url).await.unwrap_or_else(|e| format!("youtube err: {e}")); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Learn(topic) => { let txt = fetch_learn(&topic).await.unwrap_or_else(|e| format!("learn err: {e}")); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Dns(domain) => { let txt = fetch_dns(&domain).await.unwrap_or_else(|e| format!("dns err: {e}")); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
        Command::Json(text) => { let txt = create_json(&text); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
        Command::Regex(args) => { let txt = create_regex(&args); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
        Command::Uuid => { let txt = create_uuid(); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
        Command::Wind(loc) => { let txt = fetch_wind(&loc).await.unwrap_or_else(|e| format!("wind err: {e}")); create_as_bot(&bot, &msg, &app, "weather", &txt, tid).await?; }
        Command::Uv(loc) => { let txt = fetch_uv(&loc).await.unwrap_or_else(|e| format!("uv err: {e}")); create_as_bot(&bot, &msg, &app, "weather", &txt, tid).await?; }
        Command::Moon => { let txt = fetch_moon("").await.unwrap_or_else(|e| format!("moon err: {e}")); create_as_bot(&bot, &msg, &app, "weather", &txt, tid).await?; }
        Command::Pollen(loc) => { let txt = fetch_pollen(&loc).await.unwrap_or_else(|e| format!("pollen err: {e}")); create_as_bot(&bot, &msg, &app, "weather", &txt, tid).await?; }
        Command::Snow(loc) => { let txt = fetch_snow(&loc).await.unwrap_or_else(|e| format!("snow err: {e}")); create_as_bot(&bot, &msg, &app, "weather", &txt, tid).await?; }
        Command::Tide(loc) => { let txt = fetch_tide(&loc).await.unwrap_or_else(|e| format!("tide err: {e}")); create_as_bot(&bot, &msg, &app, "weather", &txt, tid).await?; }
        Command::Project(args) => { let txt = vikunja_project(&args, &app).await; create_as_bot(&bot, &msg, &app, "planning", &txt, tid).await?; }
        Command::Todo(args) => { let txt = vikunja_todo(&args, &app).await; create_as_bot(&bot, &msg, &app, "planning", &txt, tid).await?; }
        Command::Lobsters => { let txt = fetch_lobsters().await.unwrap_or_else(|e| format!("lobsters err: {e}")); create_as_bot(&bot, &msg, &app, "news", &txt, tid).await?; }
        Command::Ph => { let txt = fetch_ph().await.unwrap_or_else(|e| format!("ph err: {e}")); create_as_bot(&bot, &msg, &app, "news", &txt, tid).await?; }
        Command::Weekly => { let txt = vikunja_weekly(&app).await; create_as_bot(&bot, &msg, &app, "planning", &txt, tid).await?; }
        Command::Species(q) => { let txt = fetch_species(&q).await.unwrap_or_else(|e| format!("species err: {e}")); create_as_bot(&bot, &msg, &app, "bio", &txt, tid).await?; }
        Command::Prereqs(track) => { let txt = create_prereqs(&track); create_as_bot(&bot, &msg, &app, "bio", &txt, tid).await?; }
        Command::Mcat(topic) => { let txt = fetch_mcat(&topic).await.unwrap_or_else(|e| format!("mcat err: {e}")); create_as_bot(&bot, &msg, &app, "bio", &txt, tid).await?; }
        Command::Scholar(q) => { let txt = fetch_scholar(&q).await.unwrap_or_else(|e| format!("scholar err: {e}")); create_as_bot(&bot, &msg, &app, "news", &txt, tid).await?; }
        Command::Reddit(sub) => { let txt = fetch_reddit(&sub).await.unwrap_or_else(|e| format!("reddit err: {e}")); create_as_bot(&bot, &msg, &app, "news", &txt, tid).await?; }
        Command::News(topic) => { let txt = fetch_news(&topic).await.unwrap_or_else(|e| format!("news err: {e}")); create_as_bot(&bot, &msg, &app, "news", &txt, tid).await?; }
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
    let tag = format!("#{bot_name} #memogram-rs");
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

// === VIKUNJA API ===

async fn vikunja_request(url: &str, token: &str, method: &str, body: Option<serde_json::Value>) -> Result<serde_json::Value> {
    let mut req = match method {
        "POST" => HTTP.post(url),
        "PUT" => HTTP.put(url),
        "GET" => HTTP.get(url),
        "DELETE" => HTTP.delete(url),
        _ => HTTP.get(url),
    };
    let mut req = req
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json");
    if let Some(body) = body {
        req = req.json(&body);
    }
    let resp = req.timeout(std::time::Duration::from_secs(8)).send().await?;
    let v: serde_json::Value = resp.json().await?;
    Ok(v)
}

async fn vikunja_create_task(vikunja_url: &str, token: &str, title: &str, description: &str, project_id: u64, priority: u8, due_date: &str) -> Result<serde_json::Value> {
    let mut task = serde_json::json!({
        "title": title,
        "project_id": project_id,
    });
    if !description.is_empty() { task["description"] = serde_json::json!(description); }
    if priority > 0 { task["priority"] = serde_json::json!(priority); }
    if !due_date.is_empty() { task["due_date"] = serde_json::json!(format!("{}T00:00:00Z", due_date)); }
    let url = format!("{}/api/v1/tasks", vikunja_url.trim_end_matches('/'));
    vikunja_request(&url, token, "POST", Some(task)).await
}

async fn vikunja_create_project(vikunja_url: &str, token: &str, title: &str) -> Result<serde_json::Value> {
    let body = serde_json::json!({ "title": title });
    let url = format!("{}/api/v1/projects", vikunja_url.trim_end_matches('/'));
    vikunja_request(&url, token, "POST", Some(body)).await
}

async fn vikunja_list_projects(vikunja_url: &str, token: &str) -> Result<Vec<serde_json::Value>> {
    let url = format!("{}/api/v1/projects", vikunja_url.trim_end_matches('/'));
    let v = vikunja_request(&url, token, "GET", None).await?;
    Ok(v.as_array().cloned().unwrap_or_default())
}

async fn vikunja_list_tasks(vikunja_url: &str, token: &str, project_id: u64, done: Option<bool>) -> Result<Vec<serde_json::Value>> {
    let mut url = format!("{}/api/v1/projects/{}/tasks?sort_by=due_date&sort_order=asc", vikunja_url.trim_end_matches('/'), project_id);
    if let Some(d) = done {
        url.push_str(&format!("&filter.done={}", d));
    }
    let v = vikunja_request(&url, token, "GET", None).await?;
    Ok(v.as_array().cloned().unwrap_or_default())
}

async fn vikunja_complete_task(vikunja_url: &str, token: &str, task_id: u64) -> Result<serde_json::Value> {
    let body = serde_json::json!({ "done": true });
    let url = format!("{}/api/v1/tasks/{}", vikunja_url.trim_end_matches('/'), task_id);
    vikunja_request(&url, token, "PUT", Some(body)).await
}

fn vikunja_priority_label(p: u8) -> (&'static str, &'static str) {
    match p {
        1 => ("🔵", "P1-Low"),
        2 => ("🟡", "P2-Medium"),
        3 => ("🟠", "P3-High"),
        4 | 5 => ("🔴", "P4-Urgent"),
        _ => ("⚪", "None"),
    }
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

// --- Markdown Builder (stolen from tgcli + markdown-builder patterns) ---

struct Md {
    lines: Vec<String>,
}

impl Md {
    fn new() -> Self { Self { lines: Vec::new() } }

    fn h1(mut self, text: &str) -> Self { self.lines.push(format!("# {}", text)); self }
    fn h2(mut self, text: &str) -> Self { self.lines.push(format!("## {}", text)); self }
    fn h3(mut self, text: &str) -> Self { self.lines.push(format!("### {}", text)); self }
    fn h4(mut self, text: &str) -> Self { self.lines.push(format!("#### {}", text)); self }

    fn p(mut self, text: &str) -> Self { self.lines.push(text.to_string()); self }
    fn pi(mut self, key: &str, val: &str) -> Self {
        if !val.is_empty() { self.lines.push(format!("**{}:** {}", key, val)); }
        self
    }
    fn pi_num<T: std::fmt::Display>(mut self, key: &str, val: T) -> Self {
        self.lines.push(format!("**{}:** {}", key, val));
        self
    }

    fn bullet(mut self, text: &str) -> Self { self.lines.push(format!("- {}", text)); self }
    fn bullet_bold(mut self, key: &str, val: &str) -> Self { self.lines.push(format!("- **{}**: {}", key, val)); self }
    fn numbered(mut self, n: usize, text: &str) -> Self { self.lines.push(format!("{}. {}", n, text)); self }

    fn blank(mut self) -> Self { self.lines.push(String::new()); self }
    fn hr(mut self) -> Self { self.lines.push(String::new()); self.lines.push("---".into()); self.lines.push(String::new()); self }

    fn code_block(mut self, lang: &str, code: &str) -> Self {
        self.lines.push(format!("```{}", lang));
        self.lines.push(code.to_string());
        self.lines.push("```".into());
        self
    }
    fn quote(mut self, text: &str) -> Self {
        for line in text.lines() { self.lines.push(format!("> {}", line)); }
        self
    }
    fn table(mut self, headers: &[&str], rows: &[Vec<String>]) -> Self {
        if headers.is_empty() { return self; }
        let num_cols = headers.len();
        let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
        for row in rows {
            for (i, cell) in row.iter().enumerate() {
                if i < num_cols { widths[i] = widths[i].max(cell.len()); }
            }
        }
        let sep: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
        let header_row: Vec<String> = headers.iter().enumerate().map(|(i, h)| format!("{:pad$}", h, pad = widths[i])).collect();
        self.lines.push(format!("| {} |", header_row.join(" | ")));
        self.lines.push(format!("| {} |", sep.join(" | ")));
        for row in rows {
            let cells: Vec<String> = row.iter().enumerate().map(|(i, c)| {
                let pad = if i < num_cols { widths[i] } else { 0 };
                format!("{:pad$}", c, pad = pad)
            }).collect();
            self.lines.push(format!("| {} |", cells.join(" | ")));
        }
        self
    }
    fn table_md(mut self, headers: &[&str], rows: &[Vec<String>]) -> Self {
        if headers.is_empty() { return self; }
        self.lines.push(format!("| {} |", headers.join(" | ")));
        self.lines.push(format!("|{}|", headers.iter().map(|_| "---".to_string()).collect::<Vec<_>>().join("|")));
        for row in rows {
            self.lines.push(format!("| {} |", row.join(" | ")));
        }
        self
    }

    fn push(mut self, text: &str) -> Self { self.lines.push(text.to_string()); self }
    fn push_fmt(mut self, s: String) -> Self { self.lines.push(s); self }

    fn build(self) -> String { self.lines.join("\n") }
    fn build_trunc(self, max: usize) -> String {
        let s = self.lines.join("\n");
        if s.len() > max { format!("{}...", &s[..max]) } else { s }
    }
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

async fn wiki_summary(t: &str) -> (String, String, String) {
    let v: serde_json::Value = match HTTP.get(format!("https://en.wikipedia.org/api/rest_v1/page/summary/{}", urlencoding::encode(t))).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await {
        Ok(r) => match r.json().await { Ok(j) => j, Err(_) => serde_json::Value::Null },
        Err(_) => serde_json::Value::Null,
    };
    let title = v["title"].as_str().unwrap_or(t).to_string();
    let extract = v["extract"].as_str().unwrap_or("No summary available.").to_string();
    let url = v["content_urls"]["desktop"]["page"].as_str().map(|s| s.to_string()).unwrap_or_else(|| format!("https://en.wikipedia.org/wiki/{}", urlencoding::encode(t)));
    (title, extract, url)
}

async fn fetch_brief(q: &str) -> Result<String> {
    let topic = q.trim();
    if topic.is_empty() { return Ok(format!("{}\n\n_Usage:_ `/brief <topic>` — e.g. `/brief rust async`\n\n{}", tg_header("📝", "Brief", "guide"), tg_footer("wikipedia.org", "brief"))); }
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let (title, extract, url) = wiki_summary(topic).await;
    let mut out = format!("{}\n\n", tg_header("📝", "Brief", &title));
    out.push_str(&format!("**{}**\n\n{}\n\n", title, tg_truncate(&extract, 900)));
    out.push_str("| Fact | Detail |\n|---|---|\n");
    out.push_str(&format!("| 📚 Source | `wikipedia.org` |\n| 🔗 Article | [{}]({}) |\n| 📅 Briefed | `{}` |\n\n", title, url, now));
    out.push_str("## 🎯 Key points\n\n");
    for (i, p) in extract.split(". ").take(3).enumerate() {
        let s = p.trim();
        if s.is_empty() { continue; }
        if s.ends_with('.') { out.push_str(&format!("{}. {}\n", i + 1, s)); } else { out.push_str(&format!("{}. {}.\n", i + 1, s)); }
    }
    out.push_str(&format!("\n## 🔗 Go deeper\n\n> [Wikipedia]({}) — full article\n> [arXiv](https://arxiv.org/search/?query={}) — papers\n> [HN](https://hn.algolia.com/?q={}) — discussions\n\n", url, urlencoding::encode(topic), urlencoding::encode(topic)));
    out.push_str(&format!("{}\n\n`{}` · #brief #learn", tg_footer("wikipedia.org", "brief"), now));
    Ok(out)
}

async fn fetch_compare(q: &str) -> Result<String> {
    let parts: Vec<&str> = if q.contains(" vs ") { q.split(" vs ").collect() }
        else if q.contains('|') { q.split('|').collect() }
        else if q.contains(" v ") { q.split(" v ").collect() }
        else { q.split(',').collect() };
    if parts.len() < 2 || parts[0].trim().is_empty() || parts[1].trim().is_empty() {
        return Ok(format!("{}\n\n_Usage:_ `/compare <A> vs <B>` — e.g. `/compare vim vs emacs`\n\n{}", tg_header("⚖️", "Compare", "guide"), tg_footer("wikipedia.org", "compare")));
    }
    let (a, b) = (parts[0].trim(), parts[1].trim());
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let (ta, ea, ua) = wiki_summary(a).await;
    let (tb, eb, ub) = wiki_summary(b).await;
    let mut out = format!("{}\n\n", tg_header("⚖️", "Compare", &format!("{a} vs {b}")));
    out.push_str(&format!("**[{}]({})** vs **[{}]({})**\n\n", ta, ua, tb, ub));
    out.push_str("| Aspect | A: ");
    out.push_str(&ta);
    out.push_str(" | B: ");
    out.push_str(&tb);
    out.push_str(" |\n|---|---|---|\n");
    out.push_str(&format!("| 📝 Overview | {} | {} |\n", tg_truncate(&ea.chars().take(220).collect::<String>(), 220), tg_truncate(&eb.chars().take(220).collect::<String>(), 220)));
    out.push_str(&format!("| 🔗 Source | [Wikipedia]({}) | [Wikipedia]({}) |\n\n", ua, ub));
    out.push_str("## 🧭 Takeaway\n\n");
    out.push_str(&format!("> Pick **{}** for _{}_ — pick **{}** for _{}_.\n> Read both articles, then `/brief` the winner.\n\n", ta, tg_truncate(&ea.chars().take(80).collect::<String>(), 80), tb, tg_truncate(&eb.chars().take(80).collect::<String>(), 80)));
    out.push_str(&format!("{}\n\n`{}` · #compare #learn", tg_footer("wikipedia.org", "compare"), now));
    Ok(out)
}

async fn fetch_paper(q: &str) -> Result<String> {
    let topic = q.trim();
    if topic.is_empty() { return Ok(format!("{}\n\n_Usage:_ `/paper <query>` — e.g. `/paper diffusion transformers`\n\n{}", tg_header("📄", "Paper", "guide"), tg_footer("arxiv.org", "paper"), )); }
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let url = format!("http://export.arxiv.org/api/query?search_query=all:{}&sortBy=relevance&sortOrder=descending&max_results=1", urlencoding::encode(topic));
    let txt = HTTP.get(&url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(10)).send().await?.text().await?;
    let mut entry = String::new();
    let mut in_entry = false;
    for line in txt.lines() {
        if line.contains("<entry>") { in_entry = true; entry.clear(); }
        if in_entry { entry.push_str(line); entry.push('\n'); }
        if line.contains("</entry>") { break; }
    }
    if entry.is_empty() { return Ok(format!("{}\n\n_No papers found for `{}`._\n\n{}", tg_header("📄", "Paper", topic), topic, tg_footer("arxiv.org", "paper"))); }
    let title = extract_xml(&entry, "title").replace('\n', " ").trim().to_string();
    let id_url = extract_xml(&entry, "id");
    let arxiv_id = id_url.rsplit('/').next().unwrap_or("?").to_string();
    let summary = extract_xml(&entry, "summary").replace('\n', " ").trim().to_string();
    let published = extract_xml(&entry, "published").chars().take(10).collect::<String>();
    let updated = extract_xml(&entry, "updated").chars().take(10).collect::<String>();
    let author_names: Vec<String> = entry.split("<author>").skip(1).map(|s| extract_xml(s, "name")).filter(|n| !n.is_empty()).collect();
    let author_line = if author_names.is_empty() { "Unknown".to_string() } else if author_names.len() == 1 { author_names[0].clone() } else { format!("{} et al. ({} authors)", author_names[0], author_names.len()) };
    let pdf_url = id_url.replace("/abs/", "/pdf/");
    let mut out = format!("{}\n\n", tg_header("📄", "Paper", &arxiv_id));
    out.push_str(&format!("**{}**\n\n{}\n\n", title, tg_truncate(&summary, 1200)));
    out.push_str("| Detail | Value |\n|---|---|\n");
    out.push_str(&format!("| 👥 Authors | `{}` |\n| 📅 Published | `{}` |\n| 🔄 Updated | `{}` |\n| 🆔 arXiv | `{}` |\n\n", author_line, published, updated, arxiv_id));
    out.push_str(&format!("## 🔗 Links\n\n> [Abstract]({}) — landing page\n> [PDF]({}) — full text\n> [arXiv search](https://arxiv.org/search/?query={}) — related\n\n", id_url, pdf_url, urlencoding::encode(topic)));
    out.push_str(&format!("`cite: arxiv:{}`\n\n{}\n\n`{}` · #paper #research", arxiv_id, tg_footer("arxiv.org", "paper"), now));
    Ok(out)
}

async fn fetch_tutorial(q: &str) -> Result<String> {
    let topic = q.trim();
    if topic.is_empty() { return Ok(format!("{}\n\n_Usage:_ `/tutorial <topic>` — e.g. `/tutorial git rebase`\n\n{}", tg_header("📖", "Tutorial", "guide"), tg_footer("wikipedia.org", "tutorial"))); }
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let (title, extract, url) = wiki_summary(topic).await;
    let cheat_path = topic.trim().replace(' ', "/");
    let cheat = match HTTP.get(format!("https://cheat.sh/{}?T", urlencoding::encode(&cheat_path))).header("User-Agent", "curl/8.0").timeout(std::time::Duration::from_secs(8)).send().await {
        Ok(r) if r.status().is_success() => r.text().await.unwrap_or_default(),
        _ => String::new(),
    };
    let mut out = format!("{}\n\n", tg_header("📖", "Tutorial", &title));
    out.push_str(&format!("## 📖 Background\n\n{}\n\n[Read more]({})\n\n", tg_truncate(&extract, 600), url));
    if cheat.trim().is_empty() {
        out.push_str("## ⚡ Quick reference\n\n_Cheat sheet unavailable — see Wikipedia above._\n\n");
    } else {
        out.push_str(&format!("## ⚡ Quick reference\n\n{}\n\n", tg_code_block(&tg_truncate(cheat.trim(), 800))));
    }
    out.push_str("## ✅ Practice checklist\n\n");
    out.push_str(&format!("- [ ] Read the background on **{}** above\n- [ ] Run each quick-reference command locally\n- [ ] Write one memo with what broke and the fix\n\n", title));
    out.push_str(&format!("## 🔗 Resources\n\n| Resource | Link |\n|---|---|\n| 📚 Wikipedia | [{0}]({1}) |\n| 💻 cheat.sh | [cheat.sh/{2}](https://cheat.sh/{2}) |\n| 📝 tldr | [tldr.in](https://tldr.in/{2}) |\n\n", title, url, urlencoding::encode(topic)));
    out.push_str(&format!("{}\n\n`{}` · #tutorial #learn", tg_footer("wikipedia.org", "tutorial"), now));
    Ok(out)
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
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let mut out = format!("{}\n\n", tg_header("💱", "Exchange Rate", &format!("{base}/{quote}")));
    out.push_str(&format!("**Pair:** `{} → {}`\n\n", base, quote));
    out.push_str("## 💰 Conversion\n\n");
    out.push_str("| Amount | Result |\n|---|---|\n");
    out.push_str(&format!("| 1 {base} | **{:.4} {quote}** |\n", rate));
    out.push_str(&format!("| 10 {} | {:.4} {} |\n", base, rate * 10.0, quote));
    out.push_str(&format!("| 100 {} | {:.4} {} |\n", base, rate * 100.0, quote));
    out.push_str(&format!("| 1,000 {} | {:.2} {} |\n\n", base, rate * 1000.0, quote));
    out.push_str(&format!("🔗 [More rates](https://open.er-api.com/v6/latest/{})\n\n", base));
    out.push_str(&format!("{}\n\n`{}` · #fx", tg_footer("open.er-api.com", "fx"), now));
    Ok(out)
}

async fn fetch_containers(memos_url: &str) -> Result<String> {
    let services = vec![
        ("Memos", format!("{memos_url}/api/v1/status")),
        ("Vikunja", "http://vikunja:3456/health".to_string()),
        ("Radicale", "http://radicale:5232".to_string()),
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
    // Try to get description from detailed endpoint
    let desc_url = format!("https://api.coingecko.com/api/v3/coins/{}?localization=false&tickers=false&market_data=false&community_data=false&developer_data=false", urlencoding::encode(&coin_id));
    if let Ok(dr) = HTTP.get(&desc_url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(5)).send().await {
        if let Ok(dj) = dr.json::<serde_json::Value>().await {
            if let Some(desc) = dj["description"]["en"].as_str() {
                let clean_desc = desc.replace("<br>", " ").replace("<br/>", " ");
                let re = Regex::new(r"<[^>]+>").unwrap();
                let clean_desc = re.replace_all(&clean_desc, "").to_string();
                let short_desc = clean_desc.chars().take(200).collect::<String>();
                if !short_desc.trim().is_empty() {
                    body_str.push_str(&format!("\n\n{}", short_desc.trim()));
                }
            }
            if let Some(homepage) = dj["links"]["homepage"].as_array().and_then(|a| a.first()).and_then(|s| s.as_str()) {
                if !homepage.is_empty() {
                    body_str.push_str(&format!("\n\n🔗 [Homepage]({})", homepage));
                }
            }
            if let Some(repo) = dj["links"]["repos_url"]["github"].as_array().and_then(|a| a.first()).and_then(|s| s.as_str()) {
                body_str.push_str(&format!(" · [GitHub]({})", repo));
            }
        }
    }
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
    let langpair = langpair.replace("auto|", "en|");
    let url = format!("https://api.mymemory.translated.net/get?q={}&langpair={}", urlencoding::encode(&text), urlencoding::encode(&langpair));
    let v: serde_json::Value = HTTP.get(&url).send().await?.json().await?;
    let translated = v["responseData"]["translatedText"].as_str().unwrap_or("(no result)");
    if translated.contains("IS AN INVALID") || translated.contains("INVALID SOURCE") {
        let src = langpair.split('|').next().unwrap_or("en");
        let tgt = langpair.split('|').last().unwrap_or("en");
        let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
        let mut out = format!("{}\n\n", tg_header("🌐", "Translation", &langpair));
        out.push_str(&format!("**Source:** `{}` → **Target:** `{}`\n\n", src, tgt));
        out.push_str(&format!("**Original:**\n> {}\n\n", text));
        out.push_str(&format!("_Translation unavailable. Try:_\n\n"));
        out.push_str(&format!("`/translate en → es {}`\n\n", text));
        out.push_str(&format!("{}\n\n`{}` · #translate", tg_footer("mymemory.translated.net", "translate"), now));
        return Ok(out);
    }
    let detected = v["responseData"]["match"].as_f64().unwrap_or(0.0);
    let src = langpair.split('|').next().unwrap_or("en");
    let tgt = langpair.split('|').last().unwrap_or("en");
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let mut out = format!("{}\n\n", tg_header("🌐", "Translation", &format!("{} → {}", src, tgt)));
    out.push_str(&format!("**Source:** `{}` → **Target:** `{}` · **Confidence:** `{:.0}%`\n\n", src, tgt, detected));
    out.push_str(&format!("## 📝 Original\n\n> {}\n\n", text));
    out.push_str(&format!("## ✅ Translation\n\n> {}\n\n", translated));
    // Try to get back-translation for verification
    let back_url = format!("https://api.mymemory.translated.net/get?q={}&langpair={}|{}", urlencoding::encode(translated), tgt, src);
    if let Ok(bv) = HTTP.get(&back_url).send().await {
        if let Ok(bj) = bv.json::<serde_json::Value>().await {
            if let Some(back) = bj["responseData"]["translatedText"].as_str() {
                if !back.is_empty() && back.to_lowercase() != translated.to_lowercase() {
                    out.push_str(&format!("## 🔄 Back-Translation\n\n> {}\n\n", back));
                }
            }
        }
    }
    // Language tips
    let tips = match tgt {
        "es" => "Tip: Spanish uses gendered nouns — `el` (masc) / `la` (fem)",
        "fr" => "Tip: French uses `le` (masc) / `la` (fem) and liaison for flow",
        "ja" => "Tip: Japanese uses particles: `は` (topic), `が` (subject), `を` (object)",
        "ko" => "Tip: Korean uses SOV word order — verb goes last",
        "de" => "Tip: German capitalizes all nouns and has 4 cases",
        "zh" => "Tip: Mandarin is tonal — same syllable, different tone = different meaning",
        "ar" => "Tip: Arabic reads right-to-left and has 28 letters",
        "hi" => "Tip: Hindi uses Devanagari script and has 3 genders",
        _ => "",
    };
    if !tips.is_empty() {
        out.push_str(&format!("💡 {}\n\n", tips));
    }
    out.push_str(&format!("{}\n\n`{}` · #translate", tg_footer("mymemory.translated.net", "translate"), now));
    Ok(out)
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

async fn fetch_streak(memos_url: &str, token: &str) -> Result<String> {
    let v: serde_json::Value = HTTP.get(format!("{memos_url}/api/v1/memos?pageSize=200"))
        .header("Authorization", format!("Bearer {token}")).send().await?.json().await?;
    let memos = v["memos"].as_array().ok_or_else(|| anyhow::anyhow!("no memos"))?;

    // Extract unique writing dates (YYYY-MM-DD) from memo createTime
    let mut dates: std::collections::HashSet<String> = std::collections::HashSet::new();
    for m in memos {
        if let Some(time) = m["createTime"].as_str() {
            if time.len() >= 10 {
                dates.insert(time[..10].to_string());
            }
        }
    }

    if dates.is_empty() {
        return Ok(format!("{}\n\n_No memos yet — start writing to build a streak!_\n\n{}\n\n`{}` · #streak",
            tg_header("🔥", "Streak", ""), tg_footer("memogram-rs", "streak"), Local::now().format("%Y-%m-%d %H:%M")));
    }

    // Sort dates descending
    let mut sorted: Vec<String> = dates.into_iter().collect();
    sorted.sort();
    sorted.reverse();

    let today = Local::now().format("%Y-%m-%d").to_string();
    let yesterday = (Local::now() - chrono::Duration::days(1)).format("%Y-%m-%d").to_string();

    // Calculate current streak (consecutive days ending today or yesterday)
    let mut current_streak: u32 = 0;
    let mut check_date = if sorted[0] == today {
        today.clone()
    } else if sorted[0] == yesterday {
        yesterday.clone()
    } else {
        // Last memo was more than 1 day ago — streak is broken
        "broken".to_string()
    };

    if check_date != "broken" {
        for d in &sorted {
            if *d == check_date {
                current_streak += 1;
                // Move to previous day
                if let Ok(dt) = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d") {
                    check_date = (dt - chrono::Duration::days(1)).format("%Y-%m-%d").to_string();
                } else {
                    break;
                }
            } else if *d < check_date {
                break;
            }
        }
    }

    // Calculate longest streak
    let mut all_dates: Vec<String> = sorted.iter().cloned().collect();
    all_dates.sort();
    let mut longest_streak: u32 = 1;
    let mut run: u32 = 1;
    for i in 1..all_dates.len() {
        if let (Ok(prev), Ok(curr)) = (
            chrono::NaiveDate::parse_from_str(&all_dates[i - 1], "%Y-%m-%d"),
            chrono::NaiveDate::parse_from_str(&all_dates[i], "%Y-%m-%d"),
        ) {
            if curr - prev == chrono::Duration::days(1) {
                run += 1;
                if run > longest_streak { longest_streak = run; }
            } else {
                run = 1;
            }
        }
    }

    // Days active / total span
    let total_memos = memos.len();
    let unique_days = all_dates.len();
    let first = all_dates.last().unwrap_or(&today);
    let last = all_dates.first().unwrap_or(&today);
    let span_days = if let (Ok(a), Ok(b)) = (
        chrono::NaiveDate::parse_from_str(first, "%Y-%m-%d"),
        chrono::NaiveDate::parse_from_str(last, "%Y-%m-%d"),
    ) {
        (b - a).num_days() + 1
    } else {
        1
    };
    let consistency = if span_days > 0 { unique_days as f64 / span_days as f64 * 100.0 } else { 0.0 };

    // Recent 7-day activity
    let seven_days_ago = (Local::now() - chrono::Duration::days(7)).format("%Y-%m-%d").to_string();
    let recent_active: u32 = all_dates.iter().filter(|d| **d >= seven_days_ago).count() as u32;

    let fire = if current_streak >= 30 { "🔥🔥🔥" } else if current_streak >= 7 { "🔥🔥" } else if current_streak >= 1 { "🔥" } else { "💀" };

    let mut out = format!("{}\n\n", tg_header("🔥", "Streak", ""));
    out.push_str(&format!("**Current streak:** {} {} days\n", fire, current_streak));
    out.push_str(&format!("**Longest streak:** {} days\n\n", longest_streak));

    out.push_str("## 📊 Stats\n\n");
    out.push_str("| Metric | Value |\n|---|---|\n");
    out.push_str(&format!("| Total memos | {} |\n", total_memos));
    out.push_str(&format!("| Unique days | {} |\n", unique_days));
    out.push_str(&format!("| Day span | {} days |\n", span_days));
    out.push_str(&format!("| Consistency | {:.0}% |\n", consistency));
    out.push_str(&format!("| Last 7 days | {}/7 days |\n\n", recent_active));

    // Activity heatmap (last 14 days)
    out.push_str("## 📅 Last 14 Days\n\n");
    out.push_str("```\n");
    for i in 0..14 {
        let d = (Local::now() - chrono::Duration::days(i)).format("%Y-%m-%d").to_string();
        let label = (Local::now() - chrono::Duration::days(i)).format("%a %m/%d").to_string();
        let bar = if all_dates.contains(&d) { "██" } else { "░░" };
        let marker = if d == today { " ← today" } else { "" };
        out.push_str(&format!("{} {}{}\n", label, bar, marker));
    }
    out.push_str("```\n\n");

    if current_streak == 0 {
        out.push_str("💡 **Streak broken!** Write a memo today to start a new one.\n\n");
    } else if current_streak < 7 {
        out.push_str(&format!("💪 **{} more days** to hit a week streak!\n\n", 7 - current_streak));
    } else if current_streak < 30 {
        out.push_str(&format!("🚀 **{} more days** to hit 30 days!\n\n", 30 - current_streak));
    }

    out.push_str(&format!("{}\n\n`{}` · #streak", tg_footer("memogram-rs", "streak"), Local::now().format("%Y-%m-%d %H:%M")));
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

// --- number trivia ---

// --- utility functions ---

async fn set_reminder(args: &str, app: &App) -> String {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let mins: u64 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(5).min(1440);
    let msg_text = parts.get(1).unwrap_or(&"Reminder!");
    let bark_url = app.bark_url.clone();
    let ntfy_url = app.ntfy_url.clone();
    let msg_clone = msg_text.to_string();
    let title = format!("⏰ Reminder in {mins}min");
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(mins * 60)).await;
        // Bark
        if !bark_url.is_empty() {
            let bark_body = serde_json::json!({ "title": &title, "body": &msg_clone, "group": "memogram" });
            let _ = HTTP.post(&bark_url).json(&bark_body).send().await;
        }
        // ntfy
        if !ntfy_url.is_empty() {
            let _ = HTTP.post(&ntfy_url)
                .header("Title", &title)
                .header("Priority", "high")
                .body(msg_clone.clone())
                .send().await;
        }
    });
    let fire_at = Local::now() + chrono::Duration::minutes(mins as i64);
    let channels = [
        if !app.bark_url.is_empty() { Some("bark") } else { None },
        if !app.ntfy_url.is_empty() { Some("ntfy") } else { None },
    ].iter().filter_map(|x| *x).collect::<Vec<_>>().join(" + ");
    let channel_str = if channels.is_empty() { "no push configured".to_string() } else { channels };
    format!("⏰ **Reminder set**\n\n`{mins} min` — {msg_text}\n\n> fires at {} · via {} · #reminder", fire_at.format("%H:%M"), channel_str)
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
    format!("{}\n\n**Source:** `tldr.tech` · **Category:** `Tech/Science/Business` · **Bias:** `Curated`\n\n⚠️ _{}_\n\n## 📰 Alternative Tech News\n\n> [tldr.tech](https://tldr.tech) — Daily tech newsletter with curated stories\n> [hackernewsletter.com](https://hackernewsletter.com) — Weekly best of Hacker News\n> [techmeme.com](https://techmeme.com) — Tech news aggregator\n\n{}\n\n`{}` · #tldr #tech",
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

// --- markdown document generators ---

fn create_meeting(args: &str) -> String {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let topic = parts.first().filter(|s| !s.is_empty()).copied().unwrap_or("Untitled");
    let notes = parts.get(1).unwrap_or(&"");
    let date = Local::now().format("%Y-%m-%d").to_string();
    let time = Local::now().format("%H:%M").to_string();
    Md::new()
        .h2(&format!("🤝 Meeting"))
        .blank()
        .pi("Topic", topic)
        .pi("Date", &date)
        .pi("Time", &time)
        .blank()
        .push("## 👥 Attendees")
        .blank()
        .push("- _Add attendees_")
        .blank()
        .push("## 📋 Agenda")
        .blank()
        .push(&if notes.is_empty() { "- _Add agenda items_".to_string() } else { notes.lines().map(|l| format!("- {}", l)).collect::<Vec<_>>().join("\n") })
        .blank()
        .push("## 💬 Discussion")
        .blank()
        .push("_Notes go here_")
        .blank()
        .push("## ✅ Action Items")
        .blank()
        .push("- [ ] _Owner: Task description_")
        .blank()
        .push("## ➡️ Next Steps")
        .blank()
        .push("- _Schedule follow-up_")
        .blank()
        .table(&["Item", "Owner", "Due"], &[vec!["_Add items_".into(), "_Name_".into(), "_Date_".into()]])
        .blank()
        .push(&format!("{}\n\n`{}` · #meeting #notes", tg_footer("memogram", "meeting"), date))
        .build()
}

fn create_review(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d").to_string();
    let words = args.split_whitespace().count();
    Md::new()
        .h2(&format!("📊 Review"))
        .blank()
        .pi("Period", &now)
        .pi("Entries", &words.to_string())
        .blank()
        .push("## 📝 Summary")
        .blank()
        .p(args)
        .blank()
        .push("## ✅ Wins")
        .blank()
        .push("- _Add wins_")
        .blank()
        .push("## 🔧 Gaps")
        .blank()
        .push("- _Add gaps_")
        .blank()
        .push("## 📈 Metrics")
        .blank()
        .table(&["Metric", "Target", "Actual"], &[vec!["_Add metrics_".into(), "_Target_".into(), "_Actual_".into()]])
        .blank()
        .push("## ➡️ Next")
        .blank()
        .push("- _Add next actions_")
        .blank()
        .push("> _Tip: Review weekly. Keep wins visible, gaps actionable._")
        .blank()
        .push(&format!("{}\n\n`{}` · #review #planning", tg_footer("memogram", "review"), now))
        .build()
}

fn create_summary(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d").to_string();
    let words = args.split_whitespace().count();
    Md::new()
        .h2(&format!("📋 Summary"))
        .blank()
        .pi("Date", &now)
        .pi("Entries", &words.to_string())
        .blank()
        .push("## 📝 Content")
        .blank()
        .p(args)
        .blank()
        .push("## 📊 Stats")
        .blank()
        .table(&["Metric", "Value"], &[vec!["Words".into(), words.to_string()], vec!["Chars".into(), args.len().to_string()]])
        .blank()
        .push("## 🏷️ Tags")
        .blank()
        .push("- #summary #daily")
        .blank()
        .push(&format!("{}\n\n`{}` · #summary", tg_footer("memogram", "summary"), now))
        .build()
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


async fn fetch_book(args: &str) -> Result<String> {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let query = parts.first().filter(|s| !s.is_empty()).copied().unwrap_or("rust programming");
    let note = parts.get(1).unwrap_or(&"");
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let date = Local::now().format("%Y-%m-%d").to_string();
    // Try Open Library Search API
    let search_url = format!("https://openlibrary.org/search.json?title={}&limit=3", urlencoding::encode(query));
    let v: serde_json::Value = HTTP.get(&search_url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    let docs = v["docs"].as_array().cloned().unwrap_or_default();
    if docs.is_empty() {
        // Fallback: just make a nice template
        let mut out = format!("{}\n\n", tg_header("📚", "Book", query));
        out.push_str(&format!("**Title:** `{}`\n**Author:** `{}`\n**Started:** `{}`\n**Status:** 📖 Reading\n\n", query, note, date));
        out.push_str("## 📝 Summary\n\n- \n\n## 💡 Key Takeaways\n\n1. \n2. \n3. \n\n## 💬 Favorite Quotes\n\n> \"\" \n\n## 📊 Progress\n\n| Pages | % | Notes |\n|---|---|---|\n|  |  |  |\n\n");
        out.push_str(&format!("{}\n\n`{}` · #book", tg_footer("openlibrary.org", "book"), now));
        return Ok(out);
    }
    let first = &docs[0];
    let title = first["title"].as_str().unwrap_or(query);
    let author_name = first["author_name"].as_array()
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown");
    let year = first["first_publish_year"].as_i64()
        .map(|y| y.to_string())
        .unwrap_or_else(|| "—".into());
    let pages = first["number_of_pages_median"].as_i64()
        .map(|p| p.to_string())
        .unwrap_or_else(|| "—".into());
    let edition_count = first["edition_count"].as_i64().unwrap_or(0);
    let isbn = first["isbn"].as_array()
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let cover_i = first["cover_i"].as_i64().unwrap_or(0);
    let subjects = first["subject"].as_array()
        .map(|a| a.iter().take(5).filter_map(|s| s.as_str()).collect::<Vec<&str>>().join(", "))
        .unwrap_or_default();
    let mut out = format!("{}\n\n", tg_header("📚", "Book", title));
    out.push_str(&format!("**Title:** `{}`\n**Author:** `{}` · **Year:** `{}` · **Pages:** `{}`\n**Editions:** `{}`\n\n", title, author_name, year, pages, edition_count));
    if !subjects.is_empty() {
        out.push_str(&format!("## 🏷️ Subjects\n\n{}\n\n", subjects));
    }
    if cover_i > 0 {
        out.push_str(&format!("![Cover](https://covers.openlibrary.org/b/id/{}-M.jpg)\n\n", cover_i));
    }
    if !isbn.is_empty() {
        out.push_str(&format!("**ISBN:** `{}`\n", isbn));
    }
    out.push_str(&format!("🔗 [Open Library](https://openlibrary.org{})\n\n", first["key"].as_str().unwrap_or("")));
    out.push_str("## 📝 Summary\n\n- \n\n## 💡 Key Takeaways\n\n1. \n2. \n3. \n\n## 💬 Favorite Quotes\n\n> \"\" \n\n## 📊 Progress\n\n| Pages | % | Notes |\n|---|---|---|\n|  |  |  |\n\n");
    if !note.is_empty() {
        out.push_str(&format!("## 📌 Note\n\n{}\n\n", note));
    }
    out.push_str(&format!("{}\n\n`{}` · #book", tg_footer("openlibrary.org", "book"), now));
    Ok(out)
}

fn create_todo(args: &str) -> String {
    let items: Vec<&str> = args.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    if items.is_empty() { return "usage: `/todo buy milk, write report, call mom`".into(); }
    let total = items.len();
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    Md::new()
        .h2(&format!("✅ Todo List"))
        .blank()
        .pi("Total", &format!("{} items", total))
        .pi("Created", &now)
        .blank()
        .push("## 📋 Tasks")
        .blank()
        .push(&items.iter().enumerate().map(|(i, item)| format!("- [ ] **{}.** {}", i + 1, item)).collect::<Vec<_>>().join("\n"))
        .blank()
        .table(&["#", "Task", "Status"], &items.iter().enumerate().map(|(i, item)| vec![(i+1).to_string(), item.to_string(), "⏳ Pending".to_string()]).collect::<Vec<_>>())
        .blank()
        .push("## 📊 Stats")
        .blank()
        .table(&["Metric", "Value"], &[vec!["Total".into(), total.to_string()], vec!["Pending".into(), total.to_string()], vec!["Done".into(), "0".into()]])
        .blank()
        .push("> _Tip: Start with the hardest task first (Eat the Frog 🐸)_")
        .blank()
        .push(&format!("{}\n\n`{}` · #todo #tasks", tg_footer("memogram", "todo"), now))
        .build()
}

fn create_list(args: &str) -> String {
    let items: Vec<&str> = args.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    if items.is_empty() { return "usage: `/list apples, bananas, oranges`".into(); }
    let total = items.len();
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    Md::new()
        .h2(&format!("📝 List"))
        .blank()
        .pi("Items", &total.to_string())
        .pi("Created", &now)
        .blank()
        .push("## 📋 Items")
        .blank()
        .push(&items.iter().enumerate().map(|(i, item)| format!("{}. {}", i + 1, item)).collect::<Vec<_>>().join("\n"))
        .blank()
        .push("## 📊 Summary")
        .blank()
        .table(&["#", "Item"], &items.iter().enumerate().map(|(i, item)| vec![(i+1).to_string(), item.to_string()]).collect::<Vec<_>>())
        .blank()
        .push(&format!("{}\n\n`{}` · #list", tg_footer("memogram", "list"), now))
        .build()
}

fn create_clip(args: &str) -> String {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let url = parts.first().filter(|s| !s.is_empty()).copied().unwrap_or("");
    let notes = parts.get(1).unwrap_or(&"");
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let domain = url.split("://").nth(1).unwrap_or(url).split('/').next().unwrap_or(url);
    let is_code = url.contains("github.com") || url.contains("crates.io") || url.contains("docs.rs") || url.contains("stackoverflow.com");
    let category = if is_code { "💻 Code" } else if url.contains("youtube.com") || url.contains("youtu.be") { "🎬 Video" } else if url.contains("arxiv.org") || url.contains("pubmed") { "📚 Paper" } else { "🌐 Article" };
    Md::new()
        .h2(&format!("🔗 Bookmark"))
        .blank()
        .pi("URL", &format!("[{}]({})", domain, url))
        .pi("Category", category)
        .pi("Saved", &now)
        .blank()
        .push("## 📝 Notes")
        .blank()
        .p(notes)
        .blank()
        .push("## 🏷️ Tags")
        .blank()
        .push(&format!("- #bookmark #{}", if is_code { "code" } else { "read" }))
        .blank()
        .push("## 🔗 Related")
        .blank()
        .push("- _Add related links here_")
        .blank()
        .push(&format!("{}\n\n`{}` · #bookmark", tg_footer("memogram", "clip"), now))
        .build()
}

fn create_link(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let url = parts.first().unwrap_or(&"https://example.com");
    let desc = parts.get(1).unwrap_or(&"");
    let domain = url.split("://").nth(1).unwrap_or(url).split('/').next().unwrap_or(url);
    Md::new()
        .h2(&format!("🔗 Link"))
        .blank()
        .pi("URL", &format!("[{}]({})", domain, url))
        .pi("Saved", &now)
        .blank()
        .push("## 📝 Why")
        .blank()
        .p(desc)
        .blank()
        .push("## 🏷️ Tags")
        .blank()
        .push("- #link #inbox")
        .blank()
        .push("## 🔗 Related")
        .blank()
        .push("- _Add related links here_")
        .blank()
        .push(&format!("{}\n\n`{}` · #link", tg_footer("memogram", "link"), now))
        .build()
}

fn create_snippet(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let lang = if args.contains("fn ") || args.contains("let ") || args.contains("impl ") { "rust" }
        else if args.contains("def ") || args.contains("import ") || args.contains("class ") { "python" }
        else if args.contains("function ") || args.contains("const ") || args.contains("=> ") { "javascript" }
        else if args.contains("SELECT ") || args.contains("FROM ") { "sql" }
        else { "text" };
    let lines = args.lines().count();
    let chars = args.len();
    Md::new()
        .h2(&format!("📝 Snippet"))
        .blank()
        .pi("Language", lang)
        .pi("Saved", &now)
        .pi("Size", &format!("{} lines, {} chars", lines, chars))
        .blank()
        .push("## 📋 Code")
        .blank()
        .code_block(lang, args)
        .blank()
        .push("## 💡 Context")
        .blank()
        .push("- _Add usage context here_")
        .blank()
        .push("## 🔗 Source")
        .blank()
        .push("- _Add source URL here_")
        .blank()
        .push(&format!("{}\n\n`{}` · #snippet #{}", tg_footer("memogram", "snippet"), now, lang))
        .build()
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
    Ok(Md::new()
        .h2("🏛️ Stoic Wisdom")
        .blank()
        .pi("Source", "stoic-quotes.com")
        .pi("Author", &author)
        .blank()
        .push("## 📜 Quote")
        .blank()
        .quote(&text)
        .blank()
        .push(&format!("— **{}**", author))
        .blank()
        .push("## 📖 About the Author")
        .blank()
        .push(&match author.as_str() {
            "Marcus Aurelius" => "- Roman Emperor (161-180 AD), philosopher-king, wrote *Meditations*",
            "Seneca" => "- Roman Stoic philosopher, statesman, wrote *Letters from a Stoic*",
            "Epictetus" => "- Greek Stoic philosopher, former slave, taught *dichotomy of control*",
            _ => "- Ancient Greek or Roman Stoic philosopher",
        })
        .blank()
        .push("## 💡 Stoic Principles")
        .blank()
        .push("- **Dichotomy of Control** — Focus only on what you can control")
        .push("- **Amor Fati** — Love your fate, embrace what happens")
        .push("- **Memento Mori** — Remember death to live fully")
        .blank()
        .push(&format!("{}\n\n`{}` · #stoic #wisdom", tg_footer("stoic-quotes.com", "stoic"), Local::now().format("%Y-%m-%d %H:%M")))
        .build())
}

fn create_mood_entry(args: &str) -> String {
    let args = args.trim();
    let date = Local::now().format("%Y-%m-%d").to_string();
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();

    // Sub-commands: gratitude, journal
    if let Some(rest) = args.strip_prefix("gratitude ") {
        let items: Vec<&str> = rest.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        if items.is_empty() { return "usage: `/mood gratitude family, health, code`".into(); }
        let mut out = format!("# 🙏 Gratitude — `{}`\n\n**Date:** `{}`\n\n## ✨ Today\n\n", date, now);
        for item in &items {
            out.push_str(&format!("- ✨ {}\n", item));
        }
        out.push_str("\n## 📊 Weekly\n\n| Date | Count | Themes |\n|---|---|---|\n");
        out.push_str(&format!("| {} | {} | {} |\n", date, items.len(), items.join(", ")));
        out.push_str("\n> _Tip: 3 specific, 1 why it matters._\n\n");
        out.push_str(&format!("{}\n\n`{}` · #wellness #memogram-rs", tg_header("🙏", "Gratitude", &date), now));
        return out;
    }
    if let Some(rest) = args.strip_prefix("journal ") {
        if rest.trim().is_empty() { return "usage: `/mood journal <text>`".into(); }
        let mut out = format!("# 📔 Journal — `{}`\n\n**Date:** `{}`\n\n## ✍️ Entry\n\n{}\n\n## 🔍 Reflection\n\n| Prompt | Response |\n|---|---|\n| What went well? |  |\n| What was hard? |  |\n| Gratitude |  |\n| Tomorrow |  |\n\n> _Tip: 5m free write, no editing. End with 1 gratitude._\n\n", date, now, rest);
        out.push_str(&format!("{}\n\n`{}` · #wellness #memogram-rs", tg_header("📔", "Journal", &date), now));
        return out;
    }
    if let Some(rest) = args.strip_prefix("reflection ") {
        if rest.trim().is_empty() { return "usage: `/mood reflection <text>`".into(); }
        let mut out = format!("# 🪞 Reflection — `{}`\n\n**Date:** `{}`\n\n## 💭 Prompt\n\n{}\n\n## 🔍 Insights\n\n- \n\n## ✅ Action\n\n- [ ] \n\n## 📊 Mood\n\n| Energy | Stress | Gratitude |\n|---|---|---|\n| /10 | /10 |  |\n\n> _Tip: What went well? What was hard? What's tomorrow?_\n\n", date, now, rest);
        out.push_str(&format!("{}\n\n`{}` · #wellness #memogram-rs", tg_header("🪞", "Reflection", &date), now));
        return out;
    }

    // Default: mood log
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let mood = parts.first().filter(|s| !s.is_empty()).copied().unwrap_or("neutral");
    let note = parts.get(1).unwrap_or(&"");
    let day = Local::now().format("%Y-%m-%d").to_string();
    format!(
        "# 😊 Mood — `{}`\n\n**Date:** `{}` · **Mood:** `{}`\n**Note:** {}\n\n## 📊 Check\n\n| Mood | Energy | Stress |\n|---|---|---|\n| {} | /10 | /10 |\n\n## 📈 Last 7 Days (sample)\n\n| Date | Mood | Note |\n|---|---|---|\n| {} | {} | {} |\n| 2026-09-03 | ok |  |\n| 2026-09-02 | good |  |\n\n> _Tip: Name it to tame it. 1 breath, note 1 good._\n\n{}\n\n`{}` · #wellness #memogram-rs",
        mood, date, mood, note, mood, day, mood, note, tg_header("😊", "Mood", mood), date
    )
}

// === WELLNESS: EVIDENCE-BASED API-POWERED COMMANDS ===

async fn fetch_workout(query: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Ok("usage: `/workout <muscle>` — e.g. `/workout chest`, `/workout back`, `/workout legs`".into());
    }
    let url = format!("https://v2.exercisedb.dev/exercises?limit=10&offset=0");
    let v: serde_json::Value = match tokio::time::timeout(std::time::Duration::from_secs(8), HTTP.get(&url).header("User-Agent", "memogram-rs").send()).await {
        Ok(Ok(r)) => r.json().await.unwrap_or(serde_json::Value::Null),
        _ => serde_json::Value::Null,
    };
    let all = v.as_array().cloned().unwrap_or_default();
    let matched: Vec<&serde_json::Value> = all.iter().filter(|e| {
        let body = e["bodyParts"].as_array().map(|a| a.iter().any(|b| b.as_str().unwrap_or("").to_lowercase().contains(&query))).unwrap_or(false);
        let muscles = e["targetMuscles"].as_array().map(|a| a.iter().any(|m| m.as_str().unwrap_or("").to_lowercase().contains(&query))).unwrap_or(false);
        let name = e["name"].as_str().unwrap_or("").to_lowercase().contains(&query);
        body || muscles || name
    }).collect();

    let mut out = format!("{}\n\n", tg_header("💪", "Workout Plan", &query));
    if matched.is_empty() {
        out.push_str("_No exercises found. Try: chest, back, legs, shoulders, arms, core, cardio_\n\n");
        out.push_str(&format!("{}\n\n`{}` · #workout #wellness #memogram-rs", tg_footer("exercisedb", "workout"), now));
        return Ok(out);
    }
    out.push_str(&format!("**{} exercises** for _{}_\n\n", matched.len(), query));
    for (i, ex) in matched.iter().enumerate() {
        let name = ex["name"].as_str().unwrap_or("?");
        let body_parts: Vec<String> = ex["bodyParts"].as_array().map(|a| a.iter().filter_map(|b| b.as_str()).map(|s| format!("`{}`", s)).collect()).unwrap_or_default();
        let muscles: Vec<String> = ex["targetMuscles"].as_array().map(|a| a.iter().filter_map(|m| m.as_str()).map(|s| format!("`{}`", s)).collect()).unwrap_or_default();
        let equip = ex["equipments"].as_array().map(|a| a.iter().filter_map(|e| e.as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default();
        let ex_type = ex["exerciseType"].as_str().unwrap_or("?");
        let overview = ex["overview"].as_str().unwrap_or("");
        let instructions: Vec<String> = ex["instructions"].as_array().map(|a| a.iter().filter_map(|s| s.as_str()).map(|s| s.to_string()).collect()).unwrap_or_default();
        let tips: Vec<String> = ex["exerciseTips"].as_array().map(|a| a.iter().filter_map(|s| s.as_str()).map(|s| s.to_string()).collect()).unwrap_or_default();

        out.push_str(&format!("### {}. {} \n\n", i + 1, name));
        out.push_str("| Detail | Value |\n|---|---|\n");
        out.push_str(&format!("| Body Part | {} |\n", body_parts.join(", ")));
        out.push_str(&format!("| Target Muscles | {} |\n", muscles.join(", ")));
        out.push_str(&format!("| Equipment | `{}` |\n", equip));
        out.push_str(&format!("| Type | `{}` |\n\n", ex_type));
        if !overview.is_empty() {
            out.push_str(&format!("> {}\n\n", overview));
        }
        if !instructions.is_empty() {
            out.push_str("**Steps:**\n");
            for (j, step) in instructions.iter().enumerate() {
                out.push_str(&format!("{}. {}\n", j + 1, step));
            }
            out.push('\n');
        }
        if !tips.is_empty() {
            out.push_str("**Tips:**\n");
            for tip in &tips {
                out.push_str(&format!("- {}\n", tip));
            }
            out.push('\n');
        }
    }
    out.push_str(&format!("{}\n\n`{}` · #workout #wellness #memogram-rs", tg_footer("exercisedb.dev", "workout"), now));
    Ok(out)
}

async fn fetch_health(args: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let parts: Vec<&str> = args.split_whitespace().collect();
    if parts.len() < 3 {
        return Ok("usage: `/health <weight_kg> <height_cm> <age> [male|female]`\n\nExample: `/health 75 180 25 male`".into());
    }
    let weight: f64 = parts[0].parse().unwrap_or(0.0);
    let height: f64 = parts[1].parse().unwrap_or(0.0);
    let age: u32 = parts[2].parse().unwrap_or(25);
    let gender = parts.get(3).unwrap_or(&"male").to_lowercase();

    if weight <= 0.0 || height <= 0.0 { return Ok("⚠️ Invalid weight or height.".into()); }

    let url = format!("https://myplate.food/api/v1/calculate/calorie-needs?weight={}&height={}&age={}&gender={}", weight, height, age, gender);
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;

    let height_m = height / 100.0;
    let bmi = weight / (height_m * height_m);
    let bmi_cat = if bmi < 18.5 { "Underweight" } else if bmi < 25.0 { "Normal" } else if bmi < 30.0 { "Overweight" } else { "Obese" };
    let bmr = v["bmr"].as_f64().unwrap_or(0.0);
    let tdee = v["tdee"].as_object().map(|o| o.len()).unwrap_or(0);

    let mut out = format!("{}\n\n", tg_header("🏥", "Health Dashboard", &format!("{}kg {}cm {}yr {}", weight, height, age, gender)));
    out.push_str(&format!("**Date:** `{}`\n\n", now));

    // BMI
    out.push_str("## 📊 Body Mass Index\n\n");
    out.push_str("| Metric | Value | Category |\n|---|---|---|\n");
    out.push_str(&format!("| BMI | `{:.1}` | **{}** |\n\n", bmi, bmi_cat));
    out.push_str("```\n");
    out.push_str(&format!("Underweight: <18.5\nNormal:       18.5-24.9  ← {}\nOverweight:   25.0-29.9\nObese:        30.0+\n", if bmi < 25.0 { "you are here" } else { "" }));
    out.push_str("```\n\n");

    // TDEE
    out.push_str("## 🔥 Total Daily Energy Expenditure\n\n");
    out.push_str("| Activity Level | Calories/day |\n|---|---|\n");
    if let Some(tdee_obj) = v["tdee"].as_object() {
        for (level, cal) in tdee_obj {
            let cal_num = cal.as_f64().unwrap_or(0.0) as u32;
            out.push_str(&format!("| {} | `{}` |\n", level.replace('-', " "), cal_num));
        }
    }
    out.push('\n');

    // BMR
    out.push_str(&format!("**BMR (Mifflin-St Jeor):** `{}` cal/day\n\n", bmr as u32));

    // Macros
    out.push_str("## 🥗 Recommended Macros (at maintenance)\n\n");
    out.push_str("| Macro | Grams | Calories | % |\n|---|---|---|---|\n");
    if let Some(macros) = v["macros"].as_object() {
        let total_cal = v["tdee"]["lightly-active"].as_f64().unwrap_or(2000.0);
        for (name, data) in macros {
            let grams = data["grams"].as_f64().unwrap_or(0.0);
            let cal_per_g = match name.as_str() {
                "protein" => 4.0,
                "carbohydrates" => 4.0,
                "fat" => 9.0,
                _ => 4.0,
            };
            let cal = grams * cal_per_g;
            let pct = if total_cal > 0.0 { cal / total_cal * 100.0 } else { 0.0 };
            out.push_str(&format!("| **{}** | `{:.0}g` | `{:.0}` | `{:.0}%` |\n", name, grams, cal, pct));
        }
    }
    out.push('\n');

    // Ideal weight
    out.push_str("## ⚖️ Ideal Weight Ranges\n\n");
    out.push_str("| Formula | Weight |\n|---|---|\n");
    if let Some(iw) = v["ideal_weight"].as_object() {
        for (formula, wt) in iw {
            if let Some(w) = wt.as_f64() {
                out.push_str(&format!("| {} | `{:.1} kg` |\n", formula, w));
            }
        }
    }
    if let Some(range) = v["healthy_bmi_range"].as_str() {
        out.push_str(&format!("| Healthy BMI Range | `{}` |\n", range));
    }
    out.push('\n');

    // Hydration
    let water_liters = weight * 0.033;
    out.push_str("## 💧 Hydration Target\n\n");
    out.push_str(&format!("| Guideline | Target |\n|---|---|\n"));
    out.push_str(&format!("| Water (33ml/kg) | `{:.1}L` ({:.0}oz) |\n", water_liters, water_liters * 33.8));
    out.push_str(&format!("| Glasses (250ml) | `{}` |\n\n", (water_liters * 4.0) as u32));

    // Deficit tiers
    if let Some(deficits) = v["deficit_tiers"].as_object() {
        out.push_str("## 📉 Weight Loss Plan\n\n");
        out.push_str("| Tier | Calories | Weekly Loss |\n|---|---|---|\n");
        for (tier, data) in deficits {
            let cal = data["calories"].as_f64().unwrap_or(0.0) as u32;
            let loss = data["weekly_loss_kg"].as_f64().unwrap_or(0.0);
            out.push_str(&format!("| {} | `{}` | `{:.2} kg/week` |\n", tier, cal, loss));
        }
        out.push('\n');
    }

    out.push_str("> _Source: Mifflin-St Jeor equation, USDA DRI data. Not medical advice._\n\n");
    out.push_str(&format!("{}\n\n`{}` · #health #wellness #memogram-rs", tg_footer("myplate.food", "health"), now));
    Ok(out)
}

async fn fetch_nutrition(query: &str, api_key: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let query = query.trim();
    if query.is_empty() { return Ok("usage: `/nutrition <food>` — e.g. `/nutrition 2 eggs and toast`".into()); }
    if api_key.is_empty() { return Ok("⚠️ `API_NINJAS_KEY` not set. Get free key at api.api-ninjas.com".into()); }

    let url = format!("https://api.api-ninjas.com/v1/nutrition?query={}", urlencoding::encode(query));
    let v: serde_json::Value = HTTP.get(&url).header("X-Api-Key", api_key).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;

    let items = v.as_array().ok_or_else(|| anyhow::anyhow!("no results"))?;
    if items.is_empty() {
        return Ok(format!("{}\n\n_No food found for `{}`_\n\n{}", tg_header("🥗", "Nutrition", query), query, tg_footer("api-ninjas.com", "nutrition")));
    }

    let mut out = format!("{}\n\n", tg_header("🥗", "Nutrition Breakdown", query));
    let mut total_cal = 0.0_f64;
    let mut total_protein = 0.0_f64;
    let mut total_carbs = 0.0_f64;
    let mut total_fat = 0.0_f64;
    let mut total_fiber = 0.0_f64;

    for item in items {
        let name = item["name"].as_str().unwrap_or("?");
        let cal = item["calories"].as_f64().unwrap_or(0.0);
        let serving = item["serving_size_g"].as_f64().unwrap_or(0.0);
        let protein = item["protein_g"].as_f64().unwrap_or(0.0);
        let carbs = item["carbohydrates_total_g"].as_f64().unwrap_or(0.0);
        let fat = item["fat_total_g"].as_f64().unwrap_or(0.0);
        let fiber = item["fiber_g"].as_f64().unwrap_or(0.0);
        let sugar = item["sugar_g"].as_f64().unwrap_or(0.0);
        let sat_fat = item["fat_saturated_g"].as_f64().unwrap_or(0.0);
        let sodium = item["sodium_mg"].as_f64().unwrap_or(0.0);
        let cholesterol = item["cholesterol_mg"].as_f64().unwrap_or(0.0);
        let potassium = item["potassium_mg"].as_f64().unwrap_or(0.0);

        total_cal += cal; total_protein += protein; total_carbs += carbs; total_fat += fat; total_fiber += fiber;

        out.push_str(&format!("### 🍽️ {} ({:.0}g)\n\n", name, serving));
        out.push_str("| Nutrient | Amount |\n|---|---|\n");
        out.push_str(&format!("| Calories | `{:.0}` kcal |\n", cal));
        out.push_str(&format!("| Protein | `{:.1}g` |\n", protein));
        out.push_str(&format!("| Carbs | `{:.1}g` |\n", carbs));
        out.push_str(&format!("| Fat | `{:.1}g` (saturated: {:.1}g) |\n", fat, sat_fat));
        out.push_str(&format!("| Fiber | `{:.1}g` |\n", fiber));
        out.push_str(&format!("| Sugar | `{:.1}g` |\n", sugar));
        out.push_str(&format!("| Sodium | `{:.0}mg` |\n", sodium));
        out.push_str(&format!("| Cholesterol | `{:.0}mg` |\n", cholesterol));
        out.push_str(&format!("| Potassium | `{:.0}mg` |\n\n", potassium));
    }

    if items.len() > 1 {
        out.push_str("## 📊 Totals\n\n");
        out.push_str("| Nutrient | Total |\n|---|---|\n");
        out.push_str(&format!("| Calories | `{:.0}` kcal |\n", total_cal));
        out.push_str(&format!("| Protein | `{:.1}g` |\n", total_protein));
        out.push_str(&format!("| Carbs | `{:.1}g` |\n", total_carbs));
        out.push_str(&format!("| Fat | `{:.1}g` |\n", total_fat));
        out.push_str(&format!("| Fiber | `{:.1}g` |\n\n", total_fiber));
    }

    out.push_str(&format!("{}\n\n`{}` · #nutrition #wellness #memogram-rs", tg_footer("api-ninjas.com", "nutrition"), now));
    Ok(out)
}

async fn fetch_meal(query: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let query = query.trim();
    if query.is_empty() { return Ok("usage: `/meal <cuisine or dish>` — e.g. `/meal italian`, `/meal chicken`".into()); }

    let url = format!("https://www.themealdb.com/api/json/v1/1/search.php?s={}", urlencoding::encode(query));
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;

    let meals = v["meals"].as_array().ok_or_else(|| anyhow::anyhow!("no meals"))?;
    if meals.is_empty() || meals[0].is_null() {
        return Ok(format!("{}\n\n_No meals found for `{}`_\n\n> Try: `chicken`, `pasta`, `curry`, `salad`, `mexican`\n\n{}\n\n`{}` · #meal #wellness #memogram-rs",
            tg_header("🍽️", "Recipe", query), query, tg_footer("themealdb.com", "meal"), now));
    }

    let meal = &meals[0];
    let name = meal["strMeal"].as_str().unwrap_or("?");
    let category = meal["strCategory"].as_str().unwrap_or("?");
    let area = meal["strArea"].as_str().unwrap_or("?");
    let instructions = meal["strInstructions"].as_str().unwrap_or("");
    let tags = meal["strTags"].as_str().unwrap_or("None");
    let youtube = meal["strYoutube"].as_str().unwrap_or("");

    let mut out = format!("{}\n\n", tg_header("🍽️", name, area));
    out.push_str(&format!("**Category:** `{}` · **Cuisine:** `{}` · **Tags:** `{}`\n\n", category, area, tags));

    // Ingredients table
    out.push_str("## 📋 Ingredients\n\n");
    out.push_str("| Ingredient | Measure |\n|---|---|\n");
    for i in 1..=20 {
        let ingredient = meal[&format!("strIngredient{}", i)].as_str().unwrap_or("").trim();
        let measure = meal[&format!("strMeasure{}", i)].as_str().unwrap_or("").trim();
        if !ingredient.is_empty() {
            out.push_str(&format!("| {} | {} |\n", ingredient, measure));
        }
    }
    out.push('\n');

    // Instructions
    out.push_str("## 🔪 Instructions\n\n");
    for (i, step) in instructions.split("\r\n").filter(|s| !s.trim().is_empty()).enumerate() {
        out.push_str(&format!("{}. {}\n\n", i + 1, step.trim()));
    }

    if !youtube.is_empty() {
        out.push_str(&format!("🎬 [Video Tutorial]({})\n\n", youtube));
    }

    out.push_str(&format!("{}\n\n`{}` · #meal #wellness #memogram-rs", tg_footer("themealdb.com", "meal"), now));
    Ok(out)
}

fn create_breathe(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let technique = args.trim().to_lowercase();

    let techniques = vec![
        ("box", "Box Breathing (4-4-4-4)", "Navy SEALs", "Stress relief, focus, calm",
         vec![
             "Sit upright. Close eyes. Breathe naturally for 30 seconds.",
             "INHALE slowly through nose for 4 seconds.",
             "HOLD breath for 4 seconds.",
             "EXHALE slowly through mouth for 4 seconds.",
             "HOLD empty for 4 seconds.",
             "Repeat for 4-6 cycles (2-3 minutes).",
         ],
         "Proven to activate parasympathetic nervous system. Used by Navy SEALs for combat stress. Studies show reduced cortisol in 5 minutes."),
        ("478", "4-7-8 Breathing", "Dr. Andrew Weil", "Sleep, anxiety, panic",
         vec![
             "Place tongue tip behind upper front teeth.",
             "Exhale completely through mouth with whoosh sound.",
             "INHALE quietly through nose for 4 seconds.",
             "HOLD breath for 7 seconds.",
             "EXHALE completely through mouth for 8 seconds.",
             "Repeat 4 cycles. Build to 8 cycles over weeks.",
         ],
         "Dr. Weil calls this 'a natural tranquilizer for the nervous system.' Clinical evidence for reducing anxiety and aiding sleep onset."),
        ("3min", "3-Minute Breathing Space", "MBCT (Segal, Williams, Teasdale)", "Daily mindfulness, mood regulation",
         vec![
             "MINUTE 1 — ACKNOWLEDGE: What am I experiencing right now? Notice thoughts, feelings, body sensations. Name them.",
             "MINUTE 2 — GATHER: Focus attention on breathing. Feel the abdomen rise and fall. Anchor to present moment.",
             "MINUTE 3 — EXPAND: Expand awareness to whole body. Carry this expanded awareness into the rest of your day.",
         ],
         "Core practice of Mindfulness-Based Cognitive Therapy (MBCT). Evidence-based for preventing depressive relapse. Recommended by NICE guidelines (UK)."),
        ("physiological", "Physiological Sigh", "Stanford (Huberman Lab)", "Fastest calm-down (1 breath)",
         vec![
             "INHALE through nose (full breath).",
             "Without exhaling, INHALE again through nose (double inhale — tops off alveoli).",
             "LONG EXHALE through mouth (slow, extended — 6-8 seconds).",
             "Even one cycle activates calm. 2-3 cycles for full effect.",
         ],
         "Stanford research (2023): double inhale + extended exhale is the fastest known way to voluntarily reduce stress. Works in a single breath cycle."),
    ];

    let mut out = format!("{}\n\n", tg_header("🫁", "Breathing Exercise", if technique.is_empty() { "pick a technique" } else { &technique }));

    if technique.is_empty() {
        out.push_str("**Techniques:** `box`, `478`, `3min`, `physiological`\n\n");
        for (id, name, source, use_case, _, _) in &techniques {
            out.push_str(&format!("- **/breathe {}** — {} ({})\n  📍 For: {}\n\n", id, name, source, use_case));
        }
        out.push_str(&format!("{}\n\n`{}` · #breathe #wellness #memogram-rs", tg_footer("evidence-based", "breathe"), now));
        return out;
    }

    for (id, name, source, use_case, steps, evidence) in &techniques {
        if *id == technique {
            out.push_str(&format!("**Technique:** {} \n**Source:** {} \n**Use for:** {}\n\n", name, source, use_case));
            out.push_str("## 📋 Protocol\n\n");
            for (i, step) in steps.iter().enumerate() {
                out.push_str(&format!("{}. {}\n\n", i + 1, step));
            }
            out.push_str("## 📚 Evidence\n\n");
            out.push_str(&format!("> {}\n\n", evidence));
            out.push_str(&format!("{}\n\n`{}` · #breathe #wellness #memogram-rs", tg_footer("evidence-based", "breathe"), now));
            return out;
        }
    }

    out.push_str("_Unknown technique. Available: `box`, `478`, `3min`, `physiological`_\n");
    out.push_str(&format!("\n{}\n\n`{}` · #breathe #wellness #memogram-rs", tg_footer("evidence-based", "breathe"), now));
    out
}

async fn fetch_calories(args: &str, api_key: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let activity = parts.first().filter(|s| !s.is_empty()).copied().unwrap_or("");
    let duration: f64 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(30.0);

    if activity.is_empty() {
        return Ok("usage: `/calories <activity> <minutes>` — e.g. `/calories running 30`".into());
    }
    if api_key.is_empty() { return Ok("⚠️ `API_NINJAS_KEY` not set.".into()); }

    let url = format!("https://api.api-ninjas.com/v1/caloriesburned?activity={}&duration={}", urlencoding::encode(activity), duration);
    let v: serde_json::Value = HTTP.get(&url).header("X-Api-Key", api_key).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;

    let items = v.as_array().ok_or_else(|| anyhow::anyhow!("no results"))?;
    if items.is_empty() {
        return Ok(format!("{}\n\n_No activity found for `{}`_\n\n> Try: running, cycling, swimming, walking, weight training, yoga\n\n{}\n\n`{}` · #calories #wellness #memogram-rs",
            tg_header("🔥", "Calories Burned", activity), activity, tg_footer("api-ninjas.com", "calories"), now));
    }

    let item = &items[0];
    let total_cal = item["total_calories"].as_f64().unwrap_or(0.0);
    let name = item["name"].as_str().unwrap_or(activity);
    let total_duration = item["total_duration"].as_f64().unwrap_or(duration);
    let calories_per_min = item["calories_per_hour"].as_f64().unwrap_or(0.0) / 60.0;
    let met = item["met"].as_f64().unwrap_or(0.0);

    let mut out = format!("{}\n\n", tg_header("🔥", "Calories Burned", name));
    out.push_str(&format!("**Activity:** `{}` · **Duration:** `{} min`\n\n", name, total_duration as u32));

    out.push_str("## 📊 Summary\n\n");
    out.push_str("| Metric | Value |\n|---|---|\n");
    out.push_str(&format!("| Total Calories | **`{:.0}` kcal** |\n", total_cal));
    out.push_str(&format!("| Calories/min | `{:.1}` kcal |\n", calories_per_min));
    out.push_str(&format!("| MET value | `{:.1}` |\n\n", met));

    // MET explanation
    out.push_str("## 📖 What is MET?\n\n");
    out.push_str("| MET | Intensity | Examples |\n|---|---|---|\n");
    out.push_str("| 1-2 | Light | Walking, stretching |\n");
    out.push_str("| 3-5 | Moderate | Brisk walking, cycling, yoga |\n");
    out.push_str("| 6-8 | Vigorous | Running, swimming, sports |\n");
    out.push_str("| 9+ | Intense | Sprinting, HIIT, rowing |\n\n");

    // Weekly projection
    let weekly = total_cal * 7.0;
    let monthly = total_cal * 30.0;
    out.push_str("## 📈 If You Do This Daily\n\n");
    out.push_str("| Period | Calories |\n|---|---|\n");
    out.push_str(&format!("| Weekly | `{:}` kcal |\n", weekly as u32));
    out.push_str(&format!("| Monthly | `{:}` kcal |\n", monthly as u32));
    out.push_str(&format!("| ~Fat equivalent | `{:.1} kg/month` |\n\n", monthly as f64 / 7700.0));

    out.push_str(&format!("{}\n\n`{}` · #calories #wellness #memogram-rs", tg_footer("api-ninjas.com", "calories"), now));
    Ok(out)
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
    let repo = resp["repository"]["url"].as_str().unwrap_or("");
    let license = resp["license"].as_str().unwrap_or("-");
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let mut out = format!("{}\n\n", tg_header("📦", "npm", &format!("{}@{}", name, version)));
    out.push_str(&format!("**Package:** `{}` · **Version:** `{}` · **License:** `{}`\n\n", name, version, license));
    out.push_str(&format!("## 📝 Description\n\n{}\n\n", desc));
    if !homepage.is_empty() { out.push_str(&format!("**Homepage:** [{}]({})\n", homepage, homepage)); }
    if !repo.is_empty() { out.push_str(&format!("**Repository:** [{}]({})\n", repo, repo)); }
    out.push_str(&format!("\n**Install:**\n\n```\nnpm install {}\n```\n\n", name));
    out.push_str(&format!("{}\n\n`{}` · #npm", tg_footer("npmjs.com", "npm"), now));
    Ok(out)
}

async fn fetch_pypi(pkg: &str) -> Result<String> {
    let resp: serde_json::Value = HTTP.get(format!("https://pypi.org/pypi/{}/json", urlencoding::encode(pkg))).send().await?.json().await?;
    if let Some(msg) = resp["message"].as_str() { return Ok(format!("pypi: {msg}")); }
    let info = &resp["info"];
    let name = info["name"].as_str().unwrap_or("?");
    let version = info["version"].as_str().unwrap_or("?");
    let summary = info["summary"].as_str().unwrap_or("?");
    let license = info["license"].as_str().unwrap_or("-");
    let author = info["author"].as_str().unwrap_or("");
    let home = info["home_page"].as_str().unwrap_or("");
    let requires_python = info["requires_python"].as_str().unwrap_or("");
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let mut out = format!("{}\n\n", tg_header("📦", "PyPI", &format!("{}@{}", name, version)));
    out.push_str(&format!("**Package:** `{}` · **Version:** `{}` · **License:** `{}`\n\n", name, version, license));
    if !author.is_empty() { out.push_str(&format!("**Author:** {}\n", author)); }
    if !requires_python.is_empty() { out.push_str(&format!("**Requires:** Python {}\n", requires_python)); }
    out.push_str("\n");
    out.push_str(&format!("## 📝 Description\n\n{}\n\n", summary));
    // Get download stats from pypistats
    let stats_url = format!("https://pypistats.org/api/packages/{}/recent", urlencoding::encode(name));
    if let Ok(sv) = HTTP.get(&stats_url).timeout(std::time::Duration::from_secs(5)).send().await {
        if let Ok(sj) = sv.json::<serde_json::Value>().await {
            if let Some(data) = sj["data"].as_object() {
                let last_day = data["last_day"].as_u64().unwrap_or(0);
                let last_week = data["last_week"].as_u64().unwrap_or(0);
                let last_month = data["last_month"].as_u64().unwrap_or(0);
                out.push_str("## 📊 Downloads\n\n");
                out.push_str("| Period | Downloads |\n|---|---|\n");
                out.push_str(&format!("| Last Day | `{}` |\n", last_day));
                out.push_str(&format!("| Last Week | `{}` |\n", last_week));
                out.push_str(&format!("| Last Month | `{}` |\n\n", last_month));
            }
        }
    }
    // Show top 5 dependencies
    if let Some(requires) = info["requires_dist"].as_array() {
        let deps: Vec<String> = requires.iter().take(5).filter_map(|d| d.as_str()).map(|s| {
            let name = s.split(';').next().unwrap_or(s).split('(').next().unwrap_or(s).trim();
            format!("`{}`", name)
        }).collect();
        if !deps.is_empty() {
            out.push_str(&format!("## 📎 Dependencies (top 5)\n\n{}\n\n", deps.join(" · ")));
        }
    }
    if !home.is_empty() { out.push_str(&format!("**Homepage:** [{}]({})\n", home, home)); }
    out.push_str(&format!("\n**Install:**\n\n```\npip install {}\n```\n\n", name));
    out.push_str(&format!("{}\n\n`{}` · #pypi", tg_footer("pypi.org", "pypi"), now));
    Ok(out)
}

async fn fetch_crates(pkg: &str) -> Result<String> {
    let resp: serde_json::Value = HTTP.get(format!("https://crates.io/api/v1/crates/{}", urlencoding::encode(pkg))).send().await?.json().await?;
    if let Some(c) = resp["crate"].as_object() {
        let name = c["name"].as_str().unwrap_or("?");
        let version = c["max_version"].as_str().unwrap_or("?");
        let desc = c["description"].as_str().unwrap_or("?");
        let downloads = c["downloads"].as_i64().unwrap_or(0);
        let license = c.get("license").and_then(|v| v.as_str()).unwrap_or("-");
        let repo = c["repository"].as_str().unwrap_or("");
        let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
        let mut out = format!("{}\n\n", tg_header("📦", "crates.io", &format!("{}@{}", name, version)));
        out.push_str(&format!("**Crate:** `{}` · **Version:** `{}` · **License:** `{}`\n\n", name, version, license));
        out.push_str(&format!("## 📝 Description\n\n{}\n\n", desc));
        out.push_str(&format!("**Downloads:** `{}`\n", downloads));
        if !repo.is_empty() { out.push_str(&format!("**Repository:** [{}]({})\n", repo, repo)); }
        out.push_str(&format!("\n**Add to Cargo.toml:**\n\n```toml\n{} = \"{}\"\n```\n\n", name, version));
        out.push_str(&format!("{}\n\n`{}` · #crates", tg_footer("crates.io", "crates"), now));
        return Ok(out);
    }
    Ok(format!("crate not found: **{pkg}**"))
}

async fn fetch_stackoverflow(query: &str) -> Result<String> {
    let url = format!("https://api.stackexchange.com/2.3/search?order=desc&sort=activity&intitle={}&site=stackoverflow&pagesize=5", urlencoding::encode(query));
    let resp: serde_json::Value = HTTP.get(&url).send().await?.json().await?;
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    if let Some(items) = resp["items"].as_array() {
        if items.is_empty() { return Ok(format!("No results for **{query}**")); }
        let total = items.len();
        let _total_answers: i64 = items.iter().map(|i| i["answer_count"].as_i64().unwrap_or(0)).sum();
        let avg_score: f64 = items.iter().map(|i| i["score"].as_i64().unwrap_or(0) as f64).sum::<f64>() / total as f64;
        let mut out = format!("{}\n\n", tg_header("📖", "Stack Overflow", query));
        out.push_str(&format!("**Query:** `{}` · **Results:** {} · **Avg Score:** {:.0}\n\n", query, total, avg_score));
        out.push_str("## 📊 Top Results\n\n");
        for item in items {
            let title = item["title"].as_str().unwrap_or("?");
            let link = item["link"].as_str().unwrap_or("?");
            let score = item["score"].as_i64().unwrap_or(0);
            let answers = item["answer_count"].as_i64().unwrap_or(0);
            let accepted = if item["is_answered"].as_bool().unwrap_or(false) { " ✅" } else { "" };
            out.push_str(&format!("**[{}]({})**\n   ⬆ {} · 💬 {} answers{}\n\n", title, link, score, answers, accepted));
        }
        out.push_str(&format!("{}\n\n`{}` · #stackoverflow", tg_footer("stackoverflow.com", "stackoverflow"), now));
        return Ok(out);
    }
    Ok(format!("stackoverflow err for *{query}*"))
}

// === NEW COMMANDS: Dev Tools ===

async fn fetch_docker(query: &str) -> Result<String> {
    let q = query.trim();
    if q.is_empty() { return Ok(format!("{}\n\n_Usage:_ `/docker <image>` — e.g. `nginx`, `postgres`, `redis`\n\n{}", tg_header("🐳", "Docker Hub", "Images"), tg_footer("hub.docker.com", "docker"))); }
    let url = format!("https://hub.docker.com/v2/search/repositories/?query={}&page_size=5", urlencoding::encode(q));
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    let results = v["results"].as_array().ok_or_else(|| anyhow::anyhow!("no results"))?;
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    if results.is_empty() { return Ok(format!("{}\n\n⚠️ _No images for `{}`_\n\n> Try: [hub.docker.com](https://hub.docker.com/search?q={})\n\n{}\n\n`{}` · #docker", tg_header("🐳", "Docker Hub", q), q, urlencoding::encode(q), tg_footer("hub.docker.com", "docker"), now)); }
    let mut out = format!("{}\n\n", tg_header("🐳", "Docker Hub", q));
    out.push_str(&format!("**Query:** `{}` · **Results:** {}\n\n", q, results.len()));
    out.push_str("## 🐳 Top Images\n\n");
    for (i, item) in results.iter().take(5).enumerate() {
        let name = item["repo_name"].as_str().unwrap_or("?");
        let desc = item["short_description"].as_str().unwrap_or("").chars().take(100).collect::<String>();
        let stars = item["star_count"].as_i64().unwrap_or(0);
        let pulls = item["pull_count"].as_i64().unwrap_or(0);
        let official = item["is_official"].as_bool().unwrap_or(false);
        let verified = item["is_verified"].as_bool().unwrap_or(false);
        let badge = if official { " ⭐ Official" } else if verified { " ✅ Verified" } else { "" };
        let pulls_str = if pulls >= 1_000_000 { format!("{}M", pulls / 1_000_000) } else if pulls >= 1_000 { format!("{}K", pulls / 1_000) } else { pulls.to_string() };
        out.push_str(&format!("**{}.** [`{}`](https://hub.docker.com/r/{}){}\n   ⭐ `{}` · ⬇️ `{} pulls`\n   📝 {}\n\n", i+1, name, name, badge, stars, pulls_str, desc));
    }
    out.push_str(&format!("{}\n\n`{}` · #docker #dev", tg_footer("hub.docker.com", "docker"), now));
    Ok(out)
}

// === NEW COMMANDS: Weather ===

async fn fetch_airquality(loc: &str) -> Result<String> {
    let loc = if loc.trim().is_empty() { "Thousand Oaks, CA" } else { loc };
    let url = format!("https://api.waqi.info/feed/{}/?token=demo", urlencoding::encode(loc));
    let resp: serde_json::Value = HTTP.get(&url).send().await?.json().await?;
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    if resp["status"].as_str() == Some("ok") {
        let data = &resp["data"];
        let aqi = data["aqi"].as_i64().unwrap_or(0);
        let city = data["city"]["name"].as_str().unwrap_or("?");
        let dominant = data["dominentpol"].as_str().unwrap_or("?");
        let level = if aqi <= 50 { "Good 🟢" } else if aqi <= 100 { "Moderate 🟡" } else if aqi <= 150 { "Unhealthy for Sensitive 🟠" } else if aqi <= 200 { "Unhealthy 🔴" } else if aqi <= 300 { "Very Unhealthy 🟣" } else { "Hazardous ⚫" };
        let mut out = format!("{}\n\n", tg_header("🌬️", "Air Quality", city));
        out.push_str(&format!("**City:** `{}` · **Station:** `{}`\n\n", city, loc));
        out.push_str(&format!("## 📊 AQI Report\n\n"));
        out.push_str("| Metric | Value |\n|---|---|\n");
        out.push_str(&format!("| AQI | **{}** |\n", aqi));
        out.push_str(&format!("| Level | {} |\n", level));
        out.push_str(&format!("| Dominant | `{}` |\n", dominant));
        out.push_str(&format!("| Updated | `{}` |\n\n", now));
        out.push_str(&format!("**Health Advice:**\n"));
        if aqi <= 50 { out.push_str("✅ Excellent air quality. Perfect for outdoor activities.\n"); }
        else if aqi <= 100 { out.push_str("⚠️ Acceptable. Unusually sensitive people should reduce prolonged outdoor exertion.\n"); }
        else if aqi <= 150 { out.push_str("⚠️ Sensitive groups may experience health effects. Limit prolonged outdoor exertion.\n"); }
        else if aqi <= 200 { out.push_str("🔴 Everyone may begin to experience health effects. Avoid prolonged outdoor exertion.\n"); }
        else { out.push_str("🟣 Health alert: everyone may experience more serious health effects.\n"); }
        out.push_str(&format!("\n🔗 [Detailed forecast](https://aqicn.org/city/{})\n\n", urlencoding::encode(loc)));
        out.push_str(&format!("{}\n\n`{}` · #airquality", tg_footer("aqicn.org", "airquality"), now));
        return Ok(out);
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
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    if resp["status"].as_str() == Some("OK") {
        let results = &resp["results"];
        let sunrise = results["sunrise"].as_str().unwrap_or("?");
        let sunset = results["sunset"].as_str().unwrap_or("?");
        let dawn = results["civil_twilight_begin"].as_str().unwrap_or("?");
        let dusk = results["civil_twilight_end"].as_str().unwrap_or("?");
        let day_length = results["day_length"].as_i64().unwrap_or(0);
        let hours = day_length / 3600;
        let mins = (day_length % 3600) / 60;
        let mut out = format!("{}\n\n", tg_header("🌅", "Sunrise/Sunset", &format!("({},{})", lat, lon)));
        out.push_str(&format!("**Location:** `{}, {}`\n\n", lat, lon));
        out.push_str("## ⏰ Sun Times\n\n");
        out.push_str("| Event | Time |\n|---|---|\n");
        out.push_str(&format!("| 🌅 Dawn | `{}` |\n", dawn));
        out.push_str(&format!("| ☀️ Sunrise | `{}` |\n", sunrise));
        out.push_str(&format!("| 🌇 Sunset | `{}` |\n", sunset));
        out.push_str(&format!("| 🌆 Dusk | `{}` |\n", dusk));
        out.push_str(&format!("| ☀️ Day Length | **{}h {}m** |\n\n", hours, mins));
        out.push_str(&format!("{}\n\n`{}` · #sunrise", tg_footer("sunrise-sunset.org", "sunrise"), now));
        return Ok(out);
    }
    Ok(format!("sunrise data unavailable for *{loc}*"))
}

// === NEW COMMANDS: Learn ===

fn eval_math(expr: &str) -> String {
    let cleaned: String = expr.chars().filter(|c| c.is_ascii_digit() || *c == '.' || *c == '+' || *c == '-' || *c == '*' || *c == '/' || *c == '(' || *c == ')' || *c == ' ').collect();
    let header = tg_header("🔢", "Math", expr);
    format!("{}\n\nEvaluate: {}\n\n{}", header, tg_code_block(&cleaned), tg_footer("math", "learn"))
}



async fn fetch_synonym(word: &str) -> Result<String> {
    // Fetch synonyms and antonyms in parallel
    let syn_resp: serde_json::Value = HTTP.get(format!("https://api.datamuse.com/words?rel_syn={}", urlencoding::encode(word))).send().await?.json().await?;
    let ant_resp: serde_json::Value = HTTP.get(format!("https://api.datamuse.com/words?rel_ant={}", urlencoding::encode(word))).send().await?.json().await?;
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let syns = syn_resp.as_array().map(|a| a.iter().take(15).filter_map(|w| w["word"].as_str()).map(|s| format!("`{}`", s)).collect::<Vec<String>>()).unwrap_or_default();
    let ants = ant_resp.as_array().map(|a| a.iter().take(10).filter_map(|w| w["word"].as_str()).map(|s| format!("`{}`", s)).collect::<Vec<String>>()).unwrap_or_default();
    let mut out = format!("{}\n\n", tg_header("📝", "Synonyms & Antonyms", word));
    out.push_str(&format!("**Word:** `{}`\n\n", word));
    if syns.is_empty() && ants.is_empty() {
        out.push_str("_No synonyms or antonyms found._\n\n");
        out.push_str(&format!("🔗 [Try Thesaurus.com](https://www.thesaurus.com/browse/{}?s=t)\n\n", urlencoding::encode(word)));
        out.push_str(&format!("{}\n\n`{}` · #synonym", tg_footer("datamuse.com", "synonym"), now));
        return Ok(out);
    }
    if !syns.is_empty() {
        out.push_str(&format!("## ✅ Synonyms ({})\n\n{}\n\n", syns.len(), syns.join(" · ")));
    }
    if !ants.is_empty() {
        out.push_str(&format!("## ❌ Antonyms ({})\n\n{}\n\n", ants.len(), ants.join(" · ")));
    }
    // Try to get definition for context
    let def_url = format!("https://api.datamuse.com/words?sp={}&md=d", urlencoding::encode(word));
    if let Ok(dv) = HTTP.get(&def_url).send().await {
        if let Ok(dj) = dv.json::<serde_json::Value>().await {
            if let Some(arr) = dj.as_array() {
                if let Some(item) = arr.first() {
                    if let Some(defs) = item["defs"].as_array() {
                        if let Some(first_def) = defs.first().and_then(|d| d.as_str()) {
                            out.push_str(&format!("## 📖 Definition\n\n{}\n\n", first_def));
                        }
                    }
                }
            }
        }
    }
    out.push_str(&format!("{}\n\n`{}` · #synonym", tg_footer("datamuse.com", "synonym"), now));
    Ok(out)
}

async fn fetch_philosophy_quote() -> Result<String> {
    // Primary: quotable.io (full quotes with author, tags, length)
    let api = async {
        let v: serde_json::Value = HTTP.get("https://api.quotable.io/quotes/random?limit=1&minLength=80")
            .timeout(std::time::Duration::from_secs(5))
            .send().await?.json().await?;
        let arr = v.as_array().ok_or_else(|| anyhow::anyhow!("not array"))?;
        let item = arr.first().ok_or_else(|| anyhow::anyhow!("empty"))?;
        let text = item["content"].as_str().ok_or_else(|| anyhow::anyhow!("no content"))?;
        let author = item["author"].as_str().unwrap_or("Unknown");
        let tags = item["tags"].as_array().map(|a| a.iter().filter_map(|t| t.as_str().map(|s| s.to_string())).collect::<Vec<String>>()).unwrap_or_default();
        Ok::<(String, String, Vec<String>), anyhow::Error>((text.to_string(), author.to_string(), tags))
    }.await;
    let (quote, author, tags) = match api {
        Ok((q, a, t)) if !q.is_empty() => (q, a, t),
        _ => {
            // Fallback: ZenQuotes (longer passages)
            let fb = async {
                let v: serde_json::Value = HTTP.get("https://zenquotes.io/api/random")
                    .timeout(std::time::Duration::from_secs(5))
                    .send().await?.json().await?;
                let arr = v.as_array().ok_or_else(|| anyhow::anyhow!("not array"))?;
                let item = arr.first().ok_or_else(|| anyhow::anyhow!("empty"))?;
                let text = item["q"].as_str().ok_or_else(|| anyhow::anyhow!("no q"))?;
                let author = item["a"].as_str().unwrap_or("Unknown");
                Ok::<(String, String, Vec<String>), anyhow::Error>((text.to_string(), author.to_string(), vec!["philosophy".into()]))
            }.await;
            match fb {
                Ok((q, a, t)) => (q, a, t),
                _ => {
                    let quotes = [
                        ("The only true wisdom is in knowing you know nothing.", "Socrates", vec!["wisdom"]),
                        ("Unexamined life is not worth living.", "Socrates", vec!["existentialism"]),
                        ("I think, therefore I am.", "Rene Descartes", vec!["metaphysics"]),
                        ("Man is condemned to be free.", "Jean-Paul Sartre", vec!["existentialism"]),
                        ("One cannot step twice into the same river.", "Heraclitus", vec!["metaphysics"]),
                        ("Life can only be understood backwards; but it must be lived forwards.", "Soren Kierkegaard", vec!["existentialism"]),
                        ("The owl of Minerva spreads its wings only with the falling of the dusk.", "G.W.F. Hegel", vec!["epistemology"]),
                        ("Happiness is not an ideal of reason but of imagination.", "Immanuel Kant", vec!["ethics"]),
                        ("God is dead. And we have killed him.", "Friedrich Nietzsche", vec!["nihilism"]),
                        ("No man ever steps in the same river twice, for it is not the same river and he is not the same man.", "Heraclitus", vec!["metaphysics"]),
                    ];
                    let idx = (chrono::Utc::now().timestamp() as usize) % quotes.len();
                    let (q, a, ref t) = quotes[idx];
                    (q.to_string(), a.to_string(), t.iter().map(|s| s.to_string()).collect())
                }
            }
        }
    };
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let tag_str = if tags.is_empty() { String::new() } else { format!(" · `{}`", tags.join("`, `")) };
    let char_count = quote.len();
    let word_count = quote.split_whitespace().count();
    let mut out = format!("{}\n\n", tg_header("📚", "Philosophy", &author));
    out.push_str(&format!("## 💭 Passage\n\n> _\"{}\"_\n\n", quote));
    out.push_str(&format!("— **{}**{}\n\n", author, tag_str));
    out.push_str(&format!("| Stat | Value |\n|---|---|\n| Characters | `{}` |\n| Words | `{}` |\n\n", char_count, word_count));
    // Try to get a short bio from Wikipedia
    let wiki_url = format!("https://en.wikipedia.org/api/rest_v1/page/summary/{}", urlencoding::encode(&author));
    if let Ok(wv) = HTTP.get(&wiki_url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(5)).send().await {
        if let Ok(wj) = wv.json::<serde_json::Value>().await {
            if let Some(extract) = wj["extract"].as_str() {
                let short_bio = extract.chars().take(200).collect::<String>();
                out.push_str(&format!("## 🧑 About the Author\n\n{}\n\n", short_bio));
            }
        }
    }
    out.push_str(&format!("{}\n\n`{}` · #philosophy #learn", tg_footer("quotable.io", "learn"), now));
    Ok(out)
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
    let products = v["products"].as_array();
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
        let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
        // Fallback: common food nutrition data
        let foods = [
            ("apple", "1 medium (182g)", "95 kcal", "0.3g", "0.2g", "25.1g", "19g", "4.4g", "a", "Rich in fiber, vitamin C, and antioxidants. May reduce risk of heart disease."),
            ("banana", "1 medium (118g)", "105 kcal", "1.3g", "0.4g", "27g", "14g", "3.1g", "a", "High in potassium, vitamin B6, and fiber. Great pre-workout snack."),
            ("orange", "1 medium (131g)", "62 kcal", "1.2g", "0.2g", "15.4g", "12g", "3.1g", "a", "Excellent source of vitamin C. Contains folate and thiamine."),
            ("chicken breast", "100g cooked", "165 kcal", "31g", "3.6g", "0g", "0g", "0g", "a", "Lean protein source. Rich in B vitamins and selenium."),
            ("rice", "1 cup cooked (158g)", "206 kcal", "4.3g", "0.4g", "44.5g", "0.6g", "0.6g", "a", "Staple grain worldwide. Good source of manganese and energy."),
            ("egg", "1 large (50g)", "72 kcal", "6.3g", "4.8g", "0.4g", "0.4g", "0g", "a", "Complete protein. Rich in choline and vitamin D."),
        ];
        let q_lower = q.to_lowercase();
        if let Some((name, serving, kcal, prot, fat, carbs, sugar, fiber, score, desc)) = foods.iter().find(|(name, _, _, _, _, _, _, _, _, _)| q_lower.contains(name)) {
            let mut out = format!("{}\n\n", tg_header("🥗", "Nutrition", name));
            out.push_str(&format!("**Food:** `{}` · **Serving:** `{}`\n\n", name, serving));
            out.push_str("## 📊 Nutrition Facts (per serving)\n\n");
            out.push_str("| Nutrient | Amount |\n|---|---|\n");
            out.push_str(&format!("| Calories | `{}` |\n", kcal));
            out.push_str(&format!("| Protein | `{}` |\n", prot));
            out.push_str(&format!("| Fat | `{}` |\n", fat));
            out.push_str(&format!("| Carbs | `{}` |\n", carbs));
            out.push_str(&format!("| Sugar | `{}` |\n", sugar));
            out.push_str(&format!("| Fiber | `{}` |\n", fiber));
            out.push_str(&format!("| Nutri-Score | `{} 🟢` |\n\n", score));
            out.push_str(&format!("## 💡 Fun Fact\n\n{}\n\n", desc));
            out.push_str(&format!("🔗 [Open Food Facts](https://world.openfoodfacts.org/cgi/search?search_terms={})\n\n", urlencoding::encode(name)));
            out.push_str(&format!("{}\n\n`{}` · #food", tg_footer("openfoodfacts.org", "food"), now));
            return Ok(out);
        }
        return Ok(format!("{}\n\n_No foods found for `{}`._\n\n> Try: `apple`, `banana`, `orange`, `chicken`, `rice`, `egg`\n> Or search by product name: `/food oreo`, `/food coca-cola`\n\n{}\n\n`{}` · #food", tg_header("🥗", "Nutrition", q), q, tg_footer("openfoodfacts.org", "food"), now));
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

fn create_flag(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let content = if args.trim().is_empty() { "_Flag for follow-up._" } else { args.trim() };
    let mut out = format!("{}\n\n", tg_header("🚩", "Flagged", &now));
    out.push_str(&format!("**Flagged:** `{}`\n\n", now));
    out.push_str(&format!("## 🚩 Content\n\n{}\n\n", content));
    out.push_str("## 📋 Follow-up\n\n");
    out.push_str("- [ ] Review flagged item\n");
    out.push_str("- [ ] Take action\n");
    out.push_str("- [ ] Archive when done\n\n");
    out.push_str("## ⏰ Priority\n\n");
    out.push_str("| Urgent | Important |\n|---|---|\n| ⬜ | ⬜ |\n\n");
    out.push_str(&format!("{}\n\n`{}` · #flag #inbox", tg_footer("memos", "flag"), now));
    out
}

fn create_archive(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let content = if args.trim().is_empty() { "_Archived memo._" } else { args.trim() };
    let mut out = format!("{}\n\n", tg_header("📦", "Archived", &now));
    out.push_str(&format!("**Archived:** `{}`\n\n", now));
    out.push_str(&format!("## 📦 Content\n\n{}\n\n", content));
    out.push_str("## 📋 Archive Info\n\n");
    out.push_str("| Field | Value |\n|---|---|\n");
    out.push_str(&format!("| Status | `archived` |\n"));
    out.push_str(&format!("| Date | `{}` |\n", now));
    out.push_str(&format!("| Tags | `#archive` |\n\n"));
    out.push_str(&format!("{}\n\n`{}` · #archive #inbox", tg_footer("memos", "archive"), now));
    out
}

fn create_move(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let bucket = parts.first().unwrap_or(&"");
    let content = parts.get(1).unwrap_or(&"");
    let word_count = content.split_whitespace().count();
    Md::new()
        .h2("↗️ Memo Moved")
        .blank()
        .pi("Destination", &format!("#{}", bucket))
        .pi("Time", &now)
        .pi("Words", &word_count.to_string())
        .blank()
        .push("## 📝 Content")
        .blank()
        .push(content)
        .blank()
        .push("## 📊 Move Details")
        .blank()
        .table(&["Property", "Value"], &[
            vec!["Bucket".into(), format!("#{}", bucket)],
            vec!["Time".into(), now.clone()],
            vec!["Words".into(), word_count.to_string()],
        ])
        .blank()
        .push("## 💡 Tips")
        .blank()
        .push(&format!("- Check `#{}` for new memos", bucket))
        .push("- Use `/archive` to clean up old memos")
        .blank()
        .push(&format!("{}\n\n`{}` · #move #inbox", tg_footer("memogram", "move"), now))
        .build()
}

async fn fetch_wind(loc: &str) -> Result<String> {
    let city = if loc.trim().is_empty() { "Thousand Oaks, CA" } else { loc };
    let url = format!("https://wttr.in/{}?format=j1", urlencoding::encode(city));
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    if let Some(arr) = v["current_condition"].as_array() {
        if let Some(c) = arr.first() {
            let speed = c["windspeedKmph"].as_str().unwrap_or("?");
            let dir = c["winddir16Point"].as_str().unwrap_or("?");
            let gust = c["WindGustKmph"].as_str().unwrap_or("?");
            let beaufort = c["beaufort"].as_str().unwrap_or("?");
            let mut out = format!("{}\n\n", tg_header("💨", "Wind", city));
            out.push_str(&format!("**City:** `{}`\n\n", city));
            out.push_str("## 💨 Wind Conditions\n\n");
            out.push_str("| Metric | Value |\n|---|---|\n");
            out.push_str(&format!("| Speed | `{} km/h` |\n", speed));
            out.push_str(&format!("| Direction | `{}` |\n", dir));
            out.push_str(&format!("| Gust | `{} km/h` |\n", gust));
            out.push_str(&format!("| Beaufort | `{}` |\n\n", beaufort));
            out.push_str(&format!("{}\n\n`{}` · #wind #weather", tg_footer("wttr.in", "wind"), now));
            return Ok(out);
        }
    }
    Ok(format!("{}\n\n⚠️ _No wind data for `{}`_\n\n{}", tg_header("💨", "Wind", city), city, tg_footer("wttr.in", "wind")))
}

async fn fetch_uv(loc: &str) -> Result<String> {
    let city = if loc.trim().is_empty() { "Thousand Oaks, CA" } else { loc };
    let url = format!("https://wttr.in/{}?format=j1", urlencoding::encode(city));
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    if let Some(arr) = v["current_condition"].as_array() {
        if let Some(c) = arr.first() {
            let uv = c["uvIndex"].as_str().unwrap_or("?");
            let uv_val = uv.parse::<i64>().unwrap_or(0);
            let level = if uv_val <= 2 { "Low 🟢" } else if uv_val <= 5 { "Moderate 🟡" } else if uv_val <= 7 { "High 🟠" } else if uv_val <= 10 { "Very High 🔴" } else { "Extreme 🟣" };
            let mut out = format!("{}\n\n", tg_header("☀️", "UV Index", city));
            out.push_str(&format!("**City:** `{}`\n\n", city));
            out.push_str("## ☀️ UV Report\n\n");
            out.push_str("| Metric | Value |\n|---|---|\n");
            out.push_str(&format!("| UV Index | **{}** |\n", uv));
            out.push_str(&format!("| Level | {} |\n\n", level));
            out.push_str("## 🛡️ Protection\n\n");
            if uv_val <= 2 { out.push_str("✅ Low risk. No protection needed.\n"); }
            else if uv_val <= 5 { out.push_str("⚠️ Moderate. Wear sunscreen, hat.\n"); }
            else if uv_val <= 7 { out.push_str("🔴 High. Seek shade 10am-4pm.\n"); }
            else { out.push_str("🟣 Very High. Avoid sun 10am-4pm.\n"); }
            out.push_str(&format!("\n{}\n\n`{}` · #uv #weather", tg_footer("wttr.in", "uv"), now));
            return Ok(out);
        }
    }
    Ok(format!("{}\n\n⚠️ _No UV data for `{}`_\n\n{}", tg_header("☀️", "UV Index", city), city, tg_footer("wttr.in", "uv")))
}

async fn fetch_pollen(loc: &str) -> Result<String> {
    let city = if loc.trim().is_empty() { "Thousand Oaks, CA" } else { loc };
    let now = Local::now().format("%Y-%m-%d").to_string();
    let mut out = format!("{}\n\n", tg_header("🌿", "Pollen", city));
    out.push_str(&format!("**City:** `{}` · **Date:** `{}`\n\n", city, now));
    out.push_str("## 🌿 Pollen Count\n\n");
    out.push_str("| Type | Level |\n|---|---|\n");
    out.push_str(&format!("| Trees | — |\n| Grass | — |\n| Weeds | — |\n| Mold | — |\n\n"));
    out.push_str("## 🛡️ Tips\n\n");
    out.push_str("- Check local pollen forecast\n");
    out.push_str("- Keep windows closed during high counts\n");
    out.push_str("- Shower after outdoor activities\n\n");
    out.push_str(&format!("{}\n\n`{}` · #pollen #weather", tg_footer("pollen.com", "pollen"), now));
    Ok(out)
}

async fn fetch_moon(_loc: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d").to_string();
    let mut out = format!("{}\n\n", tg_header("🌙", "Moon Phase", &now));
    out.push_str(&format!("**Date:** `{}`\n\n", now));
    out.push_str("## 🌙 Moon Phase\n\n");
    out.push_str("| Phase | Illumination |\n|---|---|\n");
    out.push_str(&format!("| Current | — |\n| Illumination | — |\n| Age | — |\n\n"));
    out.push_str("## 📅 Upcoming Phases\n\n");
    out.push_str("| Phase | Date |\n|---|---|\n");
    out.push_str(&format!("| 🌑 New | — |\n| 🌓 First Quarter | — |\n| 🌕 Full | — |\n| 🌗 Last Quarter | — |\n\n"));
    out.push_str(&format!("{}\n\n`{}` · #moon #weather", tg_footer("mooncalc.org", "moon"), now));
    Ok(out)
}

async fn fetch_tide(loc: &str) -> Result<String> {
    let city = if loc.trim().is_empty() { "Thousand Oaks, CA" } else { loc };
    let now = Local::now().format("%Y-%m-%d").to_string();
    let mut out = format!("{}\n\n", tg_header("🌊", "Tides", city));
    out.push_str(&format!("**Location:** `{}` · **Date:** `{}`\n\n", city, now));
    out.push_str("## 🌊 Tide Times\n\n");
    out.push_str("| Time | Type | Height |\n|---|---|---|\n");
    out.push_str(&format!("| — | High | — ft |\n| — | Low | — ft |\n| — | High | — ft |\n| — | Low | — ft |\n\n"));
    out.push_str("## 📊 Tide Chart\n\n");
    out.push_str("```\nHigh  ▁▂▃▄▅▆▇█▇▆▅▄▃▂▁ Low  ▁▂▃▄▅▆▇█▇▆▅▄▃▂▁\n```\n\n");
    out.push_str(&format!("{}\n\n`{}` · #tide #weather", tg_footer("tidesandcurrents.noaa.gov", "tide"), now));
    Ok(out)
}

async fn fetch_snow(loc: &str) -> Result<String> {
    let city = if loc.trim().is_empty() { "Thousand Oaks, CA" } else { loc };
    let url = format!("https://wttr.in/{}?format=j1", urlencoding::encode(city));
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    if let Some(arr) = v["current_condition"].as_array() {
        if let Some(c) = arr.first() {
            let precip = c["precipMM"].as_str().unwrap_or("0");
            let desc = c["weatherDesc"][0]["value"].as_str().unwrap_or("?");
            let temp = c["temp_C"].as_str().unwrap_or("?");
            let mut out = format!("{}\n\n", tg_header("❄️", "Snow Report", city));
            out.push_str(&format!("**City:** `{}`\n\n", city));
            out.push_str("## ❄️ Conditions\n\n");
            out.push_str("| Metric | Value |\n|---|---|\n");
            out.push_str(&format!("| Conditions | `{}` |\n", desc));
            out.push_str(&format!("| Temperature | `{}°C` |\n", temp));
            out.push_str(&format!("| Precipitation | `{} mm` |\n\n", precip));
            out.push_str("## 🎿 Forecast\n\n");
            out.push_str("| Day | Snow | Temp |\n|---|---|---|\n");
            out.push_str(&format!("| Today | — | — |\n\n"));
            out.push_str(&format!("{}\n\n`{}` · #snow #weather", tg_footer("wttr.in", "snow"), now));
            return Ok(out);
        }
    }
    Ok(format!("{}\n\n⚠️ _No snow data for `{}`_\n\n{}", tg_header("❄️", "Snow Report", city), city, tg_footer("wttr.in", "snow")))
}

// === WELLNESS COMMANDS ===

async fn fetch_hustle(skill: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let skill_lower = skill.trim().to_lowercase();

    // Curated side hustle database indexed by skill/keyword
    let hustles: Vec<(&str, &str, &str, &str, &str, &str)> = vec![
        // Tech
        ("python", "Freelance Python Automation", "Build scripts, web scrapers, data pipelines for clients", "$30-100/hr", "Medium", "upwork.com, fiverr.com"),
        ("javascript", "Contract Web Development", "Landing pages, Shopify tweaks, WordPress sites", "$25-80/hr", "Medium", "upwork.com, toptal.com"),
        ("rust", "Systems Contracting", "CLI tools, embedded firmware, performance-critical services", "$60-150/hr", "Hard", "github.com/jobs, LinkedIn"),
        ("go", "Backend/API Freelance", "Microservices, DevOps tooling, cloud functions", "$40-120/hr", "Medium", "upwork.com, arc.dev"),
        ("react", "Frontend Freelance", "Dashboards, SPAs, component libraries", "$30-90/hr", "Medium", "upwork.com, gun.io"),
        ("swift", "iOS App Development", "MVPs, SwiftUI prototypes, App Store releases", "$50-150/hr", "Hard", "toptal.com, upwork.com"),
        ("flutter", "Cross-platform Mobile", "Ship iOS + Android from one codebase", "$35-100/hr", "Medium", "upwork.com, flutterjobs.com"),
        ("devops", "DevOps Consulting", "CI/CD, Docker, K8s, cloud migrations", "$50-200/hr", "Hard", "toptal.com, arc.dev"),
        ("machine learning", "ML Consulting", "Model training, data pipelines, MLOps", "$60-200/hr", "Hard", "kaggle.com, upwork.com"),
        ("data", "Data Analytics Freelance", "Dashboards, ETL, Excel automation", "$25-75/hr", "Medium", "upwork.com, toptal.com"),
        // Creative
        ("design", "UI/UX Design Freelance", "Figma prototypes, brand kits, design systems", "$30-100/hr", "Medium", "dribbble.com, upwork.com"),
        ("video", "Video Editing Services", "YouTube edits, reels, ad creatives", "$20-75/hr", "Easy", "fiverr.com, upwork.com"),
        ("photo", "Photography Side Gig", "Events, product shots, real estate", "$50-500/event", "Easy", "thumbtack.com, yelp"),
        ("writing", "Technical Writing", "Docs, blog posts, API guides", "$0.10-0.50/word", "Easy", "upwork.com, technicalwritinghq.com"),
        ("copywriting", "Copywriting Services", "Landing pages, email sequences, ad copy", "$25-100/hr", "Medium", "copyblogger.com, upwork.com"),
        // Business
        ("marketing", "Digital Marketing Consulting", "SEO, paid ads, social media strategy", "$30-100/hr", "Medium", "upwork.com, LinkedIn"),
        ("seo", "SEO Freelancing", "Audits, keyword research, link building", "$25-80/hr", "Easy", "upwork.com, semrush.com"),
        ("finance", "Financial Modeling / Consulting", "Excel models, pitch decks, CFO-as-a-service", "$50-200/hr", "Hard", "toptal.com, cruwix.com"),
        ("account", "Bookkeeping Side Gig", "QuickBooks, Xero, tax prep for small biz", "$20-50/hr", "Easy", "bookkeeper.com, upwork.com"),
        ("legal", "Contract Review Freelance", "NDAs, SOWs, employment agreements", "$50-150/hr", "Hard", "lawtradr.com, upwork.com"),
        // General
        ("tutor", "Online Tutoring", "Math, science, language tutoring", "$15-60/hr", "Easy", "wyzant.com, tutor.com"),
        ("translate", "Translation Services", "Document, website, video translation", "$0.05-0.20/word", "Easy", "upwork.com, proz.com"),
        ("music", "Music Lessons / Production", "Instrument tutoring, beat making, mixing", "$20-80/hr", "Easy", "takelessons.com, fiverr.com"),
        ("fitness", "Online Coaching", "Personalized workout + meal plans", "$50-200/mo per client", "Medium", "trainerize.com, Instagram"),
        ("cook", "Private Chef / Meal Prep", "Weekly meal prep, event catering", "$200-500/event", "Medium", "thumbtack.com, yelp"),
    ];

    // Find matching hustles by skill keyword
    let matched: Vec<_> = if skill_lower.is_empty() {
        // Random general picks when no skill given
        hustles.iter().choose_multiple(&mut rand::rng(), 5).into_iter().cloned().collect()
    } else {
        let mut matches: Vec<_> = hustles.iter()
            .filter(|(k, _, _, _, _, _)| skill_lower.contains(k) || k.contains(&skill_lower))
            .cloned()
            .collect();
        if matches.is_empty() {
            // Fallback: fuzzy match on description
            matches = hustles.iter()
                .filter(|(_, title, desc, _, _, _)| {
                    let combined = format!("{} {}", title.to_lowercase(), desc.to_lowercase());
                    skill_lower.split_whitespace().any(|w| combined.contains(w))
                })
                .cloned()
                .collect();
        }
        if matches.is_empty() {
            // Still nothing — give general advice
            let mut out = format!("{}\n\n", tg_header("💰", "Side Hustle Ideas", skill));
            out.push_str(&format!("No specific matches for **{}**, but here are universal high-ROI hustles:\n\n", skill));
            out.push_str("| Hustle | Why | Effort |\n|---|---|---|\n");
            out.push_str("| Freelance consulting | Package what you already know | Low |\n");
            out.push_str("| Digital products | Templates, courses, presets | Medium |\n");
            out.push_str("| Open source + sponsorship | Build in public, get sponsors | High |\n\n");
            out.push_str("💡 **Tip:** Tell me your specific skill (e.g. `/hustle python` or `/hustle design`) for tailored ideas.\n\n");
            out.push_str(&format!("{}\n\n`{}` · #hustle #money", tg_footer("memogram-rs", "hustle"), now));
            return Ok(out);
        }
        matches
    };

    let mut out = format!("{}\n\n", tg_header("💰", "Side Hustle Ideas", skill));
    out.push_str(&format!("**{} matches** for _{}_\n\n", matched.len(), if skill.is_empty() { "general picks" } else { skill }));

    for (i, (_, title, desc, pay, difficulty, platforms)) in matched.iter().enumerate() {
        let diff_emoji = match *difficulty {
            "Easy" => "🟢",
            "Medium" => "🟡",
            "Hard" => "🔴",
            _ => "⚪",
        };
        out.push_str(&format!("**{}. {}**\n", i + 1, title));
        out.push_str(&format!("{}\n", desc));
        out.push_str(&format!("   💵 {} · {} {} · 📍 {}\n\n", pay, diff_emoji, difficulty, platforms));
    }

    out.push_str("## 🚀 Next Steps\n\n");
    out.push_str("1. Pick one that matches your current skills\n");
    out.push_str("2. Create a profile on the listed platform\n");
    out.push_str("3. Start with a small gig to build reviews\n");
    out.push_str("4. Use `/brief <platform>` to learn how to succeed there\n\n");
    out.push_str(&format!("{}\n\n`{}` · #hustle #money", tg_footer("memogram-rs", "hustle"), now));
    Ok(out)
}

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

// === NEW COMMANDS ===

async fn fetch_digest(memos_url: &str, token: &str) -> Result<String> {
    let now = Local::now();
    let today = now.format("%Y-%m-%d").to_string();
    let v: serde_json::Value = HTTP.get(format!("{memos_url}/api/v1/memos?pageSize=50"))
        .header("Authorization", format!("Bearer {token}")).send().await?.json().await?;
    let memos = v["memos"].as_array().ok_or_else(|| anyhow::anyhow!("no memos"))?;
    let today_memos: Vec<&serde_json::Value> = memos.iter().filter(|m| {
        m["createTime"].as_str().map(|t| t.starts_with(&today)).unwrap_or(false)
    }).collect();
    let total_memos = memos.len();
    let count = today_memos.len();
    let mut out = format!("{}\n\n", tg_header("📋", "Daily Digest", &today));
    if today_memos.is_empty() {
        out.push_str("**No memos today yet.** Start writing to build your streak!\n\n");
        out.push_str(&format!("> {} total memos in your collection\n\n", total_memos));
    } else {
        out.push_str(&format!("**{} memos** created today\n\n", count));
        out.push_str("| Time | Preview | Tags |\n|---|---|---|\n");
        for m in &today_memos {
            let time = m["createTime"].as_str().unwrap_or("");
            let hour = if time.len() >= 16 { &time[11..16] } else { "?" };
            let content = m["content"].as_str().unwrap_or("");
            let preview: String = content.chars().take(60).collect();
            let tags: Vec<String> = m["tags"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).map(|s| format!("`#{}`", s)).collect()).unwrap_or_default();
            let tag_str = if tags.is_empty() { String::new() } else { format!(" {}", tags.join(" ")) };
            out.push_str(&format!("| {} | {} |{} |\n", hour, preview.replace('\n', " ").replace('|', "\\|"), tag_str));
        }
    }
    // Word count estimate
    let total_chars: usize = today_memos.iter().filter_map(|m| m["content"].as_str()).map(|c| c.len()).sum();
    out.push_str(&format!("\n📊 **Stats:** {} memos, ~{} words today\n", count, total_chars / 5));
    out.push_str(&format!("{}\n\n`{}` · #digest #daily", tg_footer("memogram-rs", "digest"), now.format("%Y-%m-%d %H:%M")));
    Ok(out)
}

fn create_income(args: &str) -> String {
    let parts: Vec<&str> = args.splitn(3, ' ').collect();
    let source = parts.first().filter(|s| !s.is_empty()).copied().unwrap_or("unknown");
    let amount = parts.get(1).unwrap_or(&"0");
    let note = parts.get(2).unwrap_or(&"");
    let date = Local::now().format("%Y-%m-%d").to_string();
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let mut out = format!("# 💰 Income — `{}`\n\n**Date:** `{}` · **Source:** `{}` · **Amount:** `${}`\n\n", date, date, source, amount);
    if !note.is_empty() {
        out.push_str(&format!("**Note:** {}\n\n", note));
    }
    out.push_str("## 📊 Income Log\n\n");
    out.push_str("| Date | Source | Amount | Note |\n|---|---|---|---|\n");
    out.push_str(&format!("| {} | {} | ${} | {} |\n", date, source, amount, note));
    out.push_str("\n> _Tip: Use `/hustle <skill>` to find new income sources._\n\n");
    out.push_str(&format!("{}\n\n`{}` · #income #money #memogram-rs", tg_header("💰", "Income", source), now));
    out
}

async fn fetch_youtube(url: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    if url.trim().is_empty() {
        return Ok("usage: `/youtube <url>`".into());
    }
    // Try to extract video ID
    let video_id = if url.contains("youtu.be/") {
        url.split("youtu.be/").nth(1).unwrap_or("").split('?').next().unwrap_or("")
    } else if url.contains("v=") {
        url.split("v=").nth(1).unwrap_or("").split('&').next().unwrap_or("")
    } else {
        ""
    };
    if video_id.is_empty() {
        return Ok("⚠️ Could not extract video ID from URL.".into());
    }
    // Use Invidious API for video info
    let api_url = format!("https://vid.puffyan.us/api/v1/videos/{}", video_id);
    let v: serde_json::Value = match tokio::time::timeout(std::time::Duration::from_secs(8), HTTP.get(&api_url).header("User-Agent", "memogram-rs").send()).await {
        Ok(Ok(r)) => match r.json::<serde_json::Value>().await { Ok(j) => j, Err(_) => serde_json::Value::Null },
        _ => serde_json::Value::Null,
    };
    let title = v["title"].as_str().unwrap_or("Unknown");
    let author = v["author"].as_str().unwrap_or("Unknown");
    let length = v["lengthSeconds"].as_u64().unwrap_or(0);
    let mins = length / 60;
    let secs = length % 60;
    let published = v["publishedText"].as_str().unwrap_or("");
    let desc = v["description"].as_str().unwrap_or("").chars().take(500).collect::<String>();
    let mut out = format!("{}\n\n", tg_header("🎬", "YouTube", title));
    out.push_str(&format!("**Title:** {}\n**Channel:** {} · **Length:** {}:{:02}\n**Published:** {}\n\n", title, author, mins, secs, published));
    if !desc.is_empty() {
        out.push_str(&format!("## 📝 Description\n\n{}\n\n", desc));
    }
    out.push_str(&format!("🔗 [Watch](https://youtube.com/watch?v={})\n\n", video_id));
    out.push_str(&format!("{}\n\n`{}` · #youtube #learn #memogram-rs", tg_footer("invidious", "youtube"), now));
    Ok(out)
}

async fn fetch_learn(topic: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let topic = topic.trim().to_string();
    if topic.is_empty() {
        return Ok("usage: `/learn <topic>` — get a structured learning overview".into());
    }
    let mut out = format!("{}\n\n", tg_header("🎓", "Learn", &topic));
    out.push_str(&format!("**Topic:** `{}` · **Started:** `{}`\n\n", topic, now));

    // 1. Wikipedia summary
    let wiki_url = format!("https://en.wikipedia.org/api/rest_v1/page/summary/{}", urlencoding::encode(&topic));
    let wiki: serde_json::Value = match tokio::time::timeout(std::time::Duration::from_secs(5), HTTP.get(&wiki_url).header("User-Agent", "memogram-rs").send()).await {
        Ok(Ok(r)) => r.json().await.unwrap_or(serde_json::Value::Null),
        _ => serde_json::Value::Null,
    };
    let extract = wiki["extract"].as_str().unwrap_or("");
    if !extract.is_empty() {
        out.push_str("## 📖 Overview\n\n");
        let summary = extract.chars().take(500).collect::<String>();
        out.push_str(&format!("> {}\n\n", summary));
        if let Some(url) = wiki["content_urls"]["desktop"]["page"].as_str() {
            out.push_str(&format!("🔗 [Wikipedia]({})\n\n", url));
        }
    }

    // 2. Prerequisites — search Wikipedia for related concepts
    let search_url = format!("https://en.wikipedia.org/api/rest_v1/page/related/{}", urlencoding::encode(&topic));
    let related: serde_json::Value = match tokio::time::timeout(std::time::Duration::from_secs(5), HTTP.get(&search_url).header("User-Agent", "memogram-rs").send()).await {
        Ok(Ok(r)) => r.json().await.unwrap_or(serde_json::Value::Null),
        _ => serde_json::Value::Null,
    };
    if let Some(pages) = related["pages"].as_array() {
        if !pages.is_empty() {
            out.push_str("## 🔗 Related Topics\n\n");
            for p in pages.iter().take(5) {
                let title = p["title"].as_str().unwrap_or("?");
                let desc = p["description"].as_str().unwrap_or("");
                let page_url = p["content_urls"]["desktop"]["page"].as_str().unwrap_or("#");
                out.push_str(&format!("- [**{}**]({}) — {}\n", title, page_url, desc));
            }
            out.push('\n');
        }
    }

    // 3. arXiv papers
    let arxiv_url = format!("http://export.arxiv.org/api/query?search_query=all:{}&max_results=3&sortBy=relevance", urlencoding::encode(&topic));
    let arxiv_xml = HTTP.get(&arxiv_url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.text().await.unwrap_or_default();
    // Simple XML parsing for title and link
    let mut arxiv_entries = Vec::new();
    let mut remaining = arxiv_xml.as_str();
    while let Some(entry_start) = remaining.find("<entry>") {
        remaining = &remaining[entry_start + 7..];
        if let Some(entry_end) = remaining.find("</entry>") {
            let entry = &remaining[..entry_end];
            let title = entry.split("<title>").nth(1).and_then(|s| s.split("</title>").next()).map(|s| s.trim().replace('\n', " ")).unwrap_or_default();
            let id = entry.split("<id>").nth(1).and_then(|s| s.split("</id>").next()).map(|s| s.trim()).unwrap_or("");
            let summary = entry.split("<summary>").nth(1).and_then(|s| s.split("</summary>").next()).map(|s| s.trim().chars().take(120).collect::<String>()).unwrap_or_default();
            if !title.is_empty() {
                arxiv_entries.push((title, id.to_string(), summary));
            }
            remaining = &remaining[entry_end..];
        } else { break; }
    }
    if !arxiv_entries.is_empty() {
        out.push_str("## 📄 Related Papers (arXiv)\n\n");
        for (i, (title, id, summary)) in arxiv_entries.iter().enumerate() {
            let arxiv_id = id.split("/abs/").last().unwrap_or(id);
            out.push_str(&format!("{}. [{}]({})\n", i + 1, title, id));
            if !summary.is_empty() {
                out.push_str(&format!("   _{}_\n\n", summary));
            }
        }
    }

    // 4. YouTube tutorials
    let yt_search = format!("{} {} tutorial", topic, topic);
    let yt_url = format!("https://vid.puffyan.us/api/v1/search?q={}&type=video&sort_by=relevance&page=1", urlencoding::encode(&yt_search));
    let yt: serde_json::Value = match tokio::time::timeout(std::time::Duration::from_secs(5), HTTP.get(&yt_url).header("User-Agent", "memogram-rs").send()).await {
        Ok(Ok(r)) => r.json().await.unwrap_or(serde_json::Value::Null),
        _ => serde_json::Value::Null,
    };
    if let Some(videos) = yt.as_array() {
        if !videos.is_empty() {
            out.push_str("## 🎬 Video Tutorials\n\n");
            for v in videos.iter().take(3) {
                let title = v["title"].as_str().unwrap_or("?");
                let author = v["author"].as_str().unwrap_or("?");
                let vid_id = v["videoId"].as_str().unwrap_or("");
                let length = v["lengthSeconds"].as_u64().unwrap_or(0);
                let mins = length / 60;
                let secs = length % 60;
                out.push_str(&format!("- [**{}**](https://youtube.com/watch?v={}) by {} · {}:{:02}\n", title, vid_id, author, mins, secs));
            }
            out.push('\n');
        }
    }

    // 5. Suggested learning path
    out.push_str("## 🗺️ Suggested Path\n\n");
    out.push_str("| Phase | Focus | Time |\n|---|---|---|\n");
    out.push_str(&format!("| 1 | Read the overview & related topics | 30 min |\n"));
    out.push_str(&format!("| 2 | Watch top video tutorial | 20 min |\n"));
    out.push_str(&format!("| 3 | Read 1-2 papers for depth | 1 hr |\n"));
    out.push_str(&format!("| 4 | Build something / take notes | 2 hr |\n"));
    out.push_str(&format!("| 5 | Review with `/search {}` (your own notes) | 15 min |\n\n", topic));

    // 6. Progress tracker
    out.push_str("## 📊 Progress\n\n");
    out.push_str("| Step | Status | Notes |\n|---|---|---|\n");
    out.push_str("| Overview read | ⬜ | |\n");
    out.push_str("| Videos watched | ⬜ | |\n");
    out.push_str("| Papers read | ⬜ | |\n");
    out.push_str("| Practice done | ⬜ | |\n");
    out.push_str("| Reviewed | ⬜ | |\n\n");

    out.push_str(&format!("{}\n\n`{}` · #learn #memogram-rs", tg_footer("wikipedia + arxiv + invidious", "learn"), now));
    Ok(out)
}

fn create_transcribe(text: &str) -> String {
    let date = Local::now().format("%Y-%m-%d").to_string();
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    if text.trim().is_empty() {
        return "usage: `/transcribe <text>` — paste voice-to-text output here".into();
    }
    let mut out = format!("# 🎤 Transcription — `{}`\n\n**Date:** `{}`\n\n## ✍️ Text\n\n{}\n\n", date, now, text);
    out.push_str("## 📊 Stats\n\n");
    let words = text.split_whitespace().count();
    let chars = text.len();
    out.push_str(&format!("| Metric | Value |\n|---|---|\n| Words | {} |\n| Characters | {} |\n| Reading time | ~{} min |\n\n", words, chars, (words / 200).max(1)));
    out.push_str("> _Edit this memo in Memos to clean up the transcription._\n\n");
    out.push_str(&format!("{}\n\n`{}` · #transcribe #inbox #memogram-rs", tg_header("🎤", "Transcription", &date), now));
    out
}

fn create_timestamp(args: &str) -> String {
    let now = Local::now();
    let args = args.trim();
    let mut out = format!("{}\n\n", tg_header("⏱️", "Timestamp", ""));

    if args.is_empty() {
        // No args: show current time conversions
        let epoch = now.timestamp();
        let utc = now.format("%Y-%m-%d %H:%M:%S UTC").to_string();
        let iso = now.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let unix_ms = epoch * 1000;
        let weekday = now.format("%A").to_string();
        let year_day = now.format("%j").to_string();
        out.push_str("## 🕐 Current Time\n\n");
        out.push_str("| Format | Value |\n|---|---|\n");
        out.push_str(&format!("| Unix | `{}` |\n", epoch));
        out.push_str(&format!("| Unix (ms) | `{}` |\n", unix_ms));
        out.push_str(&format!("| ISO 8601 | `{}` |\n", iso));
        out.push_str(&format!("| UTC | `{}` |\n", utc));
        out.push_str(&format!("| Day | {} |\n", weekday));
        out.push_str(&format!("| Day of Year | `{}` |\n\n", year_day));
        out.push_str("**Tip:** Pass a unix timestamp to convert it: `/timestamp 1700000000`\n\n");
    } else if let Ok(ts) = args.parse::<i64>() {
        // It's a unix timestamp — convert to human
        let ts = if ts > 1_000_000_000_000 { ts / 1000 } else { ts };
        let dt = chrono::DateTime::from_timestamp(ts, 0).unwrap_or_default();
        let local_dt = dt.with_timezone(&Local);
        let naive = dt.naive_utc();
        let now_naive = now.naive_utc();
        let diff = now_naive - naive;
        let days = diff.num_days();
        let hours = diff.num_hours();
        let abs_days = days.abs();
        let relative = if days > 0 {
            format!("{} days ago", abs_days)
        } else if days < 0 {
            format!("{} days from now", abs_days)
        } else {
            "now".to_string()
        };
        out.push_str(&format!("## 📅 Converted from `{}`\n\n", args));
        out.push_str("| Format | Value |\n|---|---|\n");
        out.push_str(&format!("| Unix | `{}` |\n", ts));
        out.push_str(&format!("| UTC | `{}` |\n", dt.format("%Y-%m-%d %H:%M:%S UTC")));
        out.push_str(&format!("| Local | `{}` |\n", local_dt.format("%Y-%m-%d %H:%M:%S %Z")));
        out.push_str(&format!("| ISO 8601 | `{}` |\n", dt.format("%Y-%m-%dT%H:%M:%SZ")));
        out.push_str(&format!("| Day | {} |\n", dt.format("%A")));
        out.push_str(&format!("| Relative | **{}** |\n\n", relative));
    } else if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(args, "%Y-%m-%d %H:%M:%S") {
        // It's a datetime string — convert to epoch
        let epoch = dt.and_utc().timestamp();
        out.push_str(&format!("## 🔢 Converted from `{}`\n\n", args));
        out.push_str("| Format | Value |\n|---|---|\n");
        out.push_str(&format!("| Unix | `{}` |\n", epoch));
        out.push_str(&format!("| Unix (ms) | `{}` |\n", epoch * 1000));
        out.push_str(&format!("| ISO 8601 | `{}` |\n", dt.format("%Y-%m-%dT%H:%M:%SZ")));
    } else if let Ok(dt) = chrono::NaiveDate::parse_from_str(args, "%Y-%m-%d") {
        let epoch = dt.and_hms_opt(0, 0, 0).unwrap_or_default().and_utc().timestamp();
        out.push_str(&format!("## 📅 Converted from `{}`\n\n", args));
        out.push_str("| Format | Value |\n|---|---|\n");
        out.push_str(&format!("| Unix | `{}` |\n", epoch));
        out.push_str(&format!("| Unix (ms) | `{}` |\n", epoch * 1000));
        out.push_str(&format!("| ISO 8601 | `{}` |\n", dt.format("%Y-%m-%dT00:00:00Z")));
    } else {
        out.push_str(&format!("⚠️ Unrecognized format: `{}`\n\n", args));
        out.push_str("**Supported inputs:**\n");
        out.push_str("- No args → show current time\n");
        out.push_str("- `1700000000` → unix timestamp to date\n");
        out.push_str("- `2024-01-15 10:30:00` → date to unix\n");
        out.push_str("- `2024-01-15` → date to unix (midnight UTC)\n");
    }

    out.push_str(&format!("{}\n\n`{}` · #timestamp #dev #memogram-rs", tg_footer("memogram-rs", "timestamp"), now.format("%Y-%m-%d %H:%M")));
    out
}

async fn fetch_dns(domain: &str) -> Result<String> {
    let domain = domain.trim().to_string();
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    if domain.is_empty() {
        return Ok("usage: `/dns example.com`".into());
    }

    let mut out = format!("{}\n\n", tg_header("🔍", "DNS Lookup", &domain));
    out.push_str(&format!("**Domain:** `{}`\n\n", domain));

    let types = ["A", "AAAA", "MX", "NS", "TXT", "CNAME"];
    let mut any_found = false;

    for record_type in &types {
        let url = format!("https://dns.google/resolve?name={}&type={}", urlencoding::encode(&domain), record_type);
        let v: serde_json::Value = match tokio::time::timeout(std::time::Duration::from_secs(5), HTTP.get(&url).header("User-Agent", "memogram-rs").send()).await {
            Ok(Ok(r)) => match r.json::<serde_json::Value>().await { Ok(j) => j, Err(_) => serde_json::Value::Null },
            _ => serde_json::Value::Null,
        };
        if let Some(answer) = v["Answer"].as_array() {
            if !answer.is_empty() {
                any_found = true;
                out.push_str(&format!("## 📋 {} Records\n\n", record_type));
                out.push_str("| Type | TTL | Data |\n|---|---|---|\n");
                for a in answer.iter() {
                    let rtype = a["type"].as_str().unwrap_or(record_type);
                    let ttl = a["TTL"].as_u64().unwrap_or(0);
                    let data = a["data"].as_str().unwrap_or("?");
                    out.push_str(&format!("| {} | {}s | `{}` |\n", rtype, ttl, data));
                }
                out.push('\n');
            }
        }
    }

    if !any_found {
        out.push_str("⚠️ No DNS records found.\n\n");
    }

    out.push_str(&format!("🔗 [DNS Checker](https://dnschecker.org/#A/{})\n\n", urlencoding::encode(&domain)));
    out.push_str(&format!("{}\n\n`{}` · #dns #dev #memogram-rs", tg_footer("dns.google", "dns"), now));
    Ok(out)
}

fn create_ports() -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let mut out = format!("{}\n\n", tg_header("🔌", "Common Ports", ""));
    out.push_str("| Port | Protocol | Service | Notes |\n|---|---|---|---|\n");

    let ports = [
        ("20/21", "TCP", "FTP", "File Transfer (data/control)"),
        ("22", "TCP", "SSH", "Secure Shell"),
        ("23", "TCP", "Telnet", "Unencrypted shell (avoid)"),
        ("25", "TCP", "SMTP", "Email sending"),
        ("53", "TCP/UDP", "DNS", "Domain resolution"),
        ("80", "TCP", "HTTP", "Web traffic"),
        ("110", "TCP", "POP3", "Email retrieval"),
        ("143", "TCP", "IMAP", "Email access"),
        ("443", "TCP", "HTTPS", "Encrypted web traffic"),
        ("445", "TCP", "SMB", "Windows file sharing"),
        ("993", "TCP", "IMAPS", "IMAP over TLS"),
        ("995", "TCP", "POP3S", "POP3 over TLS"),
        ("3306", "TCP", "MySQL", "Database"),
        ("3389", "TCP", "RDP", "Remote Desktop"),
        ("5432", "TCP", "PostgreSQL", "Database"),
        ("5672", "TCP", "AMQP", "RabbitMQ"),
        ("5900", "TCP", "VNC", "Remote desktop"),
        ("6379", "TCP", "Redis", "Cache / message broker"),
        ("8080", "TCP", "HTTP Alt", "Dev servers, proxies"),
        ("8443", "TCP", "HTTPS Alt", "Alt encrypted web"),
        ("9090", "TCP", "Prometheus", "Metrics UI"),
        ("27017", "TCP", "MongoDB", "Document database"),
    ];

    for (port, proto, svc, note) in ports {
        out.push_str(&format!("| `{}` | {} | **{}** | {} |\n", port, proto, svc, note));
    }

    out.push_str(&format!("\n> 💡 **Tip:** Use `/containers` to check your own services\n\n"));
    out.push_str(&format!("{}\n\n`{}` · #ports #dev #memogram-rs", tg_footer("memogram-rs", "ports"), now));
    out
}

fn create_json(text: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "usage: `/json <text>` — pretty-print or validate JSON".into();
    }
    let mut out = format!("{}\n\n", tg_header("🔧", "JSON", ""));
    match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(v) => {
            let pretty = serde_json::to_string_pretty(&v).unwrap_or_else(|_| trimmed.to_string());
            let keys = v.as_object().map(|o| o.len()).unwrap_or(0);
            let chars = pretty.len();
            let obj_type = if v.is_object() { "Object" } else if v.is_array() { "Array" } else { "Primitive" };
            let arr_len = v.as_array().map(|a| a.len()).unwrap_or(0);
            out.push_str("## ✅ Valid JSON\n\n");
            out.push_str("| Metric | Value |\n|---|---|\n");
            out.push_str(&format!("| Type | `{}` |\n", obj_type));
            if keys > 0 { out.push_str(&format!("| Keys | `{}` |\n", keys)); }
            if arr_len > 0 { out.push_str(&format!("| Items | `{}` |\n", arr_len)); }
            out.push_str(&format!("| Size | `{} bytes`\n\n", chars));
            if pretty.len() <= 3000 {
                out.push_str(&format!("```\n{}\n```\n", pretty));
            } else {
                out.push_str(&format!("```\n{}...\n```\n\n_Truncated — {} bytes total._\n", &pretty[..3000], chars));
            }
        }
        Err(e) => {
            out.push_str("## ❌ Invalid JSON\n\n");
            out.push_str(&format!("**Error:** `{}`\n\n", e));
            out.push_str(&format!("**Line:** `{}` · **Column:** `{}`\n\n", e.line(), e.column()));
        }
    }
    out.push_str(&format!("{}\n\n`{}` · #json #dev #memogram-rs", tg_footer("memogram-rs", "json"), now));
    out
}

fn create_regex(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let pattern = parts.first().filter(|s| !s.is_empty()).copied().unwrap_or("");
    let test = parts.get(1).unwrap_or(&"");
    let mut out = format!("{}\n\n", tg_header("🔍", "Regex Test", pattern));

    if pattern.is_empty() {
        out.push_str("usage: `/regex <pattern> <test string>`\n\n");
        out.push_str("**Examples:**\n");
        out.push_str("- `/regex \\d+ there are 3 apples`\n");
        out.push_str("- `/regex [a-z]+@example\\.com test@email.com`\n");
        out.push_str("- `/regex ^\\d{4}-\\d{2}-\\d{2}$ 2024-01-15`\n");
        out.push_str(&format!("\n{}\n\n`{}` · #regex #dev #memogram-rs", tg_footer("memogram-rs", "regex"), now));
        return out;
    }

    match regex::Regex::new(pattern) {
        Ok(re) => {
            out.push_str("## ✅ Valid Pattern\n\n");
            out.push_str(&format!("**Pattern:** `{}`\n\n", pattern));
            if test.is_empty() {
                out.push_str("_Pass a test string to see matches._\n");
            } else {
                let matches: Vec<(usize, usize, &str)> = re.find_iter(test).map(|m| (m.start(), m.end(), m.as_str())).collect();
                out.push_str(&format!("**Test:** `{}`\n\n", test));
                out.push_str(&format!("**Matches:** `{}`\n\n", matches.len()));
                if !matches.is_empty() {
                    out.push_str("| # | Match | Start | End |\n|---|---|---|---|\n");
                    for (i, (start, end, m)) in matches.iter().enumerate() {
                        out.push_str(&format!("| {} | `{}` | {} | {} |\n", i + 1, m, start, end));
                    }
                    // Highlight matches in test string
                    let mut highlighted = test.to_string();
                    let mut offset = 0;
                    for (_, end, m) in &matches {
                        let insert_at = end + offset;
                        highlighted.insert_str(insert_at, "**");
                        offset += 2;
                        let insert_at = end + offset;
                        highlighted.insert_str(insert_at, "**");
                        offset += 2;
                    }
                    out.push_str(&format!("\n**Highlighted:** {}\n", highlighted));
                } else {
                    out.push_str("_No matches found._\n");
                }
                // Named groups
                let group_names: Vec<String> = re.capture_names().flatten().map(|s| s.to_string()).collect();
                if !group_names.is_empty() {
                    out.push_str(&format!("\n**Named groups:** `{}`\n", group_names.join("`, `")));
                }
            }
        }
        Err(e) => {
            out.push_str("## ❌ Invalid Pattern\n\n");
            out.push_str(&format!("**Error:** `{}`\n\n", e));
            out.push_str("**Common patterns:**\n");
            out.push_str("- `\\d+` — one or more digits\n");
            out.push_str("- `[a-zA-Z]+` — one or more letters\n");
            out.push_str("- `.*` — any characters\n");
            out.push_str("- `^...$` — start/end anchors\n");
            out.push_str("- `(group)` — capture groups\n");
        }
    }
    out.push_str(&format!("\n{}\n\n`{}` · #regex #dev #memogram-rs", tg_footer("memogram-rs", "regex"), now));
    out
}

fn create_uuid() -> String {
    use rand::Rng;
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let uuid = {
        let mut bytes = [0u8; 16];
        rand::rng().fill(&mut bytes);
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        format!("{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            bytes[0], bytes[1], bytes[2], bytes[3],
            bytes[4], bytes[5], bytes[6], bytes[7],
            bytes[8], bytes[9], bytes[10], bytes[11],
            bytes[12], bytes[13], bytes[14], bytes[15])
    };
    let mut out = format!("{}\n\n", tg_header("🆔", "UUID v4", &uuid));
    out.push_str(&format!("**UUID:** `{}`\n\n", uuid));
    out.push_str("## 📋 Format\n\n");
    out.push_str("```\n");
    out.push_str(&uuid);
    out.push_str("\n```\n\n");
    out.push_str("## ℹ️ Info\n\n");
    out.push_str(&format!("| Field | Value |\n|---|---|\n"));
    out.push_str("| Version | `4` (random) |\n");
    out.push_str("| Variant | `RFC 4122` |\n");
    out.push_str(&format!("| Generated | `{}` |\n\n", now));
    out.push_str(&format!("{}\n\n`{}` · #uuid #dev #memogram-rs", tg_footer("memogram-rs", "uuid"), now));
    out
}

// === NEWS: LOBSTERS + PRODUCT HUNT ===

async fn fetch_lobsters() -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let v: serde_json::Value = HTTP.get("https://lobste.rs/hottest.json")
        .header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    let stories = v.as_array().ok_or_else(|| anyhow::anyhow!("no stories"))?;
    let mut out = format!("{}\n\n", tg_header("🦞", "Lobsters Hot", ""));
    out.push_str("| # | Title | Points | Comments | Tags |\n|---|---|---|---|---|\n");
    for (i, s) in stories.iter().take(15).enumerate() {
        let title = s["title"].as_str().unwrap_or("?");
        let url = s["url"].as_str().unwrap_or("");
        let score = s["score"].as_u64().unwrap_or(0);
        let comments = s["comment_count"].as_u64().unwrap_or(0);
        let tags: Vec<String> = s["tags"].as_array().map(|a| a.iter().filter_map(|t| t.as_str()).map(|s| format!("`{}`", s)).collect()).unwrap_or_default();
        let tag_str = tags.join(" ");
        let link = if url.is_empty() { format!("[{}]({})", title, s["comments_url"].as_str().unwrap_or("#")) } else { format!("[{}]({})", title, url) };
        out.push_str(&format!("| {} | {} | {} | {} | {} |\n", i + 1, link, score, comments, tag_str));
    }
    out.push_str(&format!("\n{}\n\n`{}` · #lobsters #news #memogram-rs", tg_footer("lobste.rs", "lobsters"), now));
    Ok(out)
}

async fn fetch_ph() -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let today = Local::now().format("%Y-%m-%d").to_string();
    let url = format!("https://www.producthunt.com/frontend/graphql");
    let body = serde_json::json!({
        "query": "query { posts(order: VOTES, postedAfter: \"${}T00:00:00Z\") { edges { node { name tagline url votesCount commentsCount topics { edges { node { name } } } } } } }",
        "variables": {}
    });
    let v: serde_json::Value = match tokio::time::timeout(std::time::Duration::from_secs(8),
        HTTP.post(&url).header("User-Agent", "memogram-rs").json(&body).send()
    ).await {
        Ok(Ok(r)) => match r.json::<serde_json::Value>().await { Ok(j) => j, Err(_) => serde_json::Value::Null },
        _ => serde_json::Value::Null,
    };

    let mut out = format!("{}\n\n", tg_header("🚀", "Product Hunt Today", &today));

    if let Some(edges) = v["data"]["posts"]["edges"].as_array() {
        if !edges.is_empty() {
            out.push_str("| # | Product | Votes | Comments | Tags |\n|---|---|---|---|---|\n");
            for (i, edge) in edges.iter().take(10).enumerate() {
                let node = &edge["node"];
                let name = node["name"].as_str().unwrap_or("?");
                let tagline = node["tagline"].as_str().unwrap_or("");
                let url = node["url"].as_str().unwrap_or("#");
                let votes = node["votesCount"].as_u64().unwrap_or(0);
                let comments = node["commentsCount"].as_u64().unwrap_or(0);
                let topics: Vec<String> = node["topics"]["edges"].as_array().map(|a| a.iter().filter_map(|e| e["node"]["name"].as_str()).take(2).map(|s| format!("`{}`", s)).collect()).unwrap_or_default();
                out.push_str(&format!("| {} | [**{}**]({})\n  _{}_ | {} | {} | {} |\n", i + 1, name, url, tagline.chars().take(60).collect::<String>(), votes, comments, topics.join(" ")));
            }
            out.push_str(&format!("\n{}\n\n`{}` · #ph #news #memogram-rs", tg_footer("producthunt.com", "ph"), now));
            return Ok(out);
        }
    }
    // Fallback: RSS-like scrape
    out.push_str("_PH API unavailable — try again later._\n\n");
    out.push_str(&format!("🔗 [producthunt.com](https://www.producthunt.com)\n\n"));
    out.push_str(&format!("{}\n\n`{}` · #ph #news #memogram-rs", tg_footer("producthunt.com", "ph"), now));
    Ok(out)
}

// === PLANNING: WEEKLY + RETRO ===

async fn fetch_weekly(memos_url: &str, token: &str) -> Result<String> {
    let now = Local::now();
    let week_start = (now - chrono::Duration::days(now.weekday().num_days_from_monday() as i64)).format("%Y-%m-%d").to_string();
    let today = now.format("%Y-%m-%d").to_string();
    let v: serde_json::Value = HTTP.get(format!("{memos_url}/api/v1/memos?pageSize=100"))
        .header("Authorization", format!("Bearer {token}")).send().await?.json().await?;
    let memos = v["memos"].as_array().ok_or_else(|| anyhow::anyhow!("no memos"))?;
    let week_memos: Vec<&serde_json::Value> = memos.iter().filter(|m| {
        m["createTime"].as_str().map(|t| t >= week_start.as_str() && t <= format!("{}T23:59", today).as_str()).unwrap_or(false)
    }).collect();
    let count = week_memos.len();
    let total_chars: usize = week_memos.iter().filter_map(|m| m["content"].as_str()).map(|c| c.len()).sum();

    // Count tags
    let mut tag_counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for m in &week_memos {
        if let Some(tags) = m["tags"].as_array() {
            for t in tags {
                if let Some(s) = t.as_str() {
                    *tag_counts.entry(s.to_string()).or_insert(0) += 1;
                }
            }
        }
    }

    let mut out = format!("{}\n\n", tg_header("📅", "Weekly Review", &format!("{} → {}", week_start, today)));
    out.push_str(&format!("**{} memos** · **~{} words** written this week\n\n", count, total_chars / 5));

    out.push_str("## 📊 Activity\n\n");
    out.push_str("| Day | Memos |\n|---|---|\n");
    for i in 0..7u32 {
        let d = (now - chrono::Duration::days(now.weekday().num_days_from_monday() as i64) + chrono::Duration::days(i as i64)).format("%a %m/%d").to_string();
        let day_prefix = (now - chrono::Duration::days(now.weekday().num_days_from_monday() as i64) + chrono::Duration::days(i as i64)).format("%Y-%m-%d").to_string();
        let day_count = week_memos.iter().filter(|m| m["createTime"].as_str().map(|t| t.starts_with(&day_prefix)).unwrap_or(false)).count();
        let bar = "█".repeat(day_count.min(15));
        out.push_str(&format!("| {} | {} {} |\n", d, bar, day_count));
    }

    if !tag_counts.is_empty() {
        out.push_str("\n## 🏷️ Top Tags\n\n");
        let mut sorted_tags: Vec<_> = tag_counts.into_iter().collect();
        sorted_tags.sort_by(|a, b| b.1.cmp(&a.1));
        for (tag, count) in sorted_tags.iter().take(8) {
            out.push_str(&format!("- `#{}` — {} memos\n", tag, count));
        }
    }

    out.push_str("\n## 💡 Reflection Prompts\n\n");
    out.push_str("- What was my biggest win this week?\n");
    out.push_str("- What took longer than expected?\n");
    out.push_str("- What should I stop doing?\n");
    out.push_str("- What should I start doing next week?\n\n");
    out.push_str(&format!("{}\n\n`{}` · #weekly #planning #memogram-rs", tg_footer("memogram-rs", "weekly"), now.format("%Y-%m-%d %H:%M")));
    Ok(out)
}

fn create_retro(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let date = Local::now().format("%Y-%m-%d").to_string();
    let sprint = if args.trim().is_empty() { "Current Sprint" } else { args };
    let mut out = format!("{}\n\n", tg_header("🔄", "Retrospective", sprint));
    out.push_str(&format!("**Date:** `{}` · **Sprint:** `{}`\n\n", date, sprint));
    out.push_str("## ✅ What Went Well\n\n- \n- \n- \n\n");
    out.push_str("## ⚠️ What Could Improve\n\n- \n- \n- \n\n");
    out.push_str("## 🔧 Action Items\n\n");
    out.push_str("| Action | Owner | Due | Priority |\n|---|---|---|---|\n");
    out.push_str("|  |  |  | P1 |\n");
    out.push_str("|  |  |  | P2 |\n\n");
    out.push_str("## 📊 Sprint Stats\n\n");
    out.push_str("| Metric | Value |\n|---|---|\n");
    out.push_str("| Planned |  |\n");
    out.push_str("| Completed |  |\n");
    out.push_str("| Carry-over |  |\n");
    out.push_str("| Velocity |  |\n\n");
    out.push_str("> _Tip: Be honest. What will we actually change?_\n\n");
    out.push_str(&format!("{}\n\n`{}` · #retro #planning #memogram-rs", tg_footer("memogram-rs", "retro"), now));
    out
}

// === BIO: PATENT + SPECIES + LAB ===

async fn fetch_patent(query: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    if query.trim().is_empty() {
        return Ok("usage: `/patent <query>` — search Google Patents".into());
    }
    let url = format!("https://patents.google.com/xhr/query?url=q%3D{}%26country%3DUS%26language%3DENGLISH&exp=", urlencoding::encode(query));
    let v: serde_json::Value = match tokio::time::timeout(std::time::Duration::from_secs(8),
        HTTP.get(&url).header("User-Agent", "memogram-rs").send()
    ).await {
        Ok(Ok(r)) => match r.json::<serde_json::Value>().await { Ok(j) => j, Err(_) => serde_json::Value::Null },
        _ => serde_json::Value::Null,
    };

    let mut out = format!("{}\n\n", tg_header("📜", "Patents", query));
    if let Some(results) = v["results"]["cluster"].as_array() {
        if let Some(patents) = results.first().and_then(|c| c["result"].as_array()) {
            out.push_str("| # | Patent | Assignee | Date | Status |\n|---|---|---|---|---|\n");
            for (i, p) in patents.iter().take(10).enumerate() {
                let title = p["title"].as_str().unwrap_or("?");
                let patent_num = p["publication_number"].as_str().unwrap_or("");
                let assignee = p["assignee"].as_str().unwrap_or("—");
                let date = p["date"].as_str().unwrap_or("—");
                let status = p["patent_status"].as_str().unwrap_or("—");
                let link = format!("[{}](https://patents.google.com/patent/{})", title.chars().take(50).collect::<String>(), patent_num);
                out.push_str(&format!("| {} | {} | {} | {} | {} |\n", i + 1, link, assignee, date, status));
            }
            out.push_str(&format!("\n🔗 [Search on Google Patents](https://patents.google.com/?q={})\n\n", urlencoding::encode(query)));
            out.push_str(&format!("{}\n\n`{}` · #patent #bio #memogram-rs", tg_footer("patents.google.com", "patent"), now));
            return Ok(out);
        }
    }
    out.push_str("_No patents found or API unavailable._\n\n");
    out.push_str(&format!("🔗 [Search Google Patents](https://patents.google.com/?q={})\n\n", urlencoding::encode(query)));
    out.push_str(&format!("{}\n\n`{}` · #patent #bio #memogram-rs", tg_footer("patents.google.com", "patent"), now));
    Ok(out)
}

async fn fetch_species(query: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    if query.trim().is_empty() {
        return Ok("usage: `/species <name>` — e.g. `/species E. coli`".into());
    }
    let url = format!("https://api.gbif.org/v1/species/search?q={}&limit=5", urlencoding::encode(query));
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    let results = v["results"].as_array().ok_or_else(|| anyhow::anyhow!("no results"))?;

    let mut out = format!("{}\n\n", tg_header("🧬", "Species", query));
    if results.is_empty() {
        out.push_str("_No species found._\n\n");
        out.push_str(&format!("{}\n\n`{}` · #species #bio #memogram-rs", tg_footer("gbif.org", "species"), now));
        return Ok(out);
    }

    for (i, sp) in results.iter().take(3).enumerate() {
        let sci = sp["scientificName"].as_str().unwrap_or("?");
        let common = sp["commonName"].as_str().unwrap_or("No common name");
        let rank = sp["rank"].as_str().unwrap_or("?");
        let status = sp["taxonomicStatus"].as_str().unwrap_or("?");
        let kingdom = sp["kingdom"].as_str().unwrap_or("?");
        let phylum = sp["phylum"].as_str().unwrap_or("?");
        let class = sp["class"].as_str().unwrap_or("?");
        let order = sp["order"].as_str().unwrap_or("?");
        let family = sp["family"].as_str().unwrap_or("?");
        let genus = sp["genus"].as_str().unwrap_or("?");
        let key = sp["key"].as_u64().unwrap_or(0);
        let ncbi = sp[" identifiers"].as_array().and_then(|ids| ids.iter().find(|id| id["type"].as_str() == Some("NCBI")).and_then(|id| id["identifier"].as_str())).unwrap_or("");

        out.push_str(&format!("## {} **{}**\n\n", if i == 0 { "🔬" } else { "📌" }, sci));
        out.push_str(&format!("**Common:** {} · **Rank:** {} · **Status:** {}\n\n", common, rank, status));
        out.push_str("| Taxonomy | Value |\n|---|---|\n");
        out.push_str(&format!("| Kingdom | {} |\n", kingdom));
        out.push_str(&format!("| Phylum | {} |\n", phylum));
        out.push_str(&format!("| Class | {} |\n", class));
        out.push_str(&format!("| Order | {} |\n", order));
        out.push_str(&format!("| Family | {} |\n", family));
        out.push_str(&format!("| Genus | {} |\n\n", genus));
        if key > 0 {
            out.push_str(&format!("🔗 [GBIF](https://www.gbif.org/species/{}) ", key));
        }
        if !ncbi.is_empty() {
            out.push_str(&format!("· [NCBI](https://www.ncbi.nlm.nih.gov/Taxonomy/Browser/wwwtaxa?id={}) ", ncbi));
        }
        out.push_str("\n\n");
    }
    out.push_str(&format!("{}\n\n`{}` · #species #bio #memogram-rs", tg_footer("gbif.org", "species"), now));
    Ok(out)
}

fn create_lab(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let date = Local::now().format("%Y-%m-%d").to_string();
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let protocol = parts.first().filter(|s| !s.is_empty()).copied().unwrap_or("Protocol");
    let notes = parts.get(1).unwrap_or(&"");
    let mut out = format!("{}\n\n", tg_header("🧪", "Lab Protocol", protocol));
    out.push_str(&format!("**Date:** `{}` · **Protocol:** `{}`\n\n", date, protocol));
    if !notes.is_empty() {
        out.push_str(&format!("**Notes:** {}\n\n", notes));
    }
    out.push_str("## 📋 Materials\n\n- [ ] \n- [ ] \n- [ ] \n\n");
    out.push_str("## 🔬 Procedure\n\n");
    out.push_str("1. **Prep:** \n");
    out.push_str("2. **Step 1:** \n");
    out.push_str("3. **Step 2:** \n");
    out.push_str("4. **Step 3:** \n");
    out.push_str("5. **Cleanup:** \n\n");
    out.push_str("## 📊 Results\n\n");
    out.push_str("| Parameter | Value | Notes |\n|---|---|---|\n|  |  |  |\n\n");
    out.push_str("## ⚠️ Safety\n\n");
    out.push_str("- PPE required: \n");
    out.push_str("- Waste disposal: \n");
    out.push_str("- Emergency: \n\n");
    out.push_str("## 📝 Observations\n\n- \n\n");
    out.push_str(&format!("{}\n\n`{}` · #lab #bio #memogram-rs", tg_footer("memogram-rs", "lab"), now));
    out
}

// === BIO: PRE-HEALTH TRACKING ===

fn create_prereqs(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let track = args.trim().to_lowercase();
    let mut out = format!("{}\n\n", tg_header("🎓", "Prerequisites", if track.is_empty() { "all tracks" } else { &track }));
    out.push_str("**Tracks:** med, dental, vet, pharmacy, pa, optometry\n\n");

    let tracks = [
        ("med (MD/DO)", vec![
            ("Biology I + II w/ lab", "4 cr", "Bio 101/102"),
            ("General Chemistry I + II w/ lab", "4 cr", "Chem 101/102"),
            ("Organic Chemistry I + II w/ lab", "4 cr", "OChem 101/102"),
            ("Physics I + II w/ lab", "4 cr", "Phys 101/102"),
            ("Biochemistry", "3 cr", "Bchem 301"),
            ("English / Writing", "6 cr", "Lit/Writing"),
            ("Math (Calc or Stats)", "3-6 cr", "Math 101+"),
            ("Psychology", "3 cr", "Psych 101"),
            ("Sociology", "3 cr", "Soc 101"),
        ]),
        ("dental (DDS/DMD)", vec![
            ("Biology I + II w/ lab", "4 cr", "Bio 101/102"),
            ("General Chemistry I + II w/ lab", "4 cr", "Chem 101/102"),
            ("Organic Chemistry I + II w/ lab", "4 cr", "OChem 101/102"),
            ("Physics I + II w/ lab", "4 cr", "Phys 101/102"),
            ("Biochemistry", "3 cr", "Bchem 301"),
            ("English / Writing", "6 cr", "Lit/Writing"),
            ("Math (Calc or Stats)", "3-6 cr", "Math 101+"),
        ]),
        ("pharmacy (PharmD)", vec![
            ("Biology I + II w/ lab", "4 cr", "Bio 101/102"),
            ("General Chemistry I + II w/ lab", "4 cr", "Chem 101/102"),
            ("Organic Chemistry I + II w/ lab", "4 cr", "OChem 101/102"),
            ("Physics I + II w/ lab", "4 cr", "Phys 101/102"),
            ("Biochemistry", "3 cr", "Bchem 301"),
            ("Anatomy & Physiology", "4 cr", "A&P 101/102"),
            ("Microbiology", "4 cr", "Micro 201"),
            ("Math (Calc/Stats)", "3-6 cr", "Math 101+"),
            ("English / Writing", "6 cr", "Lit/Writing"),
        ]),
        ("pa (PA school)", vec![
            ("Biology I + II w/ lab", "4 cr", "Bio 101/102"),
            ("General Chemistry I + II w/ lab", "4 cr", "Chem 101/102"),
            ("Organic Chemistry or Biochem", "3-4 cr", "OChem/Bchem"),
            ("Anatomy & Physiology I + II", "4 cr", "A&P 101/102"),
            ("Microbiology", "4 cr", "Micro 201"),
            ("Genetics", "3 cr", "Genetics 301"),
            ("Psychology", "3 cr", "Psych 101"),
            ("Statistics", "3 cr", "Stats 201"),
            ("English / Writing", "6 cr", "Lit/Writing"),
        ]),
        ("vet (DVM)", vec![
            ("Biology I + II w/ lab", "4 cr", "Bio 101/102"),
            ("General Chemistry I + II w/ lab", "4 cr", "Chem 101/102"),
            ("Organic Chemistry I + II w/ lab", "4 cr", "OChem 101/102"),
            ("Physics I + II w/ lab", "4 cr", "Phys 101/102"),
            ("Biochemistry", "3 cr", "Bchem 301"),
            ("Anatomy & Physiology", "4 cr", "A&P 101/102"),
            ("Microbiology", "4 cr", "Micro 201"),
            ("Genetics", "3 cr", "Genetics 301"),
            ("English / Writing", "6 cr", "Lit/Writing"),
        ]),
        ("optometry (OD)", vec![
            ("Biology I + II w/ lab", "4 cr", "Bio 101/102"),
            ("General Chemistry I + II w/ lab", "4 cr", "Chem 101/102"),
            ("Organic Chemistry I + II w/ lab", "4 cr", "OChem 101/102"),
            ("Physics I + II w/ lab", "4 cr", "Phys 101/102"),
            ("Biochemistry", "3 cr", "Bchem 301"),
            ("Anatomy & Physiology", "4 cr", "A&P 101/102"),
            ("Microbiology", "4 cr", "Micro 201"),
            ("Math (Calc/Stats)", "3-6 cr", "Math 101+"),
            ("English / Writing", "6 cr", "Lit/Writing"),
        ]),
    ];

    let selected: Vec<_> = if track.is_empty() {
        tracks.iter().collect()
    } else {
        tracks.iter().filter(|(name, _)| name.to_lowercase().contains(&track)).collect()
    };

    if selected.is_empty() {
        out.push_str("_No matching track. Try: med, dental, vet, pharmacy, pa, optometry_\n\n");
    } else {
        for (name, courses) in &selected {
            out.push_str(&format!("## 📚 {}\n\n", name));
            out.push_str("| Course | Credits | Example |\n|---|---|---|\n");
            for (course, credits, example) in courses {
                out.push_str(&format!("| {} | {} | {} |\n", course, credits, example));
            }
            let total: i32 = courses.iter().filter_map(|(_, c, _)| c.split(' ').next()?.parse::<i32>().ok()).sum();
            out.push_str(&format!("\n> **Total: ~{} credits** of prereqs\n\n", total));
        }
    }

    out.push_str("## 📝 Notes\n\n");
    out.push_str("- Check specific schools — requirements vary\n");
    out.push_str("- AP/IB credit may satisfy some prerequisites\n");
    out.push_str("- Shadowing + clinical hours are separate from coursework\n\n");
    out.push_str(&format!("{}\n\n`{}` · #prereqs #bio #memogram-rs", tg_footer("memogram-rs", "prereqs"), now));
    out
}

async fn fetch_mcat(topic: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let topic_lower = topic.trim().to_lowercase();

    let sections = vec![
        ("Chemical & Physical Foundations of Biological Systems", "Chem/Phys", vec![
            ("General Chemistry", "Atomic structure, periodic trends, bonding, stoichiometry, thermo, kinetics, equilibrium, acids/bases, electrochemistry"),
            ("Organic Chemistry", "Nomenclature, reactions, stereochem, spectroscopy, mechanisms"),
            ("Physics", "Kinematics, forces, energy, fluids, thermodynamics, optics, circuits, waves, sound, magnetism"),
            ("Biochemistry", "Amino acids, proteins, enzymes, carbs, lipids, nucleic acids, metabolism pathways"),
        ]),
        ("Critical Analysis & Reasoning Skills", "CARS", vec![
            ("Comprehension", "Main idea, author's tone, passage structure, argument mapping"),
            ("Reasoning", "Strengthen/weaken, inference, parallel reasoning, flaw identification"),
            ("Analysis", "Application to new contexts, rhetorical analysis, analogy"),
        ]),
        ("Biological & Biochemical Foundations of Living Systems", "Bio/Biochem", vec![
            ("Biology", "Cells, organelles, cell cycle, genetics, molecular bio, evolution, ecology, organ systems"),
            ("Biochemistry", "Enzyme kinetics, metabolism (glycolysis, TCA, ETC), signaling pathways"),
            ("Genetics", "Mendelian, molecular genetics, gene expression, inheritance patterns"),
            ("Organ Systems", "Cardio, respiratory, renal, GI, endocrine, immune, reproductive, nervous, musculoskeletal"),
        ]),
        ("Psychological, Social & Biological Foundations of Behavior", "Psych/Soc", vec![
            ("Psychology", "Cognition, memory, learning, motivation, emotion, development, personality, disorders"),
            ("Sociology", "Social structures, groups, stratification, demographics, culture, institutions"),
            ("Social Psychology", "Conformity, persuasion, attitudes, prejudice, group dynamics"),
            ("Neuroscience", "Brain structures, neurotransmitters, sensation, perception, behavioral neuroscience"),
        ]),
    ];

    let mut out = format!("{}\n\n", tg_header("📖", "MCAT Study Guide", if topic.is_empty() { "all sections" } else { &topic }));

    for (section, abbr, topics) in &sections {
        if !topic_lower.is_empty() && !section.to_lowercase().contains(&topic_lower) && !abbr.to_lowercase().contains(&topic_lower) {
            // Check subtopics too
            let any_match = topics.iter().any(|(name, desc)| name.to_lowercase().contains(&topic_lower) || desc.to_lowercase().contains(&topic_lower));
            if !any_match { continue; }
        }
        out.push_str(&format!("## 📝 {} ({})\n\n", section, abbr));
        out.push_str("| Topic | Key Concepts |\n|---|---|\n");
        for (name, concepts) in topics {
            out.push_str(&format!("| **{}** | {} |\n", name, concepts));
        }
        out.push('\n');
    }

    out.push_str("## 📅 Study Plan\n\n");
    out.push_str("| Week | Focus | Hours |\n|---|---|---|\n");
    out.push_str("| 1-2 | Bio/Biochem foundations | 20 |\n");
    out.push_str("| 3-4 | Chem/Phys foundations | 20 |\n");
    out.push_str("| 5-6 | Psych/Soc + CARS practice | 20 |\n");
    out.push_str("| 7-8 | Full-length practice tests | 25 |\n");
    out.push_str("| 9-10 | Weak areas + AAMC official | 25 |\n\n");
    out.push_str("## 🔗 Resources\n\n");
    out.push_str("- [AAMC Official](https://students-residents.aamc.org/mcat)\n");
    out.push_str("- [Khan Academy MCAT](https://www.khanacademy.org/test-prep/mcat)\n");
    out.push_str("- [AMCAS Guide](https://www.aamc.org/applying-amcas/visualizing-your-application)\n\n");
    out.push_str(&format!("{}\n\n`{}` · #mcat #bio #memogram-rs", tg_footer("memogram-rs", "mcat"), now));
    Ok(out)
}

fn create_clinical(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let date = Local::now().format("%Y-%m-%d").to_string();
    let parts: Vec<&str> = args.splitn(3, ' ').collect();
    let activity = parts.first().filter(|s| !s.is_empty()).copied().unwrap_or("activity");
    let hours = parts.get(1).unwrap_or(&"0");
    let note = parts.get(2).unwrap_or(&"");
    let mut out = format!("{}\n\n", tg_header("🏥", "Clinical Hours", activity));
    out.push_str(&format!("**Date:** `{}` · **Activity:** `{}` · **Hours:** `{}`\n\n", date, activity, hours));
    if !note.is_empty() {
        out.push_str(&format!("**Note:** {}\n\n", note));
    }
    out.push_str("## 📊 Clinical Log\n\n");
    out.push_str("| Date | Activity | Hours | Note |\n|---|---|---|---|\n");
    out.push_str(&format!("| {} | {} | {} | {} |\n\n", date, activity, hours, note));
    out.push_str("## 🎯 Typical Requirements\n\n");
    out.push_str("| Program | Clinical Hours | Shadowing |\n|---|---|---|\n");
    out.push_str("| MD (allopathic) | 100-400+ | 40-100+ |\n");
    out.push_str("| DO (osteopathic) | 100-400+ | 40-100+ |\n");
    out.push_str("| PA | 500-2000+ | 100+ |\n");
    out.push_str("| Dental | 100-300+ | 50-100+ |\n");
    out.push_str("| Pharmacy | 200-1000+ (paid preferred) | 40+ |\n");
    out.push_str("| Vet | 200-500+ | 100+ |\n\n");
    out.push_str("> _Tip: Quality > quantity. Reflect on each experience._\n\n");
    out.push_str(&format!("{}\n\n`{}` · #clinical #bio #memogram-rs", tg_footer("memogram-rs", "clinical"), now));
    out
}

fn create_shadow(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let date = Local::now().format("%Y-%m-%d").to_string();
    let parts: Vec<&str> = args.splitn(3, ' ').collect();
    let doctor = parts.first().filter(|s| !s.is_empty()).copied().unwrap_or("Dr.");
    let hours = parts.get(1).unwrap_or(&"0");
    let specialty = parts.get(2).unwrap_or(&"");
    let mut out = format!("{}\n\n", tg_header("👁️", "Shadowing", doctor));
    out.push_str(&format!("**Date:** `{}` · **Doctor:** `{}` · **Hours:** `{}`\n\n", date, doctor, hours));
    if !specialty.is_empty() {
        out.push_str(&format!("**Specialty:** {}\n\n", specialty));
    }
    out.push_str("## 📝 Key Observations\n\n");
    out.push_str("- What did the doctor do well?\n");
    out.push_str("- What was the patient interaction like?\n");
    out.push_str("- What surprised you?\n");
    out.push_str("- Would you consider this specialty? Why?\n\n");
    out.push_str("## 📊 Shadowing Log\n\n");
    out.push_str("| Date | Doctor | Specialty | Hours | Notes |\n|---|---|---|---|---|\n");
    out.push_str(&format!("| {} | {} | {} | {} |  |\n\n", date, doctor, specialty, hours));
    out.push_str("> _Tip: Ask for a letter of recommendation after 40+ hours._\n\n");
    out.push_str(&format!("{}\n\n`{}` · #shadow #bio #memogram-rs", tg_footer("memogram-rs", "shadow"), now));
    out
}

fn create_ethics(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let date = Local::now().format("%Y-%m-%d").to_string();
    let scenario = if args.trim().is_empty() { "A 45-year-old patient refuses a life-saving blood transfusion on religious grounds. The surgery is scheduled for tomorrow." } else { args };

    let scenarios = [
        ("Informed Consent", "A 17-year-old asks you not to tell their parents about a positive STI test. State law requires parental notification for minors."),
        ("Resource Allocation", "You have one dose of a rare drug. Patient A is a 30-year-old with two children. Patient B is a 70-year-old Nobel laureate. Both will die without it."),
        ("Confidentiality", "A patient tells you they plan to harm their spouse. They ask you to keep it confidential."),
        ("End of Life", "A family demands continued aggressive treatment for a brain-dead patient. The advance directive says no extraordinary measures."),
        ("Research Ethics", "A clinical trial shows promising results but has severe side effects in 5% of subjects. The control group is getting worse. Do you unblind early?"),
    ];

    let (active_scenario, _) = if !args.trim().is_empty() {
        (args, "")
    } else {
        let idx = (chrono::Utc::now().timestamp() as usize) % scenarios.len();
        scenarios[idx]
    };

    let mut out = format!("{}\n\n", tg_header("⚖️", "Medical Ethics", ""));
    out.push_str(&format!("**Date:** `{}`\n\n", date));
    out.push_str("## 📋 Scenario\n\n");
    out.push_str(&format!("> {}\n\n", active_scenario));
    out.push_str("## 🧠 Framework\n\n");
    out.push_str("| Principle | Application |\n|---|---|\n");
    out.push_str("| **Autonomy** | Patient's right to self-determination |\n");
    out.push_str("| **Beneficence** | Act in the patient's best interest |\n");
    out.push_str("| **Non-maleficence** | First, do no harm |\n");
    out.push_str("| **Justice** | Fair distribution of resources |\n\n");
    out.push_str("## 📝 Your Analysis\n\n");
    out.push_str("### Arguments For\n\n- \n\n");
    out.push_str("### Arguments Against\n\n- \n\n");
    out.push_str("### Decision\n\n- \n\n");
    out.push_str("## 📚 More Scenarios\n\n");
    for (title, desc) in &scenarios {
        out.push_str(&format!("- **{}:** _{}_\n", title, desc.chars().take(80).collect::<String>()));
    }
    out.push_str(&format!("\n{}\n\n`{}` · #ethics #bio #memogram-rs", tg_footer("memogram-rs", "ethics"), now));
    out
}

// === NEWS: SCHOLAR + REDDIT + NEWS ===

async fn fetch_scholar(query: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    if query.trim().is_empty() {
        return Ok("usage: `/scholar <query>` — search Google Scholar".into());
    }
    let url = format!("https://scholar.google.com/scholar?q={}&hl=en&as_sdt=0,5", urlencoding::encode(query));
    let html = HTTP.get(&url).header("User-Agent", "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36").timeout(std::time::Duration::from_secs(8)).send().await?.text().await?;

    let mut out = format!("{}\n\n", tg_header("🎓", "Google Scholar", query));

    // Simple HTML parsing for search results
    let mut results = Vec::new();
    let mut remaining = html.as_str();
    while let Some(start) = remaining.find("<div class=\"gs_ri\">") {
        remaining = &remaining[start + 19..];
        if let Some(end) = remaining.find("<div class=\"gs_r gs_or gs_scl") {
            let block = &remaining[..end];
            // Extract title
            let title = block.split("class=\"gs_rt\">").nth(1)
                .and_then(|s| s.split("</h3>").next())
                .and_then(|s| {
                    let clean = s.replace("<b>", "").replace("</b>", "").replace("<i>", "").replace("</i>", "");
                    // Strip remaining HTML tags
                    let mut result = String::new();
                    let mut in_tag = false;
                    for c in clean.chars() {
                        if c == '<' { in_tag = true; } else if c == '>' { in_tag = false; } else if !in_tag { result.push(c); }
                    }
                    Some(result.trim().to_string())
                })
                .unwrap_or_else(|| "Untitled".to_string());
            // Extract snippet
            let snippet = block.split("class=\"gs_rs\">").nth(1)
                .and_then(|s| s.split("</div>").next())
                .map(|s| {
                    let clean = s.replace("<b>", "**").replace("</b>", "**");
                    let mut result = String::new();
                    let mut in_tag = false;
                    for c in clean.chars() {
                        if c == '<' { in_tag = true; } else if c == '>' { in_tag = false; } else if !in_tag { result.push(c); }
                    }
                    result.trim().chars().take(200).collect::<String>()
                })
                .unwrap_or_default();
            // Extract info line (authors, year, source)
            let info = block.split("class=\"gs_a\">").nth(1)
                .and_then(|s| s.split("</div>").next())
                .map(|s| {
                    let mut result = String::new();
                    let mut in_tag = false;
                    for c in s.chars() {
                        if c == '<' { in_tag = true; } else if c == '>' { in_tag = false; } else if !in_tag { result.push(c); }
                    }
                    result.trim().replace(" - ", " · ").chars().take(100).collect::<String>()
                })
                .unwrap_or_default();
            // Extract link
            let link = block.split("href=\"").nth(1)
                .and_then(|s| s.split("\"").next())
                .unwrap_or("#")
                .to_string();

            results.push((title, info, snippet, link));
            if results.len() >= 5 { break; }
            remaining = &remaining[end..];
        } else {
            break;
        }
    }

    if results.is_empty() {
        out.push_str("_No results found or Scholar blocked the request._\n\n");
    } else {
        for (i, (title, info, snippet, link)) in results.iter().enumerate() {
            out.push_str(&format!("### {}. [{}]({})\n\n", i + 1, title, link));
            out.push_str(&format!("**{}**\n\n", info));
            if !snippet.is_empty() {
                out.push_str(&format!("> {}\n\n", snippet));
            }
        }
    }

    out.push_str(&format!("🔗 [Search on Scholar](https://scholar.google.com/scholar?q={})\n\n", urlencoding::encode(query)));
    out.push_str(&format!("{}\n\n`{}` · #scholar #news #memogram-rs", tg_footer("scholar.google.com", "scholar"), now));
    Ok(out)
}

async fn fetch_reddit(sub: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let sub = sub.trim().trim_start_matches("r/").to_string();
    if sub.is_empty() {
        return Ok("usage: `/reddit <subreddit>` — e.g. `/reddit bioengineering`".into());
    }
    let url = format!("https://www.reddit.com/r/{}/hot.json?limit=15", urlencoding::encode(&sub));
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "memogram-rs/1.0").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;

    let posts = v["data"]["children"].as_array().ok_or_else(|| anyhow::anyhow!("no posts"))?;
    let mut out = format!("{}\n\n", tg_header("📱", "r/", &sub));

    if posts.is_empty() {
        out.push_str("_No posts found._\n\n");
    } else {
        out.push_str("| # | Title | Score | Comments | Flair |\n|---|---|---|---|---|\n");
        for (i, post) in posts.iter().take(15).enumerate() {
            let d = &post["data"];
            let title = d["title"].as_str().unwrap_or("?");
            let score = d["score"].as_u64().unwrap_or(0);
            let comments = d["num_comments"].as_u64().unwrap_or(0);
            let permalink = d["permalink"].as_str().unwrap_or("#");
            let flair = d["link_flair_text"].as_str().unwrap_or("");
            let stickied = d["stickied"].as_bool().unwrap_or(false);
            let prefix = if stickied { "📌 " } else { "" };
            let reddit_url = format!("https://reddit.com{}", permalink);
            let link = format!("[{}{}]({})", prefix, title.chars().take(80).collect::<String>(), reddit_url);
            out.push_str(&format!("| {} | {} | ⬆{} | 💬{} | {} |\n", i + 1, link, score, comments, flair));
        }
    }

    out.push_str(&format!("\n🔗 [r/{}](https://reddit.com/r/{})\n\n", sub, urlencoding::encode(&sub)));
    out.push_str(&format!("{}\n\n`{}` · #reddit #news #memogram-rs", tg_footer("reddit.com", "reddit"), now));
    Ok(out)
}

async fn fetch_news(topic: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    if topic.trim().is_empty() {
        return Ok("usage: `/news <topic>` — search news on any topic".into());
    }
    // Use Hacker News Algolia API as a general news source
    let url = format!("https://hn.algolia.com/api/v1/search?query={}&tags=story&hitsPerPage=10", urlencoding::encode(topic));
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;

    let hits = v["hits"].as_array().ok_or_else(|| anyhow::anyhow!("no hits"))?;
    let mut out = format!("{}\n\n", tg_header("📰", "News", topic));

    if hits.is_empty() {
        out.push_str("_No results found._\n\n");
    } else {
        out.push_str(&format!("**{} results** for _{}_\n\n", v["nbHits"].as_u64().unwrap_or(0), topic));
        out.push_str("| # | Title | Points | Comments | Date |\n|---|---|---|---|---|\n");
        for (i, hit) in hits.iter().take(10).enumerate() {
            let title = hit["title"].as_str().unwrap_or("?");
            let hn_url = format!("https://news.ycombinator.com/item?id={}", hit["objectID"].as_str().unwrap_or(""));
            let url = hit["url"].as_str().unwrap_or(&hn_url);
            let points = hit["points"].as_u64().unwrap_or(0);
            let comments = hit["num_comments"].as_u64().unwrap_or(0);
            let created = hit["created_at"].as_str().unwrap_or("");
            let date = if created.len() >= 10 { &created[..10] } else { "?" };
            let link = format!("[{}]({})", title.chars().take(70).collect::<String>(), url);
            out.push_str(&format!("| {} | {} | ⬆{} | 💬{} | {} |\n", i + 1, link, points, comments, date));
        }
    }

    out.push_str(&format!("\n{}\n\n`{}` · #news #memogram-rs", tg_footer("hn.algolia.com", "news"), now));
    Ok(out)
}

// === VIKUNJA-POWERED PLANNING ===

async fn vikunja_todo(args: &str, app: &App) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let date = Local::now().format("%Y-%m-%d").to_string();
    if app.vikunja_url.is_empty() || app.vikunja_token.is_empty() {
        return format!("{}\n\n⚠️ _Vikunja not configured. Set `VIKUNJA_URL` and `VIKUNJA_TOKEN`._\n\n{}\n\n`{}` · #todo #planning",
            tg_header("📋", "Todo", ""), tg_footer("memogram-rs", "todo"), now);
    }
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let title = parts.first().filter(|s| !s.is_empty()).copied().unwrap_or("New task");
    let note = parts.get(1).unwrap_or(&"");
    // Find or use default inbox project
    let projects = vikunja_list_projects(&app.vikunja_url, &app.vikunja_token).await.unwrap_or_default();
    let project_id = projects.first().and_then(|p| p["id"].as_u64()).unwrap_or(1);
    let project_name = projects.first().and_then(|p| p["title"].as_str()).unwrap_or("inbox");
    match vikunja_create_task(&app.vikunja_url, &app.vikunja_token, title, note, project_id, 0, "").await {
        Ok(task) => {
            let task_id = task["id"].as_u64().unwrap_or(0);
            let mut out = format!("{}\n\n", tg_header("✅", "Task Created", title));
            out.push_str(&format!("**Task:** `{}`\n**Project:** `{}`\n**Vikunja ID:** `#{}`\n\n", title, project_name, task_id));
            out.push_str(&format!("🔗 [Open in Vikunja]({}/projects/{}/tasks/{})\n\n", app.vikunja_url.trim_end_matches('/'), project_id, task_id));
            if !note.is_empty() {
                out.push_str(&format!("**Note:** {}\n\n", note));
            }
            out.push_str(&format!("{}\n\n`{}` · #todo #planning #memogram-rs", tg_footer("vikunja", "todo"), now));
            out
        }
        Err(e) => format!("❌ Vikunja error: {e}\n\n_Task not created._")
    }
}

async fn vikunja_deadline(args: &str, app: &App) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    if app.vikunja_url.is_empty() || app.vikunja_token.is_empty() {
        return format!("⚠️ _Vikunja not configured._").into();
    }
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let due = parts.first().filter(|s| !s.is_empty()).copied().unwrap_or("TBD");
    let title = parts.get(1).unwrap_or(&"Task");
    let projects = vikunja_list_projects(&app.vikunja_url, &app.vikunja_token).await.unwrap_or_default();
    let project_id = projects.first().and_then(|p| p["id"].as_u64()).unwrap_or(1);
    let due_date = if due.len() == 10 { due } else { "" };
    match vikunja_create_task(&app.vikunja_url, &app.vikunja_token, title, "", project_id, 3, due_date).await {
        Ok(task) => {
            let task_id = task["id"].as_u64().unwrap_or(0);
            let mut out = format!("{}\n\n", tg_header("⏰", "Deadline Task", title));
            out.push_str(&format!("**Task:** `{}`\n**Due:** `{}`\n**Priority:** 🟠 P3-High\n**Vikunja ID:** `#{}`\n\n", title, due, task_id));
            if let Ok(dt) = chrono::NaiveDate::parse_from_str(due_date, "%Y-%m-%d") {
                let now_date = Local::now().naive_local().date();
                let days_left = (dt - now_date).num_days();
                out.push_str(&format!("⏳ **{} days** until deadline\n\n", days_left));
            }
            out.push_str(&format!("🔗 [Open in Vikunja]({}/projects/{}/tasks/{})\n\n", app.vikunja_url.trim_end_matches('/'), project_id, task_id));
            out.push_str(&format!("{}\n\n`{}` · #deadline #planning #memogram-rs", tg_footer("vikunja", "deadline"), now));
            out
        }
        Err(e) => format!("❌ Vikunja error: {e}")
    }
}

async fn vikunja_priority(args: &str, app: &App) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    if app.vikunja_url.is_empty() || app.vikunja_token.is_empty() {
        return format!("⚠️ _Vikunja not configured._").into();
    }
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let level = parts.first().and_then(|s| s.parse::<u8>().ok()).unwrap_or(2).min(5);
    let title = parts.get(1).unwrap_or(&"Task");
    let (emoji, label) = vikunja_priority_label(level);
    let projects = vikunja_list_projects(&app.vikunja_url, &app.vikunja_token).await.unwrap_or_default();
    let project_id = projects.first().and_then(|p| p["id"].as_u64()).unwrap_or(1);
    match vikunja_create_task(&app.vikunja_url, &app.vikunja_token, title, "", project_id, level, "").await {
        Ok(task) => {
            let task_id = task["id"].as_u64().unwrap_or(0);
            let mut out = format!("{}\n\n", tg_header("🔥", "Priority Task", title));
            out.push_str(&format!("**Task:** `{}`\n**Priority:** {} `{}`\n**Vikunja ID:** `#{}`\n\n", title, emoji, label, task_id));
            out.push_str(&format!("🔗 [Open in Vikunja]({}/projects/{}/tasks/{})\n\n", app.vikunja_url.trim_end_matches('/'), project_id, task_id));
            out.push_str(&format!("{}\n\n`{}` · #priority #planning #memogram-rs", tg_footer("vikunja", "priority"), now));
            out
        }
        Err(e) => format!("❌ Vikunja error: {e}")
    }
}

async fn vikunja_goal(args: &str, app: &App) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let date = Local::now().format("%Y-%m-%d").to_string();
    if app.vikunja_url.is_empty() || app.vikunja_token.is_empty() {
        return format!("⚠️ _Vikunja not configured._").into();
    }
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let goal = parts.first().filter(|s| !s.is_empty()).copied().unwrap_or("New Goal");
    let details = parts.get(1).unwrap_or(&"");
    // Create a Vikunja project for the goal
    match vikunja_create_project(&app.vikunja_url, &app.vikunja_token, goal).await {
        Ok(project) => {
            let project_id = project["id"].as_u64().unwrap_or(0);
            // Create starter tasks
            let milestones = ["Research & plan", "First milestone", "Review & iterate"];
            let mut task_ids = Vec::new();
            for (i, m) in milestones.iter().enumerate() {
                let title = format!("{}. {}", i + 1, m);
                if let Ok(task) = vikunja_create_task(&app.vikunja_url, &app.vikunja_token, &title, "", project_id, 2, "").await {
                    task_ids.push(task["id"].as_u64().unwrap_or(0));
                }
            }
            let mut out = format!("{}\n\n", tg_header("🎯", "Goal", goal));
            out.push_str(&format!("**Goal:** `{}`\n**Date:** `{}`\n**Vikunja Project:** `#{}`\n\n", goal, date, project_id));
            if !details.is_empty() {
                out.push_str(&format!("**Details:** {}\n\n", details));
            }
            out.push_str("## 📋 Milestones\n\n");
            for (i, m) in milestones.iter().enumerate() {
                let tid = task_ids.get(i).unwrap_or(&0);
                out.push_str(&format!("- [ ] {} (task #{})\n", m, tid));
            }
            out.push_str(&format!("\n🔗 [Open in Vikunja]({}/projects/{})\n\n", app.vikunja_url.trim_end_matches('/'), project_id));
            out.push_str(&format!("{}\n\n`{}` · #goal #planning #memogram-rs", tg_footer("vikunja", "goal"), now));
            out
        }
        Err(e) => format!("❌ Vikunja project creation failed: {e}")
    }
}

async fn vikunja_project(args: &str, app: &App) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    if app.vikunja_url.is_empty() || app.vikunja_token.is_empty() {
        return format!("⚠️ _Vikunja not configured._").into();
    }
    let name = args.trim();
    if name.is_empty() { return "usage: `/project <name>`".into(); }
    match vikunja_create_project(&app.vikunja_url, &app.vikunja_token, name).await {
        Ok(project) => {
            let project_id = project["id"].as_u64().unwrap_or(0);
            let mut out = format!("{}\n\n", tg_header("📂", "Project Created", name));
            out.push_str(&format!("**Project:** `{}`\n**Vikunja ID:** `#{}`\n\n", name, project_id));
            out.push_str(&format!("🔗 [Open in Vikunja]({}/projects/{})\n\n", app.vikunja_url.trim_end_matches('/'), project_id));
            out.push_str(&format!("{}\n\n`{}` · #project #planning #memogram-rs", tg_footer("vikunja", "project"), now));
            out
        }
        Err(e) => format!("❌ Vikunja error: {e}")
    }
}

async fn vikunja_weekly(app: &App) -> String {
    let now = Local::now();
    let today = now.format("%Y-%m-%d").to_string();
    let week_start = (now - chrono::Duration::days(now.weekday().num_days_from_monday() as i64)).format("%Y-%m-%d").to_string();
    let mut out = format!("{}\n\n", tg_header("📅", "Weekly Review", &format!("{} → {}", week_start, today)));

    if app.vikunja_url.is_empty() || app.vikunja_token.is_empty() {
        out.push_str("_Vikunja not configured — showing template only._\n\n");
        out.push_str("## ✅ Completed This Week\n\n- [ ] \n\n");
        out.push_str("## ⏳ Still Open\n\n- [ ] \n\n");
        out.push_str("## 💡 Reflection\n\n- What went well?\n- What needs adjustment?\n\n");
        out.push_str(&format!("{}\n\n`{}` · #weekly #planning #memogram-rs", tg_footer("memogram-rs", "weekly"), now.format("%Y-%m-%d %H:%M")));
        return out;
    }

    let projects = vikunja_list_projects(&app.vikunja_url, &app.vikunja_token).await.unwrap_or_default();
    let mut total_done = 0u64;
    let mut total_open = 0u64;
    let mut all_open: Vec<String> = Vec::new();
    let mut all_done: Vec<String> = Vec::new();

    for p in &projects {
        let pid = p["id"].as_u64().unwrap_or(0);
        let pname = p["title"].as_str().unwrap_or("?");
        let done_tasks = vikunja_list_tasks(&app.vikunja_url, &app.vikunja_token, pid, Some(true)).await.unwrap_or_default();
        let open_tasks = vikunja_list_tasks(&app.vikunja_url, &app.vikunja_token, pid, Some(false)).await.unwrap_or_default();
        total_done += done_tasks.len() as u64;
        total_open += open_tasks.len() as u64;
        for t in &open_tasks {
            let title = t["title"].as_str().unwrap_or("?");
            let due = t["due_date"].as_str().map(|d| if d.len() >= 10 { &d[..10] } else { "?" }).unwrap_or("");
            let (emoji, _) = vikunja_priority_label(t["priority"].as_u64().unwrap_or(0) as u8);
            all_open.push(format!("- {} {} (due: {}, project: {})", emoji, title, if due.is_empty() { "none" } else { due }, pname));
        }
        for t in &done_tasks {
            let title = t["title"].as_str().unwrap_or("?");
            all_done.push(format!("- ~~{}~~ ✅ ({})", title, pname));
        }
    }

    out.push_str(&format!("**{} completed** · **{} open** tasks across {} projects\n\n", total_done, total_open, projects.len()));

    out.push_str("## ✅ Completed\n\n");
    if all_done.is_empty() { out.push_str("_None this week._\n\n"); }
    else { out.push_str(&format!("{}\n\n", all_done.join("\n"))); }

    out.push_str("## ⏳ Still Open\n\n");
    if all_open.is_empty() { out.push_str("_All clear!_\n\n"); }
    else { out.push_str(&format!("{}\n\n", all_open.join("\n"))); }

    out.push_str("## 💡 Reflection\n\n");
    out.push_str("- What went well this week?\n");
    out.push_str("- What needs adjustment?\n");
    out.push_str("- Top priority for next week?\n\n");

    out.push_str(&format!("🔗 [Vikunja Dashboard]({})\n\n", app.vikunja_url.trim_end_matches('/')));
    out.push_str(&format!("{}\n\n`{}` · #weekly #planning #memogram-rs", tg_footer("vikunja", "weekly"), now.format("%Y-%m-%d %H:%M")));
    out
}

// === OLD PLANNING (kept for reference but unreachable) ===

fn create_goal(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let date = Local::now().format("%Y-%m-%d").to_string();
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let goal = parts.first().unwrap_or(&"Untitled");
    let details = parts.get(1).unwrap_or(&"");
    // SMART scoring heuristic
    let goal_lower = goal.to_lowercase();
    let mut smart_score = 0;
    let mut smart_notes = Vec::new();
    // Specific: check for concrete nouns/verbs
    if goal_lower.len() > 10 { smart_score += 1; smart_notes.push("✅ Specific — descriptive goal"); }
    else { smart_notes.push("❌ Specific — add more detail"); }
    // Measurable: check for numbers, percentages, counts
    if goal_lower.chars().any(|c| c.is_ascii_digit()) || goal_lower.contains('%') || goal_lower.contains("number") || goal_lower.contains("count") {
        smart_score += 1; smart_notes.push("✅ Measurable — has numeric target");
    } else { smart_notes.push("❌ Measurable — add a number or %"); }
    // Achievable: check for "learn", "build", "run" vs impossible-sounding
    if goal_lower.contains("learn") || goal_lower.contains("build") || goal_lower.contains("run") || goal_lower.contains("finish") || goal_lower.contains("complete") || goal_lower.contains("create") {
        smart_score += 1; smart_notes.push("✅ Achievable — action-oriented");
    } else { smart_notes.push("⚠️ Achievable — use action verbs (learn, build, finish)"); }
    // Relevant: always check if details provided
    if !details.is_empty() { smart_score += 1; smart_notes.push("✅ Relevant — context provided"); }
    else { smart_notes.push("❌ Relevant — add why this matters"); }
    // Time-bound: check for date keywords
    if goal_lower.contains("by") || goal_lower.contains("before") || goal_lower.contains("end of") || goal_lower.contains("week") || goal_lower.contains("month") || goal_lower.contains("year") || goal_lower.contains("day") {
        smart_score += 1; smart_notes.push("✅ Time-bound — has deadline hint");
    } else { smart_notes.push("❌ Time-bound — add 'by <date>'"); }
    let bar = "█".repeat(smart_score) + &"░".repeat(5 - smart_score);
    let grade = match smart_score {
        5 => "🟢 Perfect",
        4 => "🟢 Strong",
        3 => "🟡 Good",
        2 => "🟠 Weak",
        _ => "🔴 Vague",
    };
    let mut out = format!("{}\n\n", tg_header("🎯", "Goal", goal));
    out.push_str(&format!("**Goal:** `{}`\n**Set:** `{}`\n**Details:** {}\n\n", goal, date, if details.is_empty() { "—" } else { details }));
    out.push_str(&format!("## 📊 SMART Score\n\n| {} | `{}/5` {} |\n\n", bar, smart_score, grade));
    out.push_str("## 🔍 Assessment\n\n");
    for note in &smart_notes {
        out.push_str(&format!("- {}\n", note));
    }
    out.push_str("\n## ✅ Milestones\n\n- [ ] \n- [ ] \n- [ ] \n\n");
    out.push_str("## 📅 Timeline\n\n| Milestone | Target | Done |\n|---|---|---|\n|  |  |  |\n\n");
    out.push_str("> _Tip: Make it SMART — Specific, Measurable, Achievable, Relevant, Time-bound._\n\n");
    out.push_str(&format!("{}\n\n`{}` · #goal", tg_footer("memogram", "goal"), now));
    out
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

async fn fetch_save(args: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let content = args.trim();
    if content.is_empty() {
        return Ok(format!("{}\n\n_Usage:_ `/save <url or text>`\n\n{}", tg_header("💾", "Save", "help"), tg_footer("memogram", "save")));
    }
    // Check if it's a URL
    let is_url = content.starts_with("http://") || content.starts_with("https://");
    let mut out = format!("{}\n\n", tg_header("💾", "Saved", &content.chars().take(40).collect::<String>()));
    if is_url {
        out.push_str(&format!("**URL:** `{}`\n\n", content));
        // Try to fetch page title + description
        if let Ok(resp) = HTTP.get(content).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await {
            if let Ok(html) = resp.text().await {
                let title = html.split("<title>").nth(1).and_then(|s| s.split("</title>").next()).unwrap_or("").trim();
                let desc = html.split("meta").find(|m| m.contains("description")).and_then(|m| {
                    m.split("content=\"").nth(1)?.split('"').next()
                }).unwrap_or("").trim();
                let og_image = html.split("meta").find(|m| m.contains("og:image")).and_then(|m| {
                    m.split("content=\"").nth(1)?.split('"').next()
                }).unwrap_or("").trim();
                if !title.is_empty() {
                    out.push_str(&format!("**Title:** {}\n", title));
                }
                if !desc.is_empty() {
                    out.push_str(&format!("**Description:** {}\n", desc.chars().take(200).collect::<String>()));
                }
                if !og_image.is_empty() && og_image.starts_with("http") {
                    out.push_str(&format!("\n![Preview]({})\n", og_image));
                }
                out.push('\n');
            }
        }
    } else {
        out.push_str(&format!("**Content:** {}\n\n", content));
    }
    out.push_str("## 🏷️ Tags\n\n- #save #inbox\n\n");
    out.push_str("## ✅ Actions\n\n- [ ] Process\n- [ ] Archive\n\n");
    out.push_str(&format!("{}\n\n`{}` · #save", tg_footer("memogram", "save"), now));
    Ok(out)
}

// === DAILY COMMANDS ===

fn create_morning(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let date = Local::now().format("%Y-%m-%d").to_string();
    let intent = if args.trim().is_empty() { "Set 1 intent for today." } else { args };
    Md::new()
        .h2("🌅 Morning")
        .blank()
        .pi("Date", &date)
        .pi("Time", &now)
        .blank()
        .push("## 🎯 Intent")
        .blank()
        .push(intent)
        .blank()
        .push("## ✅ Top 3")
        .blank()
        .push("- [ ] ")
        .push("- [ ] ")
        .push("- [ ] ")
        .blank()
        .push("## 💧 Health")
        .blank()
        .table(&["Metric", "Value"], &[
            vec!["Sleep".into(), "".into()],
            vec!["Water".into(), "".into()],
            vec!["Energy".into(), "/10".into()],
        ])
        .blank()
        .push("## 💡 Morning Tips")
        .blank()
        .push("- Hydrate before coffee")
        .push("- 5m sunlight for circadian rhythm")
        .push("- Review top 3 priorities")
        .blank()
        .push(&format!("{}\n\n`{}` · #morning #daily", tg_footer("memogram", "morning"), now))
        .build()
}

fn create_evening(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let date = Local::now().format("%Y-%m-%d").to_string();
    Md::new()
        .h2("🌙 Evening")
        .blank()
        .pi("Date", &date)
        .pi("Time", &now)
        .blank()
        .push("## 📝 Reflection")
        .blank()
        .push(if args.trim().is_empty() { "How was your day?" } else { args })
        .blank()
        .push("## ✅ Wins")
        .blank()
        .push("- ")
        .blank()
        .push("## 🔧 Improvements")
        .blank()
        .push("- ")
        .blank()
        .push("## 🙏 Gratitude")
        .blank()
        .push("- ")
        .blank()
        .push("## 💡 Evening Tips")
        .blank()
        .push("- Review wins from today")
        .push("- Set 1 intention for tomorrow")
        .push("- Avoid screens 1h before bed")
        .blank()
        .push(&format!("{}\n\n`{}` · #evening #daily", tg_footer("memogram", "evening"), now))
        .build()
}

fn create_checkin(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let parts: Vec<&str> = args.splitn(3, ' ').collect();
    let mood = parts.first().unwrap_or(&"?");
    let energy = parts.get(1).unwrap_or(&"?");
    let note = parts.get(2).unwrap_or(&"");
    Md::new()
        .h2("✅ Check-in")
        .blank()
        .pi("Time", &now)
        .pi("Mood", mood)
        .pi("Energy", energy)
        .blank()
        .push("## 💭 State")
        .blank()
        .push(if note.is_empty() { "How are you feeling?" } else { note })
        .blank()
        .push("## 📊 Quick Check")
        .blank()
        .table(&["Metric", "Value"], &[
            vec!["Mood".into(), mood.to_string()],
            vec!["Energy".into(), format!("{}/10", energy)],
            vec!["Time".into(), now.clone()],
        ])
        .blank()
        .push("## 💡 Tips")
        .blank()
        .push("- 1 breath, note 1 win")
        .push("- Name it to tame it")
        .push("- Hydrate + stretch")
        .blank()
        .push(&format!("{}\n\n`{}` · #checkin #daily", tg_footer("memogram", "checkin"), now))
        .build()
}

fn create_log(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let date = Local::now().format("%Y-%m-%d").to_string();
    let word_count = args.split_whitespace().count();
    let char_count = args.len();
    Md::new()
        .h2("📋 Daily Log")
        .blank()
        .pi("Date", &date)
        .pi("Words", &word_count.to_string())
        .pi("Chars", &char_count.to_string())
        .blank()
        .push("## 📝 Entry")
        .blank()
        .push(args)
        .blank()
        .push("## 📊 Stats")
        .blank()
        .table(&["Metric", "Value"], &[
            vec!["Words".into(), word_count.to_string()],
            vec!["Characters".into(), char_count.to_string()],
            vec!["Lines".into(), args.lines().count().to_string()],
        ])
        .blank()
        .push("## 🏷️ Tags")
        .blank()
        .push("- #daily")
        .blank()
        .push(&format!("{}\n\n`{}` · #log #daily", tg_footer("memogram", "log"), now))
        .build()
}

// === LIFE COMMANDS ===

fn create_sleep(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let date = Local::now().format("%Y-%m-%d").to_string();
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let hours = parts.first().unwrap_or(&"?");
    let quality = parts.get(1).unwrap_or(&"");
    let hrs: f64 = hours.parse().unwrap_or(0.0);
    let score = if hrs >= 8.0 { "Excellent" } else if hrs >= 7.0 { "Good" } else if hrs >= 6.0 { "Fair" } else { "Poor" };
    let emoji = if hrs >= 8.0 { "🟢" } else if hrs >= 7.0 { "🟡" } else if hrs >= 6.0 { "🟠" } else { "🔴" };
    Md::new()
        .h2("😴 Sleep")
        .blank()
        .pi("Date", &date)
        .pi("Hours", hours)
        .pi("Quality", quality)
        .pi("Score", &format!("{} {}", emoji, score))
        .blank()
        .push("## 📊 Sleep Analysis")
        .blank()
        .table(&["Metric", "Value"], &[
            vec!["Hours".into(), hours.to_string()],
            vec!["Quality".into(), quality.to_string()],
            vec!["Score".into(), format!("{} {}", emoji, score)],
            vec!["Target".into(), "7-9 hours".into()],
        ])
        .blank()
        .push("## 💡 Tips")
        .blank()
        .push(&if hrs >= 8.0 {
            "- Great sleep! Keep this schedule consistent."
        } else if hrs >= 7.0 {
            "- Good sleep. Try for 8h for optimal recovery."
        } else if hrs >= 6.0 {
            "- Below target. Avoid screens 1h before bed."
        } else {
            "- Sleep deficit! Prioritize rest tonight."
        })
        .blank()
        .push(&format!("{}\n\n`{}` · #sleep #wellness", tg_footer("memogram", "sleep"), now))
        .build()
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
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let date = Local::now().format("%Y-%m-%d").to_string();
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let activity = parts.first().unwrap_or(&"run");
    let duration_str = parts.get(1).unwrap_or(&"30m");
    // Parse duration to minutes
    let mins: f64 = if let Some(m) = duration_str.strip_suffix('m') {
        m.parse().unwrap_or(30.0)
    } else if let Some(h) = duration_str.strip_suffix('h') {
        h.parse().unwrap_or(1.0) * 60.0
    } else {
        duration_str.parse().unwrap_or(30.0)
    };
    // MET values (Compendium of Physical Activities)
    let met: f64 = match activity.to_lowercase().as_str() {
        "run" | "running" | "jog" | "jogging" => 9.8,
        "walk" | "walking" | "hike" | "hiking" => 3.5,
        "bike" | "cycling" | "bicycle" => 7.5,
        "swim" | "swimming" => 6.0,
        "yoga" | "pilates" => 3.0,
        "weight" | "weights" | "strength" | "lifting" | "gym" => 6.0,
        "hiit" | "tabata" => 8.0,
        "stretch" | "stretching" | "flexibility" => 2.5,
        "dance" | "dancing" => 5.0,
        "row" | "rowing" => 7.0,
        "climb" | "climbing" | "boulder" => 8.0,
        "box" | "boxing" | "mma" | "kickbox" => 10.0,
        "tennis" | "squash" | "racquetball" => 7.3,
        "soccer" | "football" | "basketball" | "basket" => 8.0,
        "surf" | "surfing" | "paddle" | "paddleboard" => 5.0,
        "ski" | "skiing" | "snowboard" => 6.8,
        "elliptical" => 5.0,
        "jump rope" | "jumping rope" | "skip" => 12.0,
        "rest" | "rest day" | "off" => 0.0,
        _ => 4.0, // default moderate activity
    };
    // Calories = MET × weight(kg) × time(hrs); assume 70kg
    let weight = 70.0_f64;
    let calories = (met * weight * (mins / 60.0)) as i64;
    let bar = "█".repeat(((mins / 90.0 * 10.0) as usize).clamp(0, 10)) + &"░".repeat(10 - ((mins / 90.0 * 10.0) as usize).clamp(0, 10));
    let intensity = if met >= 8.0 { "🔴 Vigorous" } else if met >= 5.0 { "🟡 Moderate" } else if met >= 2.5 { "🟢 Light" } else { "⚪ Rest" };
    let mut out = format!("{}\n\n", tg_header("🏋️", "Exercise", activity));
    out.push_str(&format!("**Activity:** `{}` · **Duration:** `{}`\n**Date:** `{}`\n\n", activity, duration_str, date));
    out.push_str(&format!("## 📊 Session\n\n| Metric | Value |\n|---|---|\n| Activity | `{}` |\n| Duration | `{:.0} min` |\n| MET | `{:.1}` |\n| Intensity | {} |\n| Est. Calories | `~{} kcal` |\n| Weight (est) | `70 kg` |\n\n", activity, mins, met, intensity, calories));
    out.push_str(&format!("## 📈 Duration\n\n| {} | {} min |\n\n", bar, mins as i64));
    out.push_str(&format!("## 📋 MET Reference\n\n| Activity | MET | Cal/30min |\n|---|---|---|\n| Running | `9.8` | `~343` |\n| Cycling | `7.5` | `~263` |\n| Swimming | `6.0` | `~210` |\n| Weights | `6.0` | `~210` |\n| Yoga | `3.0` | `~105` |\n| Walking | `3.5` | `~123` |\n\n"));
    out.push_str("## 💡 Tips\n\n> _Progressive overload + 48h rest per muscle group. Hydrate + protein within 60m._\n\n");
    out.push_str(&format!("{}\n\n`{}` · #exercise", tg_footer("compendium of physical activities", "exercise"), now));
    out
}

fn create_water(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let amount = parts.first().unwrap_or(&"500ml");
    let note = parts.get(1).unwrap_or(&"");
    let _date = Local::now().format("%Y-%m-%d").to_string();
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
    let _date = Local::now().format("%Y-%m-%d").to_string();
    let lvl: i32 = level.parse().unwrap_or(5);
    let bar = "█".repeat((lvl as usize).clamp(0, 10)) + &"░".repeat(10 - (lvl as usize).clamp(0, 10));
    format!(
        "# 😰 Stress — `{}`\n\n**Date:** `{}` · **Level:** `{}/10` {}\n**Note:** {}\n\n## 📊 Assessment\n\n| Level | Bar | Status |\n|---|---|---|\n| {}/10 | {} | {} |\n\n## 📈 Trend (sample)\n\n```mermaid\nxychart-beta\n  title \"Stress Last 7d\"\n  x-axis [Mon Tue Wed Thu Fri Sat Sun]\n  y-axis \"Level\" 0 10\n  bar [3 4 6 5 7 4 {}]\n```\n\n## 💡 Coping\n> _Tip: Box breathing 4-4-4-4 • 10m walk • 3 gratitudes • no screens before bed._\n\n{}\n\n`{}` · #{}",
        level, now, level, bar, note, level, bar, if lvl >= 7 { "🔴 High" } else if lvl >= 4 { "🟡 Medium" } else { "🟢 Low" }, lvl, tg_header("😰", "Stress", &format!("{}/10", level)), now, "stress"
    )
}

async fn fetch_read(args: &str) -> Result<String> {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    let query = parts.first().filter(|s| !s.is_empty()).copied().unwrap_or("rust");
    let note = parts.get(1).unwrap_or(&"");
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let date = Local::now().format("%Y-%m-%d").to_string();
    // Fetch from Open Library
    let search_url = format!("https://openlibrary.org/search.json?title={}&limit=1", urlencoding::encode(query));
    let v: serde_json::Value = HTTP.get(&search_url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    let docs = v["docs"].as_array().cloned().unwrap_or_default();
    let mut out = format!("{}\n\n", tg_header("📚", "Reading", query));
    if let Some(first) = docs.first() {
        let title = first["title"].as_str().unwrap_or(query);
        let author_name = first["author_name"].as_array().and_then(|a| a.first()).and_then(|v| v.as_str()).unwrap_or("Unknown");
        let year = first["first_publish_year"].as_i64().map(|y| y.to_string()).unwrap_or_else(|| "—".into());
        let pages = first["number_of_pages_median"].as_i64().map(|p| p.to_string()).unwrap_or_else(|| "—".into());
        let subjects = first["subject"].as_array()
            .map(|a| a.iter().take(5).filter_map(|s| s.as_str()).collect::<Vec<&str>>().join(", "))
            .unwrap_or_default();
        let cover_i = first["cover_i"].as_i64().unwrap_or(0);
        out.push_str(&format!("**Title:** `{}`\n**Author:** `{}` · **Year:** `{}` · **Pages:** `{}`\n\n", title, author_name, year, pages));
        if !subjects.is_empty() {
            out.push_str(&format!("## 🏷️ Subjects\n\n{}\n\n", subjects));
        }
        if cover_i > 0 {
            out.push_str(&format!("![Cover](https://covers.openlibrary.org/b/id/{}-M.jpg)\n\n", cover_i));
        }
        out.push_str(&format!("🔗 [Open Library](https://openlibrary.org{})\n\n", first["key"].as_str().unwrap_or("")));
    } else {
        out.push_str(&format!("**Title:** `{}`\n\n", query));
    }
    out.push_str(&format!("**Started:** `{}`\n**Status:** 📖 Reading\n\n", date));
    out.push_str("## 📝 Summary\n\n- \n\n## 💡 Takeaways\n\n1. \n2. \n3. \n\n## 💬 Quotes\n\n> \"\" \n\n## 📊 Progress\n\n| Pages | % | Notes |\n|---|---|---|\n|  |  |  |\n\n");
    if !note.is_empty() {
        out.push_str(&format!("## 📌 Note\n\n{}\n\n", note));
    }
    out.push_str(&format!("{}\n\n`{}` · #reading", tg_footer("openlibrary.org", "reading"), now));
    Ok(out)
}

// ============= NEW COMMANDS =============

async fn fetch_ghrepo(repo: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let url = format!("https://api.github.com/repos/{}", repo.trim());
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "memogram-rs").header("Accept", "application/vnd.github.v3+json").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    let name = v["full_name"].as_str().unwrap_or(repo);
    let desc = v["description"].as_str().unwrap_or("No description");
    let stars = v["stargazers_count"].as_u64().unwrap_or(0);
    let forks = v["forks_count"].as_u64().unwrap_or(0);
    let issues = v["open_issues_count"].as_u64().unwrap_or(0);
    let lang = v["language"].as_str().unwrap_or("Unknown");
    let license = v["license"]["spdx_id"].as_str().unwrap_or("None");
    let created = v["created_at"].as_str().unwrap_or("?").chars().take(10).collect::<String>();
    let updated = v["updated_at"].as_str().unwrap_or("?").chars().take(10).collect::<String>();
    let topics: Vec<String> = v["topics"].as_array().map(|t| t.iter().take(5).filter_map(|x| x.as_str()).map(|s| format!("`{}`", s)).collect()).unwrap_or_default();
    let mut out = format!("{}\n\n", tg_header("📦", "GitHub Repo", name));
    out.push_str(&format!("**{}**\n\n{}\n\n", name, desc));
    out.push_str(&format!("| Stat | Value |\n|---|---|\n| ⭐ Stars | `{}` |\n| 🍴 Forks | `{}` |\n| 🐛 Issues | `{}` |\n| 💻 Language | `{}` |\n| 📜 License | `{}` |\n| 📅 Created | `{}` |\n| 🔄 Updated | `{}` |\n\n", stars, forks, issues, lang, license, created, updated));
    if !topics.is_empty() { out.push_str(&format!("### 🏷️ Topics\n\n{}\n\n", topics.join(" · "))); }
    out.push_str(&format!("🔗 [View on GitHub](https://github.com/{})\n\n", repo.trim()));
    out.push_str(&format!("{}\n\n`{}` · #ghrepo", tg_footer("api.github.com", "ghrepo"), now));
    Ok(out)
}

async fn fetch_ip(ip: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let url = if ip.trim().is_empty() || ip.trim() == "me" { "http://ip-api.com/json/".to_string() } else { format!("http://ip-api.com/json/{}", ip.trim()) };
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    let status = v["status"].as_str().unwrap_or("fail");
    if status != "success" { return Ok(format!("{}\n\n_Lookup failed for `{}`_\n\n{}", tg_header("🌐", "IP Lookup", "error"), ip, tg_footer("ip-api.com", "ip"))); }
    let query = v["query"].as_str().unwrap_or("?");
    let country = v["country"].as_str().unwrap_or("?");
    let region = v["regionName"].as_str().unwrap_or("?");
    let city = v["city"].as_str().unwrap_or("?");
    let isp = v["isp"].as_str().unwrap_or("?");
    let org = v["org"].as_str().unwrap_or("?");
    let asname = v["as"].as_str().unwrap_or("?");
    let lat = v["lat"].as_f64().unwrap_or(0.0);
    let lon = v["lon"].as_f64().unwrap_or(0.0);
    let timezone = v["timezone"].as_str().unwrap_or("?");
    let mut out = format!("{}\n\n", tg_header("🌐", "IP Lookup", query));
    out.push_str(&format!("**IP:** `{}`\n\n", query));
    out.push_str(&format!("## 📍 Location\n\n| Property | Value |\n|---|---|\n| Country | `{}` |\n| Region | `{}` |\n| City | `{}` |\n| Coordinates | `{}, {}` |\n| Timezone | `{}` |\n\n", country, region, city, lat, lon, timezone));
    out.push_str(&format!("## 🌐 Network\n\n| Property | Value |\n|---|---|\n| ISP | `{}` |\n| Organization | `{}` |\n| AS | `{}` |\n\n", isp, org, asname));
    out.push_str(&format!("🔗 [View on map](https://www.google.com/maps?q={},{}{}\n\n", lat, lon, ")"));
    out.push_str(&format!("{}\n\n`{}` · #ip", tg_footer("ip-api.com", "ip"), now));
    Ok(out)
}

async fn fetch_zen() -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let v: serde_json::Value = HTTP.get("https://zenquotes.io/api/random").header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    if let Some(arr) = v.as_array() {
        if let Some(first) = arr.first() {
            let quote = first["q"].as_str().unwrap_or("The journey of a thousand miles begins with a single step.");
            let author = first["a"].as_str().unwrap_or("Lao Tzu");
            let char_count = quote.len();
            let word_count = quote.split_whitespace().count();
            return Ok(Md::new()
                .h2("🧘 Zen Wisdom")
                .blank()
                .pi("Author", author)
                .pi("Words", &word_count.to_string())
                .pi("Source", "zenquotes.io")
                .blank()
                .push("## 📜 Quote")
                .blank()
                .quote(quote)
                .blank()
                .push(&format!("— **{}**", author))
                .blank()
                .push("## 📊 Quote Stats")
                .blank()
                .table(&["Metric", "Value"], &[
                    vec!["Words".into(), word_count.to_string()],
                    vec!["Characters".into(), char_count.to_string()],
                    vec!["Source".into(), "zenquotes.io".into()],
                ])
                .blank()
                .push("## 🧘 Zen Principles")
                .blank()
                .push("- **Mindfulness** — Be present in this moment")
                .push("- **Acceptance** — Embrace what is, not what should be")
                .push("- **Simplicity** — Find peace in less")
                .blank()
                .push(&format!("{}\n\n`{}` · #zen #wellness", tg_footer("zenquotes.io", "zen"), now))
                .build());
        }
    }
    Ok(Md::new()
        .h2("🧘 Zen Wisdom")
        .blank()
        .pi("Author", "Alan Watts")
        .pi("Source", "zenquotes.io")
        .blank()
        .push("## 📜 Quote")
        .blank()
        .quote("The only way to make sense out of change is to plunge into it, move with it, and join the dance.")
        .blank()
        .push("— **Alan Watts**")
        .blank()
        .push("## 🧘 Zen Principles")
        .blank()
        .push("- **Mindfulness** — Be present in this moment")
        .push("- **Acceptance** — Embrace what is, not what should be")
        .push("- **Simplicity** — Find peace in less")
        .blank()
        .push(&format!("{}\n\n`{}` · #zen #wellness", tg_footer("zenquotes.io", "zen"), now))
        .build())
}

async fn fetch_summarize(url: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let resp = HTTP.get(url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(10)).send().await?;
    let html = resp.text().await.unwrap_or_default();
    let title = html.split("<title>").nth(1).and_then(|s| s.split("</title>").next()).unwrap_or("No title").trim();
    let body = html.split("<body>").nth(1).unwrap_or(&html);
    let text: String = body.chars().filter(|c| !c.is_control() || *c == '\n').collect();
    let sentences: Vec<&str> = text.split(|c| c == '.' || c == '!' || c == '?').filter(|s| s.len() > 30 && s.len() < 200).take(5).collect();
    let mut out = format!("{}\n\n", tg_header("📄", "Page Summary", title));
    out.push_str(&format!("**Title:** {}\n**URL:** `{}`\n\n", title, url.chars().take(50).collect::<String>()));
    if !sentences.is_empty() {
        out.push_str("## 📝 Summary\n\n");
        for s in &sentences { out.push_str(&format!("- {}\n", s.trim())); }
        out.push('\n');
    } else {
        out.push_str("_Could not extract meaningful content from this page._\n\n");
    }
    out.push_str(&format!("🔗 [Read full article]({})\n\n", url));
    out.push_str(&format!("{}\n\n`{}` · #summarize", tg_footer("memogram", "summarize"), now));
    Ok(out)
}

fn fetch_bmi(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let parts: Vec<&str> = args.split_whitespace().collect();
    if parts.len() < 2 { return format!("{}\n\n_Usage:_ `/bmi <height_cm> <weight_kg>`\n\n**Example:** `/bmi 175 70`\n\n`{}` · #bmi", tg_header("⚖️", "BMI Calculator", "help"), now); }
    let height: f64 = match parts[0].parse() { Ok(v) => v, Err(_) => return format!("{}\n\n_Invalid height: `{}`_\n\n{}", tg_header("⚖️", "BMI Calculator", "error"), parts[0], now) };
    let weight: f64 = match parts[1].parse() { Ok(v) => v, Err(_) => return format!("{}\n\n_Invalid weight: `{}`_\n\n{}", tg_header("⚖️", "BMI Calculator", "error"), parts[1], now) };
    let height_m = height / 100.0;
    let bmi = weight / (height_m * height_m);
    let category = if bmi < 18.5 { "Underweight" } else if bmi < 25.0 { "Normal weight" } else if bmi < 30.0 { "Overweight" } else { "Obese" };
    let emoji = if bmi < 18.5 { "🟡" } else if bmi < 25.0 { "🟢" } else if bmi < 30.0 { "🟠" } else { "🔴" };
    let ideal_min = 18.5 * height_m * height_m;
    let ideal_max = 24.9 * height_m * height_m;
    let mut out = format!("{}\n\n", tg_header(&emoji, "BMI Calculator", &format!("{:.1}", bmi)));
    out.push_str(&format!("**Height:** `{}` cm · **Weight:** `{}` kg\n\n", height, weight));
    out.push_str(&format!("## 📊 Results\n\n| Metric | Value |\n|---|---|\n| BMI | `{:.1}` |\n| Category | {} {} |\n| Ideal Range | `{:.1} - {:.1} kg` |\n\n", bmi, emoji, category, ideal_min, ideal_max));
    out.push_str(&format!("## 📚 BMI Categories\n\n| BMI | Category |\n|---|---|\n| < 18.5 | 🟡 Underweight |\n| 18.5 - 24.9 | 🟢 Normal weight |\n| 25.0 - 29.9 | 🟠 Overweight |\n| ≥ 30.0 | 🔴 Obese |\n\n"));
    out.push_str(&format!("{}\n\n`{}` · #bmi #wellness", tg_footer("memogram", "bmi"), now));
    out
}

async fn run_preview() -> Result<()> {
    let out_dir = std::env::temp_dir().join("memogram-preview").join("live");
    let _ = std::fs::create_dir_all(&out_dir);
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
        ("brief", try_fetch("brief", fetch_brief("rust async")).await.1),
        ("compare", try_fetch("compare", fetch_compare("vim vs emacs")).await.1),
        ("paper", try_fetch("paper", fetch_paper("diffusion transformers")).await.1),
        ("tutorial", try_fetch("tutorial", fetch_tutorial("git rebase")).await.1),
        ("gh", try_fetch("gh", fetch_gh("rust")).await.1),
        ("fx", try_fetch("fx", fetch_fx("USD-KRW")).await.1),
        ("stock", try_fetch("stock", fetch_stock("AAPL")).await.1),
        ("crypto", try_fetch("crypto", fetch_crypto("bitcoin")).await.1),
        ("translate", try_fetch("translate", fetch_translate("hello world")).await.1),
        ("npm", try_fetch("npm", fetch_npm("express")).await.1),
        ("pypi", try_fetch("pypi", fetch_pypi("requests")).await.1),
        ("crates", try_fetch("crates", fetch_crates("tokio")).await.1),
        ("stackoverflow", try_fetch("stackoverflow", fetch_stackoverflow("rust async")).await.1),
        ("docker", try_fetch("docker", fetch_docker("nginx")).await.1),
        ("airquality", try_fetch("airquality", fetch_airquality("Beijing")).await.1),
        ("sunrise", try_fetch("sunrise", fetch_sunrise("34.1706,-118.8376")).await.1),
        ("synonym", try_fetch("synonym", fetch_synonym("happy")).await.1),
        ("philosophy", try_fetch("philosophy", fetch_philosophy_quote()).await.1),
        ("finance", try_fetch("finance", fetch_finance("inflation")).await.1),
        ("trial", try_fetch("trial", fetch_trial("diabetes")).await.1),
        ("food", try_fetch("food", fetch_food("apple")).await.1),
        ("pubmed", try_fetch("pubmed", fetch_pubmed("CRISPR")).await.1),
        ("drug", try_fetch("drug", fetch_drug("aspirin")).await.1),
        ("arxiv", try_fetch("arxiv", fetch_arxiv("quantum")).await.1),
        ("devto", try_fetch("devto", fetch_devto()).await.1),
        ("tldr", try_fetch("tldr", fetch_tldr()).await.1),
        ("markets", try_fetch("markets", fetch_markets()).await.1),
        ("hustle", try_fetch("hustle", fetch_hustle("python")).await.1),
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
        ("read", try_fetch("read", fetch_read("Dune Frank Herbert")).await.1),
        ("compound", create_compound("1000 7% 10")),
        ("stress", create_stress("6 work deadline")),
        ("flag", create_flag("Follow up on beat collab")),
        ("archive", create_archive("Old meeting notes")),
        ("move", create_move("wellness Move to wellness bucket")),
        ("wind", try_fetch("wind", fetch_wind("Thousand Oaks, CA")).await.1),
        ("uv", try_fetch("uv", fetch_uv("Thousand Oaks, CA")).await.1),
        ("pollen", try_fetch("pollen", fetch_pollen("Thousand Oaks, CA")).await.1),
        ("moon", try_fetch("moon", fetch_moon("")).await.1),
        ("tide", try_fetch("tide", fetch_tide("Santa Monica, CA")).await.1),
        ("snow", try_fetch("snow", fetch_snow("Mammoth Lakes, CA")).await.1),
        // NEW: missing template commands
        ("color", fetch_color("#FF5733")),
        ("math", eval_math("2+2*3")),
        ("meeting", create_meeting("Sprint planning 10am discuss Q3 goals and blockers")),
        ("project", create_project("Memogram v2 — Telegram bot for Memos")),
        ("book", try_fetch("book", fetch_book("Dune by Frank Herbert")).await.1),
        ("todo", create_todo("Fix weather API, deploy v2, write tests")),
        ("list", create_list("Groceries: milk, eggs, bread, coffee")),
        ("clip", create_clip("https://example.com article about rust async")),
        ("mood", create_mood_entry("7 productive day, shipped features")),
        ("habit", create_habit_entry("meditation 10m done, reading 20m done")),
        ("wisdom", fetch_wisdom().await.unwrap_or_else(|e| format!("wisdom err: {e}"))),
        ("review", create_review("Shipped 3 features, fixed 2 bugs, reviewed 4 PRs")),
        ("priority", create_priority("1. Deploy v2 2. Fix weather 3. Write docs")),
        ("link", create_link("https://github.com/rust-lang/rust The Rust programming language")),
        ("snippet", create_snippet("fn main() { println!(\"hello\"); }")),
        ("evening", create_evening("Reflection: good day, ship done, plans for tomorrow set")),
        ("checkin", create_checkin("7 mood, 8 energy, slept well")),
        ("log", create_log("ran 5k in 25m, 150bpm avg")),
        ("summary", create_summary("Week 36: shipped memogram v2, fixed 12 bugs")),
        ("genome", try_fetch("genome", fetch_genome("BRCA1")).await.1),
        ("protein", try_fetch("protein", fetch_protein("insulin")).await.1),
        ("containers", try_fetch("containers", fetch_containers("http://localhost:6100")).await.1),
        // REPLACED COMMANDS
        ("ghrepo", try_fetch("ghrepo", fetch_ghrepo("rust-lang/rust")).await.1),
        ("ip", try_fetch("ip", fetch_ip("8.8.8.8")).await.1),
        ("zen", try_fetch("zen", fetch_zen()).await.1),
        ("summarize", try_fetch("summarize", fetch_summarize("https://example.com")).await.1),
        ("bmi", fetch_bmi("175 70")),
    ];
    for (name, content) in templates {
        let path = out_dir.join(format!("{}.md", name));
        let _ = std::fs::write(&path, &content);
        println!("wrote {} (template)", name);
    }

    println!("=== DONE — check {}/ ===", out_dir.display());
    Ok(())
}
