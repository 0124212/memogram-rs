use anyhow::Result;
use base64::Engine;
use chrono::Local;
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
    Ph(String),
    Forecast(String),
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
    Lobsters,
    Guardian(String),
    Inbox,
    Undo,
    Pin,
    Note(String),
    Yt(String),
    Ghrepo(String),
    Book(String),
    Stocksave(String),
    Weather7(String),
    Img(String),
    Meditation(String),
    Affirmation(String),
    Reflection(String),
    Zen,
    Journal(String),
    Goal(String),
    Deadline(String),
    Astro(String),
    Review(String),
    Priority(String),
    Idea(String),
    Braindump(String),
    Summarize(String),
    Qr(String),
    Save(String),
    Dogs,
    Cats,
    Useless,
    Number(String),
    Activities,
    Bmi(String),
    Energy(String),
    Exercise(String),
    Water(String),
    Read(String),
    Pubmed(String),
    Drug(String),
    Ip(String),
    Protein(String),
    Chuck,
    Mood(String),
    Gratitude(String),
    Habit(String),
    Stress(String),
    Npm(String),
    Pypi(String),
    Crates(String),
    Stackoverflow(String),
    Mdn(String),
    Docker(String),
    Rfc(String),
    Man(String),
    Airquality(String),
    Sunrise(String),
    Insult(String),
    Etymology(String),
    Synonym(String),
    Philosophy,
    Finance(String),
    Compound(String),
    Trial(String),
    Food(String),
    Sunset(String),
    Itunes(String),
    Deezer(String),
    Mbrainz(String),
    Lyrics(String),
    Bpm(String),
    Trend,
    Promo(String),
    Setlist(String),
    Sample(String),
    Cover(String),
    Recap(String),
    Advice,
    Ticker(String),
    Dividend(String),
    Etf(String),
    Earnings(String),
    Trivia(String),
    Story(String),
    Hello(String),
    Wordoftheday,
    Kanye,
    Wouldyourather,
    Catfact,
    Affirmation2(String),
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
        teloxide::types::BotCommand { command: "lobsters".into(), description: "Lobsters tech stories".into() },
        teloxide::types::BotCommand { command: "guardian".into(), description: "Guardian <topic>".into() },
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
        teloxide::types::BotCommand { command: "book".into(), description: "book card".into() },
        teloxide::types::BotCommand { command: "book".into(), description: "book card".into() },
        teloxide::types::BotCommand { command: "todo".into(), description: "checklist".into() },
        teloxide::types::BotCommand { command: "list".into(), description: "bulleted list".into() },
        teloxide::types::BotCommand { command: "clip".into(), description: "save bookmark".into() },
        teloxide::types::BotCommand { command: "remind".into(), description: "remind <min> <msg>".into() },
        teloxide::types::BotCommand { command: "help".into(), description: "help".into() },
        teloxide::types::BotCommand { command: "pubmed".into(), description: "search PubMed papers".into() },
        teloxide::types::BotCommand { command: "drug".into(), description: "drug info".into() },
        teloxide::types::BotCommand { command: "genome".into(), description: "genome search".into() },
        teloxide::types::BotCommand { command: "protein".into(), description: "protein search".into() },
        teloxide::types::BotCommand { command: "joke".into(), description: "random joke".into() },
        teloxide::types::BotCommand { command: "mood".into(), description: "log mood".into() },
        teloxide::types::BotCommand { command: "gratitude".into(), description: "log gratitude".into() },
        teloxide::types::BotCommand { command: "habit".into(), description: "track habit".into() },
        teloxide::types::BotCommand { command: "npm".into(), description: "npm package info".into() },
        teloxide::types::BotCommand { command: "pypi".into(), description: "PyPI package info".into() },
        teloxide::types::BotCommand { command: "crates".into(), description: "crates.io info".into() },
        teloxide::types::BotCommand { command: "stackoverflow".into(), description: "Stack Overflow search".into() },
        teloxide::types::BotCommand { command: "mdn".into(), description: "MDN Web Docs".into() },
        teloxide::types::BotCommand { command: "docker".into(), description: "Docker Hub search".into() },
        teloxide::types::BotCommand { command: "rfc".into(), description: "IETF RFC lookup".into() },
        teloxide::types::BotCommand { command: "man".into(), description: "Unix man page".into() },
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
        teloxide::types::BotCommand { command: "setlist".into(), description: "generate setlist".into() },
        teloxide::types::BotCommand { command: "sample".into(), description: "find sample sources".into() },
        teloxide::types::BotCommand { command: "cover".into(), description: "find cover songs".into() },
        teloxide::types::BotCommand { command: "recap".into(), description: "weekly recap".into() },
        teloxide::types::BotCommand { command: "bored".into(), description: "bored? get an idea".into() },
        teloxide::types::BotCommand { command: "ticker".into(), description: "stock detail".into() },
        teloxide::types::BotCommand { command: "dividend".into(), description: "dividend info".into() },
        teloxide::types::BotCommand { command: "etf".into(), description: "ETF lookup".into() },
        teloxide::types::BotCommand { command: "earnings".into(), description: "earnings calendar".into() },
        teloxide::types::BotCommand { command: "trivia".into(), description: "random trivia question".into() },
        teloxide::types::BotCommand { command: "story".into(), description: "random short story".into() },
        teloxide::types::BotCommand { command: "fortune".into(), description: "fortune cookie".into() },
        teloxide::types::BotCommand { command: "wordoftheday".into(), description: "word of the day".into() },
        teloxide::types::BotCommand { command: "quote".into(), description: "random quote".into() },
        teloxide::types::BotCommand { command: "truth".into(), description: "truth or dare".into() },
        teloxide::types::BotCommand { command: "fact".into(), description: "random fun fact".into() },
        teloxide::types::BotCommand { command: "dadjoke".into(), description: "dad joke".into() },
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
        Command::Lobsters => { let txt = fetch_lobsters().await.unwrap_or_else(|e| format!("lobsters err: {e}")); create_as_bot(&bot, &msg, &app, "news", &txt, tid).await?; }
        Command::Guardian(q) => { let txt = fetch_guardian(&q).await.unwrap_or_else(|e| format!("guardian err: {e}")); create_as_bot(&bot, &msg, &app, "news", &txt, tid).await?; }
        Command::Arxiv(topic) => { let txt = fetch_arxiv(&topic).await.unwrap_or_else(|e| format!("arxiv err: {e}")); create_as_bot(&bot, &msg, &app, "news", &txt, tid).await?; }
        Command::Devto => { let txt = fetch_devto().await.unwrap_or_else(|e| format!("devto err: {e}")); create_as_bot(&bot, &msg, &app, "news", &txt, tid).await?; }
        Command::Stock(ticker) => { let txt = fetch_stock(&ticker).await.unwrap_or_else(|e| format!("stock err: {e}")); create_as_bot(&bot, &msg, &app, "money", &txt, tid).await?; }
        Command::Crypto(coin) => { let txt = fetch_crypto(&coin).await.unwrap_or_else(|e| format!("crypto err: {e}")); create_as_bot(&bot, &msg, &app, "money", &txt, tid).await?; }
        Command::Translate(args) => { let txt = fetch_translate(&args).await.unwrap_or_else(|e| format!("translate err: {e}")); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Ph(expr) => { let txt = fetch_ph(&expr); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
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
        Command::Note(content) => {
            let token = { app.store.read().await.get(&tid).cloned() };
            let Some(tok) = token else { bot.send_message(msg.chat.id, "run /start <token> first").await?; return Ok(()); };
            let txt = create_note(&app.memos_url, &tok, &content).await;
            bot.send_message(msg.chat.id, txt).parse_mode(ParseMode::MarkdownV2).await?;
        }
        Command::Yt(url) => { let txt = fetch_yt(&url).await.unwrap_or_else(|e| format!("yt err: {e}")); create_as_bot(&bot, &msg, &app, "planning", &txt, tid).await?; }
        Command::Ghrepo(repo) => { let txt = fetch_ghrepo(&repo).await.unwrap_or_else(|e| format!("ghrepo err: {e}")); create_as_bot(&bot, &msg, &app, "planning", &txt, tid).await?; }
        Command::Book(args) => { let txt = create_book(&args); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Stocksave(ticker) => { let txt = fetch_stocksave(&ticker).await.unwrap_or_else(|e| format!("stocksave err: {e}")); create_as_bot(&bot, &msg, &app, "planning", &txt, tid).await?; }
        Command::Weather7(city) => { let txt = fetch_weather7(&city).await.unwrap_or_else(|e| format!("weather7 err: {e}")); create_as_bot(&bot, &msg, &app, "planning", &txt, tid).await?; }
        Command::Img(url) => { let txt = fetch_img(&url).await.unwrap_or_else(|e| format!("img err: {e}")); create_as_bot(&bot, &msg, &app, "inbox", &txt, tid).await?; }
        Command::Pubmed(q) => { let txt = fetch_pubmed(&q).await.unwrap_or_else(|e| format!("pubmed err: {e}")); create_as_bot(&bot, &msg, &app, "bio", &txt, tid).await?; }
        Command::Drug(name) => { let txt = fetch_drug(&name).await.unwrap_or_else(|e| format!("drug err: {e}")); create_as_bot(&bot, &msg, &app, "bio", &txt, tid).await?; }
        Command::Ip(ip) => { let txt = fetch_ip(&ip).await.unwrap_or_else(|e| format!("ip err: {e}")); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
        Command::Protein(q) => { let txt = fetch_protein(&q).await.unwrap_or_else(|e| format!("protein err: {e}")); create_as_bot(&bot, &msg, &app, "bio", &txt, tid).await?; }
        Command::Chuck => { let txt = fetch_chuck().await.unwrap_or_else(|e| format!("chuck err: {e}")); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Mood(note) => { let txt = create_mood_entry(&note); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Gratitude(note) => { let txt = create_gratitude_entry(&note); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Habit(args) => { let txt = create_habit_entry(&args); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Stress(args) => { let txt = create_stress(&args); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Npm(pkg) => { let txt = fetch_npm(&pkg).await.unwrap_or_else(|e| format!("npm err: {e}")); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
        Command::Pypi(pkg) => { let txt = fetch_pypi(&pkg).await.unwrap_or_else(|e| format!("pypi err: {e}")); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
        Command::Crates(pkg) => { let txt = fetch_crates(&pkg).await.unwrap_or_else(|e| format!("crates err: {e}")); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
        Command::Stackoverflow(q) => { let txt = fetch_stackoverflow(&q).await.unwrap_or_else(|e| format!("stackoverflow err: {e}")); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
        Command::Mdn(q) => { let txt = fetch_mdn(&q).await.unwrap_or_else(|e| format!("mdn err: {e}")); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
        Command::Docker(q) => { let txt = fetch_docker(&q).await.unwrap_or_else(|e| format!("docker err: {e}")); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
        Command::Rfc(q) => { let txt = fetch_rfc(&q).await.unwrap_or_else(|e| format!("rfc err: {e}")); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
        Command::Man(q) => { let txt = fetch_man(&q).await.unwrap_or_else(|e| format!("man err: {e}")); create_as_bot(&bot, &msg, &app, "dev", &txt, tid).await?; }
        Command::Airquality(loc) => { let txt = fetch_airquality(&loc).await.unwrap_or_else(|e| format!("airquality err: {e}")); create_as_bot(&bot, &msg, &app, "weather", &txt, tid).await?; }
        Command::Sunrise(loc) => { let txt = fetch_sunrise(&loc).await.unwrap_or_else(|e| format!("sunrise err: {e}")); create_as_bot(&bot, &msg, &app, "weather", &txt, tid).await?; }
        Command::Sunset(loc) => { let txt = fetch_sunrise(&loc).await.unwrap_or_else(|e| format!("sunrise err: {e}")); create_as_bot(&bot, &msg, &app, "weather", &txt, tid).await?; }
        Command::Insult(topic) => { let txt = fetch_insult(&topic).await.unwrap_or_else(|e| format!("insult err: {e}")); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
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
        Command::Zen => { let txt = fetch_zen().await.unwrap_or_else(|e| format!("zen err: {e}")); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Journal(note) => { let txt = create_journal(&note); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Goal(args) => { let txt = create_goal(&args); create_as_bot(&bot, &msg, &app, "planning", &txt, tid).await?; }
        Command::Deadline(args) => { let txt = create_deadline(&args); create_as_bot(&bot, &msg, &app, "planning", &txt, tid).await?; }
        Command::Astro(sign) => { let txt = fetch_astro(&sign).await.unwrap_or_else(|e| format!("astro err: {e}")); create_as_bot(&bot, &msg, &app, "planning", &txt, tid).await?; }
        Command::Review(args) => { let txt = create_review(&args); create_as_bot(&bot, &msg, &app, "planning", &txt, tid).await?; }
        Command::Priority(args) => { let txt = create_priority(&args); create_as_bot(&bot, &msg, &app, "planning", &txt, tid).await?; }
        Command::Idea(args) => { let txt = create_idea(&args); create_as_bot(&bot, &msg, &app, "inbox", &txt, tid).await?; }
        Command::Braindump(args) => { let txt = create_braindump(&args); create_as_bot(&bot, &msg, &app, "inbox", &txt, tid).await?; }
        Command::Summarize(url) => { let txt = fetch_summarize(&url).await.unwrap_or_else(|e| format!("summarize err: {e}")); create_as_bot(&bot, &msg, &app, "inbox", &txt, tid).await?; }
        Command::Qr(text) => { let txt = fetch_qr(&text); create_as_bot(&bot, &msg, &app, "inbox", &txt, tid).await?; }
        Command::Save(args) => { let txt = create_save(&args); create_as_bot(&bot, &msg, &app, "inbox", &txt, tid).await?; }
        Command::Dogs => { let txt = fetch_dogs().await.unwrap_or_else(|e| format!("dogs err: {e}")); create_as_bot(&bot, &msg, &app, "daily", &txt, tid).await?; }
        Command::Cats => { let txt = fetch_cats().await.unwrap_or_else(|e| format!("cats err: {e}")); create_as_bot(&bot, &msg, &app, "daily", &txt, tid).await?; }
        Command::Useless => { let txt = fetch_useless().await.unwrap_or_else(|e| format!("useless err: {e}")); create_as_bot(&bot, &msg, &app, "daily", &txt, tid).await?; }
        Command::Number(num) => { let txt = fetch_number(&num).await.unwrap_or_else(|e| format!("number err: {e}")); create_as_bot(&bot, &msg, &app, "daily", &txt, tid).await?; }
        Command::Activities => { let txt = fetch_activities().await.unwrap_or_else(|e| format!("activities err: {e}")); create_as_bot(&bot, &msg, &app, "daily", &txt, tid).await?; }
        Command::Bmi(args) => { let txt = fetch_bmi(&args); create_as_bot(&bot, &msg, &app, "bio", &txt, tid).await?; }
        Command::Energy(args) => { let txt = create_energy(&args); create_as_bot(&bot, &msg, &app, "bio", &txt, tid).await?; }
        Command::Exercise(args) => { let txt = create_exercise(&args); create_as_bot(&bot, &msg, &app, "bio", &txt, tid).await?; }
        Command::Water(args) => { let txt = create_water(&args); create_as_bot(&bot, &msg, &app, "bio", &txt, tid).await?; }
        Command::Read(args) => { let txt = create_read(&args); create_as_bot(&bot, &msg, &app, "inbox", &txt, tid).await?; }
        Command::Itunes(q) => { let txt = fetch_itunes(&q).await.unwrap_or_else(|e| format!("itunes err: {e}")); create_as_bot(&bot, &msg, &app, "music", &txt, tid).await?; }
        Command::Deezer(q) => { let txt = fetch_deezer(&q).await.unwrap_or_else(|e| format!("deezer err: {e}")); create_as_bot(&bot, &msg, &app, "music", &txt, tid).await?; }
        Command::Mbrainz(q) => { let txt = fetch_mbrainz(&q).await.unwrap_or_else(|e| format!("mbrainz err: {e}")); create_as_bot(&bot, &msg, &app, "music", &txt, tid).await?; }
        Command::Lyrics(q) => { let txt = fetch_lyrics(&q).await.unwrap_or_else(|e| format!("lyrics err: {e}")); create_as_bot(&bot, &msg, &app, "music", &txt, tid).await?; }
        Command::Bpm(q) => { let txt = fetch_bpm(&q).await.unwrap_or_else(|e| format!("bpm err: {e}")); create_as_bot(&bot, &msg, &app, "music", &txt, tid).await?; }
        Command::Trend => { let txt = fetch_trend().await.unwrap_or_else(|e| format!("trend err: {e}")); create_as_bot(&bot, &msg, &app, "music", &txt, tid).await?; }
        Command::Promo(q) => { let txt = create_promo(&q); create_as_bot(&bot, &msg, &app, "music", &txt, tid).await?; }
        Command::Setlist(q) => { let txt = create_setlist(&q); create_as_bot(&bot, &msg, &app, "music", &txt, tid).await?; }
        Command::Sample(q) => { let txt = create_sample(&q); create_as_bot(&bot, &msg, &app, "music", &txt, tid).await?; }
        Command::Cover(q) => { let txt = create_cover(&q); create_as_bot(&bot, &msg, &app, "music", &txt, tid).await?; }
        Command::Recap(q) => { let txt = create_recap(&q).await; create_as_bot(&bot, &msg, &app, "daily", &txt, tid).await?; }
        Command::Trivia(args) => { let txt = fetch_trivia(&args).await.unwrap_or_else(|e| format!("trivia err: {e}")); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Story(args) => { let txt = fetch_story(&args).await.unwrap_or_else(|e| format!("story err: {e}")); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Hello(lang) => { let txt = fetch_hello(&lang).await.unwrap_or_else(|e| format!("hello err: {e}")); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Wordoftheday => { let txt = fetch_wordoftheday().await.unwrap_or_else(|e| format!("word err: {e}")); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Kanye => { let txt = fetch_kanye().await.unwrap_or_else(|e| format!("kanye err: {e}")); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Wouldyourather => { let txt = fetch_wouldyourather().await.unwrap_or_else(|e| format!("wyr err: {e}")); create_as_bot(&bot, &msg, &app, "planning", &txt, tid).await?; }
        Command::Catfact => { let txt = fetch_catfact().await.unwrap_or_else(|e| format!("catfact err: {e}")); create_as_bot(&bot, &msg, &app, "learn", &txt, tid).await?; }
        Command::Affirmation2(mood) => { let txt = fetch_affirmation2(&mood).await.unwrap_or_else(|e| format!("affirmation2 err: {e}")); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Advice => { let txt = fetch_advice().await.unwrap_or_else(|e| format!("advice err: {e}")); create_as_bot(&bot, &msg, &app, "wellness", &txt, tid).await?; }
        Command::Ticker(q) => { let txt = fetch_ticker(&q).await.unwrap_or_else(|e| format!("ticker err: {e}")); create_as_bot(&bot, &msg, &app, "money", &txt, tid).await?; }
        Command::Dividend(q) => { let txt = fetch_dividend(&q).await.unwrap_or_else(|e| format!("dividend err: {e}")); create_as_bot(&bot, &msg, &app, "money", &txt, tid).await?; }
        Command::Etf(q) => { let txt = fetch_etf(&q).await.unwrap_or_else(|e| format!("etf err: {e}")); create_as_bot(&bot, &msg, &app, "money", &txt, tid).await?; }
        Command::Earnings(q) => { let txt = fetch_earnings(&q).await.unwrap_or_else(|e| format!("earnings err: {e}")); create_as_bot(&bot, &msg, &app, "money", &txt, tid).await?; }
        
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
    // Add sunrise/sunset if available
    if let Some(astronomy) = v["weather"].as_array().and_then(|a| a.first()).and_then(|d| d["astronomy"].as_array()).and_then(|a| a.first()) {
        let sunrise = astronomy["sunrise"].as_str().unwrap_or("");
        let sunset = astronomy["sunset"].as_str().unwrap_or("");
        if !sunrise.is_empty() || !sunset.is_empty() {
            out.push_str(&format!("🌅 Sunrise: `{}` · 🌇 Sunset: `{}`\n\n", sunrise, sunset));
        }
    }
    if let Some(arr) = v["weather"].as_array() {
        out.push_str("## 📅 3-Day Forecast\n\n");
        out.push_str("| Date | High | Low | Condition | Rain |\n|---|---|---|---|---|\n");
        for day in arr.iter().take(3) {
            let date = day["date"].as_str().unwrap_or("");
            let maxt = day["maxtempC"].as_str().unwrap_or("?");
            let mint = day["mintempC"].as_str().unwrap_or("?");
            let hourly = day["hourly"].as_array();
            let noon = hourly.and_then(|h| h.get(4)).and_then(|h| h["weatherDesc"][0]["value"].as_str()).unwrap_or("");
            let rain = hourly.and_then(|h| h.get(4)).and_then(|h| h["chanceofrain"].as_str()).unwrap_or("?");
            out.push_str(&format!("| {} | ↑{}°C | ↓{}°C | {} | {}% |\n", date, maxt, mint, noon, rain));
        }
    }
    out.push_str(&format!("\n{}", tg_footer("wttr.in", "forecast")));
    Ok(out)
}

// --- number trivia ---

// --- utility functions ---

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
    format!("{}\n\n**Source:** `reddit.com` · **Subreddit:** `r/{}`\n\n⚠️ _{}_\n\n## 🔗 Quick Links\n\n> [r/{0}](https://reddit.com/r/{0}) — Browse directly\n> [reddit.com/r/{0}/top](https://reddit.com/r/{0}/top?t=day) — Top today\n> [reddit.com/r/{0}/hot](https://reddit.com/r/{0}/hot) — Hot posts\n\n{}\n\n`{}` · #reddit #{}",
        tg_header("👽", "Reddit", &format!("r/{}", sub)), sub, err, tg_footer("reddit.com", "reddit"), now, sub)
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

// --- news: lobsters ---

async fn fetch_lobsters() -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let v: serde_json::Value = HTTP.get("https://lobste.rs/hottest.json")
        .header("User-Agent", "memogram-rs")
        .timeout(std::time::Duration::from_secs(8))
        .send().await?.json().await?;
    let items = v.as_array().ok_or_else(|| anyhow::anyhow!("not array"))?;
    if items.is_empty() { return Ok(format!("{}\n\n⚠️ _No stories found_\n\n{}\n\n`{}` · #lobsters", tg_header("🔥", "Lobsters", "Tech"), tg_footer("lobste.rs", "lobsters"), now)); }
    let total_score: i64 = items.iter().map(|i| i["score"].as_i64().unwrap_or(0)).sum();
    let total_comments: i64 = items.iter().map(|i| i["comment_count"].as_i64().unwrap_or(0)).sum();
    let mut out = format!("{}\n\n", tg_header("🔥", "Lobsters", "Tech"));
    out.push_str("**Source:** `lobste.rs` · **Category:** `Tech` · **Bias:** `Community`\n\n");
    out.push_str("## 📊 Coverage\n\n");
    out.push_str("| Stat | Value |\n|---|---|\n");
    out.push_str(&format!("| Stories | {} |\n", items.len()));
    out.push_str(&format!("| Total Score | `{}` |\n", total_score));
    out.push_str(&format!("| Total Comments | `{}` |\n", total_comments));
    out.push_str(&format!("| Updated | `{}` |\n\n", now));
    out.push_str("## 📰 Top Stories\n\n");
    for (i, item) in items.iter().take(5).enumerate() {
        let title = item["title"].as_str().unwrap_or("?");
        let url = item["url"].as_str().unwrap_or("");
        let score = item["score"].as_i64().unwrap_or(0);
        let comments = item["comment_count"].as_i64().unwrap_or(0);
        let tags = item["tags"].as_array().map(|a| a.iter().filter_map(|t| t.as_str()).collect::<Vec<_>>()).unwrap_or_default();
        let tag_str = if tags.is_empty() { String::new() } else { format!(" · `{}`", tags.join("`, `")) };
        out.push_str(&format!("**{}.** [{}]({})\n   ⬆️ `{}` · 💬 `{}`{}\n\n", i+1, title, url, score, comments, tag_str));
    }
    out.push_str(&format!("{}\n\n`{}` · #lobsters #tech", tg_footer("lobste.rs", "lobsters"), now));
    Ok(out)
}

// --- news: guardian ---

async fn fetch_guardian(query: &str) -> Result<String> {
    let q = if query.trim().is_empty() { "technology".to_string() } else { query.trim().to_string() };
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    // Guardian RSS feed (no API key needed)
    let rss_url = format!("https://www.theguardian.com/{}/rss", q.replace(' ', "-"));
    let txt = match tokio::time::timeout(std::time::Duration::from_secs(8), HTTP.get(&rss_url).header("User-Agent", "memogram-rs").send()).await {
        Ok(Ok(r)) => match r.text().await { Ok(t) => t, Err(e) => return Ok(guardian_fallback(&q, &format!("Parse error: {e}"))) },
        Ok(Err(e)) => return Ok(guardian_fallback(&q, &format!("Network error: {e}"))),
        Err(_) => return Ok(guardian_fallback(&q, "Timeout")),
    };
    let items = parse_rss_items(&txt, "item");
    if items.is_empty() {
        // Try section feed as fallback
        let fallback_url = format!("https://www.theguardian.com/world/rss");
        let txt2 = match tokio::time::timeout(std::time::Duration::from_secs(8), HTTP.get(&fallback_url).header("User-Agent", "memogram-rs").send()).await {
            Ok(Ok(r)) => match r.text().await { Ok(t) => t, Err(_) => return Ok(guardian_fallback(&q, "No stories")) },
            _ => return Ok(guardian_fallback(&q, "No stories")),
        };
        let items2 = parse_rss_items(&txt2, "item");
        if items2.is_empty() { return Ok(guardian_fallback(&q, "No stories")); }
        let mut out = format!("{}\n\n", tg_header("📰", "Guardian", &q));
        out.push_str(&format!("**Source:** `theguardian.com` · **Query:** `{}` · **Bias:** `Very Low`\n\n", q));
        out.push_str("## 📊 Coverage\n\n");
        out.push_str("| Stat | Value |\n|---|---|\n");
        out.push_str(&format!("| Stories | {} |\n", items2.len()));
        out.push_str(&format!("| Section | `world` (fallback) |\n"));
        out.push_str(&format!("| Updated | `{}` |\n\n", now));
        out.push_str("## 📰 Top Stories\n\n");
        for (i, (title, link, desc, pub_date)) in items2.iter().take(5).enumerate() {
            let desc_short = if desc.len() > 100 { format!("{}...", &desc[..100]) } else { desc.clone() };
            out.push_str(&format!("**{}.** [{}]({})\n   📅 `{}` · 📝 {}\n\n", i+1, title, link, pub_date, desc_short));
        }
        out.push_str(&format!("{}\n\n`{}` · #guardian #news", tg_footer("theguardian.com", "guardian"), now));
        return Ok(out);
    }
    let mut out = format!("{}\n\n", tg_header("📰", "Guardian", &q));
    out.push_str(&format!("**Source:** `theguardian.com` · **Query:** `{}` · **Bias:** `Very Low`\n\n", q));
    out.push_str("## 📊 Coverage\n\n");
    out.push_str("| Stat | Value |\n|---|---|\n");
    out.push_str(&format!("| Stories | {} |\n", items.len()));
    out.push_str(&format!("| Updated | `{}` |\n\n", now));
    out.push_str("## 📰 Top Stories\n\n");
    for (i, (title, link, desc, pub_date)) in items.iter().take(5).enumerate() {
        let desc_short = if desc.len() > 100 { format!("{}...", &desc[..100]) } else { desc.clone() };
        out.push_str(&format!("**{}.** [{}]({})\n   📅 `{}` · 📝 {}\n\n", i+1, title, link, pub_date, desc_short));
    }
    out.push_str(&format!("{}\n\n`{}` · #guardian #news", tg_footer("theguardian.com", "guardian"), now));
    Ok(out)
}

fn guardian_fallback(query: &str, err: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    format!("{}\n\n**Source:** `theguardian.com` · **Query:** `{}`\n\n⚠️ _{}_\n\n> Try: `theguardian.com/{}`
\n\n{}\n\n`{}` · #guardian #news",
        tg_header("📰", "Guardian", query), query, err, query.replace(' ', "-"), tg_footer("theguardian.com", "guardian"), now)
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

async fn fetch_mdn(query: &str) -> Result<String> {
    let q = query.trim();
    if q.is_empty() { return Ok(format!("{}\n\n_Usage:_ `/mdn <query>` — e.g. `fetch`, `Promise`, `CSS Grid`\n\n{}", tg_header("📚", "MDN", "Web Docs"), tg_footer("developer.mozilla.org", "mdn"))); }
    let url = format!("https://developer.mozilla.org/api/v1/search?q={}&limit=5", urlencoding::encode(q));
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    let docs = v["documents"].as_array().ok_or_else(|| anyhow::anyhow!("no documents"))?;
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    if docs.is_empty() { return Ok(format!("{}\n\n⚠️ _No results for `{}`_\n\n> Try: [developer.mozilla.org](https://developer.mozilla.org/search?q={})\n\n{}\n\n`{}` · #mdn", tg_header("📚", "MDN", q), q, urlencoding::encode(q), tg_footer("developer.mozilla.org", "mdn"), now)); }
    let mut out = format!("{}\n\n", tg_header("📚", "MDN", q));
    out.push_str(&format!("**Query:** `{}` · **Results:** {}\n\n", q, docs.len()));
    out.push_str("## 📖 Top Results\n\n");
    for (i, doc) in docs.iter().take(5).enumerate() {
        let title = doc["title"].as_str().unwrap_or("?");
        let slug = doc["slug"].as_str().unwrap_or("");
        let summary = doc["summary"].as_str().unwrap_or("").chars().take(120).collect::<String>();
        let doc_url = format!("https://developer.mozilla.org/en-US/docs/{}", slug);
        let locale = doc["locale"].as_str().unwrap_or("en-US");
        let tags = doc["tags"].as_array().map(|a| a.iter().filter_map(|t| t.as_str()).take(3).collect::<Vec<_>>()).unwrap_or_default();
        let tag_str = if tags.is_empty() { String::new() } else { format!(" · `{}`", tags.join("`, `")) };
        out.push_str(&format!("**{}.** [{}]({})\n   `{}`{}\n   📝 {}\n\n", i+1, title, doc_url, locale, tag_str, summary));
    }
    out.push_str(&format!("{}\n\n`{}` · #mdn #dev", tg_footer("developer.mozilla.org", "mdn"), now));
    Ok(out)
}

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

async fn fetch_rfc(query: &str) -> Result<String> {
    let q = query.trim();
    if q.is_empty() { return Ok(format!("{}\n\n_Usage:_ `/rfc <number|topic>` — e.g. `7231`, `HTTP`, `WebSocket`\n\n{}", tg_header("📄", "IETF RFC", "Standards"), tg_footer("datatracker.ietf.org", "rfc"))); }
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    // Try direct RFC number first
    let is_number = q.chars().all(|c| c.is_ascii_digit());
    if is_number {
        let url = format!("https://datatracker.ietf.org/api/v1/doc/document/rfc{}/", q);
        if let Ok(v) = HTTP.get(&url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await {
            if let Ok(j) = v.json::<serde_json::Value>().await {
                if let Some(title) = j["title"].as_str() {
                    let rfc_num = j["rfc_number"].as_i64().unwrap_or(0);
                    let doc_type = j["doc_type"].as_str().unwrap_or("RFC");
                    let pub_date = j["published"].as_str().unwrap_or("").chars().take(10).collect::<String>();
                    let abstract_text = j["abstract"].as_str().unwrap_or("").chars().take(200).collect::<String>();
                    let stream = j["stream"].as_str().unwrap_or("?");
                    let pages = j["pages"].as_i64().unwrap_or(0);
                    let rfc_url = format!("https://www.rfc-editor.org/rfc/rfc{}", rfc_num);
                    let mut out = format!("{}\n\n", tg_header("📄", &format!("RFC {}", rfc_num), title));
                    out.push_str(&format!("| Stat | Value |\n|---|---|\n| Number | `RFC {}` |\n| Title | {} |\n| Type | `{}` |\n| Stream | `{}` |\n| Pages | `{}` |\n| Published | `{}` |\n\n", rfc_num, title, doc_type, stream, pages, pub_date));
                    if !abstract_text.is_empty() { out.push_str(&format!("## 📝 Abstract\n\n> {}\n\n", abstract_text)); }
                    out.push_str(&format!("🔗 [Full Text]({})\n\n", rfc_url));
                    out.push_str(&format!("{}\n\n`{}` · #rfc #dev", tg_footer("rfc-editor.org", "rfc"), now));
                    return Ok(out);
                }
            }
        }
    }
    // Fallback: search RFCs
    let url = format!("https://datatracker.ietf.org/api/v1/doc/document/?format=json&title={}&rows=5", urlencoding::encode(q));
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    let objects = v["objects"].as_array().ok_or_else(|| anyhow::anyhow!("no results"))?;
    if objects.is_empty() { return Ok(format!("{}\n\n⚠️ _No RFCs for `{}`_\n\n> Try: [datatracker.ietf.org](https://datatracker.ietf.org/doc/search/?name={})\n\n{}\n\n`{}` · #rfc", tg_header("📄", "IETF RFC", q), q, urlencoding::encode(q), tg_footer("datatracker.ietf.org", "rfc"), now)); }
    let mut out = format!("{}\n\n", tg_header("📄", "IETF RFC", q));
    out.push_str(&format!("**Query:** `{}` · **Results:** {}\n\n", q, objects.len()));
    out.push_str("## 📄 Matching RFCs\n\n");
    for (i, item) in objects.iter().take(5).enumerate() {
        let title = item["title"].as_str().unwrap_or("?");
        let rfc_num = item["rfc_number"].as_i64().unwrap_or(0);
        let pub_date = item["published"].as_str().unwrap_or("").chars().take(10).collect::<String>();
        let doc_type = item["doc_type"].as_str().unwrap_or("RFC");
        let rfc_url = format!("https://www.rfc-editor.org/rfc/rfc{}", rfc_num);
        out.push_str(&format!("**{}.** [RFC {} — {}]({})\n   📅 `{}` · `{}`\n\n", i+1, rfc_num, title, rfc_url, pub_date, doc_type));
    }
    out.push_str(&format!("{}\n\n`{}` · #rfc #dev", tg_footer("datatracker.ietf.org", "rfc"), now));
    Ok(out)
}

async fn fetch_man(query: &str) -> Result<String> {
    let q = query.trim();
    if q.is_empty() { return Ok(format!("{}\n\n_Usage:_ `/man <command>` — e.g. `git`, `curl`, `chmod`\n\n{}", tg_header("📖", "Man Pages", "Commands"), tg_footer("manpages.debian.net", "man"))); }
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    // Try Debian manpages API
    let url = format!("https://manpages.debian.net/cgi-bin/man.cgi?manpage={}&format=json", urlencoding::encode(q));
    match HTTP.get(&url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await {
        Ok(r) => {
            let txt = r.text().await.unwrap_or_default();
            // Try to parse as JSON, fallback to HTML
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
                let name = v["name"].as_str().unwrap_or(q);
                let section = v["section"].as_str().unwrap_or("?");
                let description = v["description"].as_str().unwrap_or("").chars().take(200).collect::<String>();
                let synopsis = v["synopsis"].as_str().unwrap_or("").chars().take(150).collect::<String>();
                let see_also = v["see_also"].as_str().unwrap_or("");
                let man_url = format!("https://manpages.debian.net/cgi-bin/man.cgi?manpage={}", urlencoding::encode(q));
                let mut out = format!("{}\n\n", tg_header("📖", "Man Page", &format!("{}({})", name, section)));
                out.push_str(&format!("| Stat | Value |\n|---|---|\n| Command | `{}` |\n| Section | `{}` |\n\n", name, section));
                if !synopsis.is_empty() { out.push_str(&format!("## 🔧 Synopsis\n\n```\n{}\n```\n\n", synopsis)); }
                if !description.is_empty() { out.push_str(&format!("## 📝 Description\n\n{}\n\n", description)); }
                if !see_also.is_empty() { out.push_str(&format!("## 🔗 See Also\n\n{}\n\n", see_also)); }
                out.push_str(&format!("🔗 [Full Man Page]({})\n\n", man_url));
                out.push_str(&format!("{}\n\n`{}` · #man #dev", tg_footer("manpages.debian.net", "man"), now));
                return Ok(out);
            }
            // Fallback to simple page
            let man_url = format!("https://manpages.debian.net/cgi-bin/man.cgi?manpage={}", urlencoding::encode(q));
            Ok(format!("{}\n\n📖 **Man page for `{}`**\n\n🔗 [View Man Page]({})\n\n{}\n\n`{}` · #man #dev",
                tg_header("📖", "Man Page", q), q, man_url, tg_footer("manpages.debian.net", "man"), now))
        }
        Err(_e) => {
            let man_url = format!("https://manpages.debian.net/cgi-bin/man.cgi?manpage={}", urlencoding::encode(q));
            // Provide common examples for popular commands
            let examples = match q {
                "git" => Some("```\ngit add . && git commit -m \"msg\" && git push\ngit log --oneline -10\ngit diff HEAD~1\n```"),
                "curl" => Some("```\ncurl -s https://api.example.com\ncurl -X POST -d '{\"key\":\"val\"}' -H 'Content-Type: application/json' url\ncurl -o file.txt https://example.com/file\n```"),
                "chmod" => Some("```\nchmod 755 script.sh    # rwxr-xr-x\nchmod +x script.sh     # make executable\nchmod 644 file.txt     # rw-r--r--\n```"),
                "ssh" => Some("```\nssh user@host\nssh -i key.pem user@host\nssh -L 8080:localhost:80 user@host  # port forward\n```"),
                "docker" => Some("```\ndocker ps -a\ndocker logs container_name\ndocker exec -it container bash\ndocker system prune -af\n```"),
                "tar" => Some("```\ntar -xzf archive.tar.gz    # extract\ntar -czf archive.tar.gz dir/  # create\ntar -tf archive.tar.gz     # list contents\n```"),
                "sed" => Some("```\nsed 's/old/new/g' file.txt           # replace all\nsed -i 's/old/new/g' file.txt       # in-place\nsed -n '10,20p' file.txt             # print lines 10-20\n```"),
                "awk" => Some("```\nawk '{print $1}' file.txt            # print first column\nawk -F: '{print $1}' /etc/passwd     # custom delimiter\nawk '{sum+=$1} END {print sum}'      # sum column\n```"),
                _ => None,
            };
            let mut out = format!("{}\n\n📖 **Man page for `{}`**\n\n", tg_header("📖", "Man Page", q), q);
            if let Some(ex) = examples {
                out.push_str("## 🔧 Common Examples\n\n");
                out.push_str(ex);
                out.push_str("\n\n");
            }
            out.push_str(&format!("🔗 [View Full Man Page]({})\n", man_url));
            out.push_str(&format!("🔗 [tldr.sh](https://tldr.sh/{})\n", urlencoding::encode(q)));
            out.push_str(&format!("🔗 [devhints.io](https://devhints.io/{})\n\n", urlencoding::encode(q)));
            out.push_str(&format!("{}\n\n`{}` · #man #dev", tg_footer("manpages.debian.net", "man"), now));
            Ok(out)
        }
    }
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

async fn fetch_etymology(word: &str) -> Result<String> {
    let url = format!("https://en.wiktionary.org/w/api.php?action=parse&page={}&prop=wikitext&format=json", urlencoding::encode(word));
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "memogram-rs").send().await?.json().await?;
    let wikitext = v["parse"]["wikitext"]["wikitext"].as_str().unwrap_or("");
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let mut out = format!("{}\n\n", tg_header("📖", "Etymology", word));
    let lower = wikitext.to_lowercase();
    // Try multiple etymology section patterns
    let etym = if let Some(start) = lower.find("==etymology==") {
        let rest = &wikitext[start + 13..];
        if let Some(end) = rest.find("\n==") { Some(rest[..end].trim()) } else { Some(rest.trim()) }
    } else if let Some(start) = lower.find("==etymology 1==") {
        let rest = &wikitext[start + 15..];
        if let Some(end) = rest.find("\n==") { Some(rest[..end].trim()) } else { Some(rest.trim()) }
    } else if let Some(start) = lower.find("===etymology===") {
        let rest = &wikitext[start + 15..];
        if let Some(end) = rest.find("\n==") { Some(rest[..end].trim()) } else { Some(rest.trim()) }
    } else {
        None
    };
    if let Some(raw) = etym {
        if !raw.is_empty() {
            let clean = raw.replace("{{inh|en|", "").replace("{{der|en|", "").replace("{{bor|en|", "").replace("{{m|en|", "").replace("{{l|en|", "").replace("}}", "").replace("{{XLIT|en|", "").replace('\n', " ");
            let short = clean.chars().take(600).collect::<String>();
            out.push_str(&format!("**Word:** `{}`\n\n", word));
            out.push_str(&format!("## 📚 Etymology\n\n{}\n\n", short));
            // Also try to extract pronunciation
            if let Some(start) = lower.find("==pronunciation==") {
                let rest = &wikitext[start + 17..];
                if let Some(end) = rest.find("\n==") {
                    let pron = rest[..end].trim().replace("{{IPA|en|", "/").replace("}}", "/").replace("{{enPR|", "").replace("}}", "");
                    if !pron.is_empty() {
                        let short_pron = pron.chars().take(150).collect::<String>();
                        out.push_str(&format!("## 🔊 Pronunciation\n\n{}\n\n", short_pron));
                    }
                }
            }
            out.push_str(&format!("🔗 [Full entry](https://en.wiktionary.org/wiki/{})\n\n", urlencoding::encode(word)));
            out.push_str(&format!("{}\n\n`{}` · #etymology", tg_footer("wiktionary.org", "etymology"), now));
            return Ok(out);
        }
    }
    // Fallback: try Wikipedia for word history
    let wiki_url = format!("https://en.wikipedia.org/api/rest_v1/page/summary/{}", urlencoding::encode(word));
    if let Ok(wv) = HTTP.get(&wiki_url).header("User-Agent", "memogram-rs").send().await {
        if let Ok(wj) = wv.json::<serde_json::Value>().await {
            if let Some(extract) = wj["extract"].as_str() {
                if extract.len() > 50 {
                    out.push_str(&format!("**Word:** `{}`\n\n", word));
                    out.push_str(&format!("## 📚 Background\n\n{}\n\n", extract.chars().take(400).collect::<String>()));
                    out.push_str(&format!("🔗 [Wikipedia]({})\n\n", wj["content_urls"]["desktop"]["page"].as_str().unwrap_or("")));
                    out.push_str(&format!("🔗 [Wiktionary](https://en.wiktionary.org/wiki/{})\n\n", urlencoding::encode(word)));
                    out.push_str(&format!("{}\n\n`{}` · #etymology", tg_footer("wiktionary.org", "etymology"), now));
                    return Ok(out);
                }
            }
        }
    }
    out.push_str(&format!("**Word:** `{}`\n\n", word));
    out.push_str(&format!("_No etymology section found._\n\n"));
    out.push_str(&format!("🔗 [Try Wiktionary](https://en.wiktionary.org/wiki/{})\n", urlencoding::encode(word)));
    out.push_str(&format!("🔗 [Try Etymonline](https://www.etymonline.com/word/{})\n\n", urlencoding::encode(word)));
    out.push_str(&format!("{}\n\n`{}` · #etymology", tg_footer("wiktionary.org", "etymology"), now));
    Ok(out)
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
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let v: serde_json::Value = match tokio::time::timeout(std::time::Duration::from_secs(8), HTTP.get(&url).header("User-Agent", "memogram-rs/0.1 ( junilab.xyz )").send()).await {
        Ok(Ok(r)) => match r.json::<serde_json::Value>().await { Ok(j) => j, Err(_) => serde_json::Value::Null },
        _ => serde_json::Value::Null,
    };
    let artists = v["artists"].as_array();
    if artists.is_none() || artists.unwrap().is_empty() {
        // Fallback: search via Wikipedia
        let wiki_url = format!("https://en.wikipedia.org/api/rest_v1/page/summary/{}", urlencoding::encode(q));
        if let Ok(wv) = HTTP.get(&wiki_url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(5)).send().await {
            if let Ok(wj) = wv.json::<serde_json::Value>().await {
                if let Some(extract) = wj["extract"].as_str() {
                    let mut out = format!("{}\n\n", tg_header("🎙️", "MusicBrainz", q));
                    out.push_str(&format!("**Query:** `{}`\n\n", q));
                    out.push_str(&format!("## 🎵 Artist Info\n\n{}\n\n", extract.chars().take(300).collect::<String>()));
                    if let Some(url) = wj["content_urls"]["desktop"]["page"].as_str() {
                        out.push_str(&format!("🔗 [Wikipedia]({})\n", url));
                    }
                    out.push_str(&format!("🔗 [MusicBrainz](https://musicbrainz.org/search?query={})\n\n", urlencoding::encode(q)));
                    out.push_str(&format!("{}\n\n`{}` · #mbrainz", tg_footer("musicbrainz.org", "mbrainz"), now));
                    return Ok(out);
                }
            }
        }
        return Ok(format!("{}\n\n_No artists for `{}`._\n\n> Try different spelling or use full name.\n\n🔗 [Search MusicBrainz](https://musicbrainz.org/search?query={})\n\n{}\n\n`{}` · #mbrainz", tg_header("🎙️", "MusicBrainz", q), q, urlencoding::encode(q), tg_footer("musicbrainz.org", "mbrainz"), now));
    }
    let arr = artists.unwrap();
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let mut out = format!("{}\n\n", tg_header("🎙️", "MusicBrainz", q));
    out.push_str(&format!("**Query:** `{}` · **Results:** {}\n\n", q, arr.len()));
    for (i, a) in arr.iter().take(3).enumerate() {
        let name = a["name"].as_str().unwrap_or("?");
        let disamb = a["disambiguation"].as_str().unwrap_or("");
        let country = a["country"].as_str().unwrap_or("?");
        let typ = a["type"].as_str().unwrap_or("?");
        let id = a["id"].as_str().unwrap_or("");
        let begin = a["life-span"]["begin"].as_str().unwrap_or("");
        let end = a["life-span"]["ended"].as_bool().and_then(|ended| {
            if ended { a["life-span"]["end"].as_str().map(|e| format!(" — {}", e)) } else { Some(" — present".to_string()) }
        }).unwrap_or_default();
        let tags: Vec<String> = a["tags"].as_array().map(|t| t.iter().take(3).filter_map(|tag| tag["name"].as_str()).map(|s| format!("`{}`", s)).collect()).unwrap_or_default();
        out.push_str(&format!("**{}. {}**", i+1, name));
        if !disamb.is_empty() { out.push_str(&format!(" — _{}_", disamb)); }
        out.push_str(&format!("\n   {} · `{}` · {}{}\n", typ, country, begin, end));
        if !tags.is_empty() { out.push_str(&format!("   🏷️ {}\n", tags.join(" · "))); }
        out.push_str(&format!("   🔗 [MusicBrainz](https://musicbrainz.org/artist/{})\n\n", id));
    }
    out.push_str(&format!("{}\n\n`{}` · #mbrainz", tg_footer("musicbrainz.org", "mbrainz"), now));
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

fn create_setlist(args: &str) -> String {
    let tracks = if args.trim().is_empty() { vec!["Beat 1", "Beat 2", "Beat 3", "Beat 4", "Beat 5"] } else { args.split(',').map(|s| s.trim()).collect::<Vec<_>>() };
    let now = Local::now().format("%Y-%m-%d").to_string();
    let total = tracks.len();
    let mut out = format!("{}\n\n", tg_header("🎵", "Setlist", &format!("{} tracks", total)));
    out.push_str(&format!("**Date:** `{}` · **Tracks:** `{}`\n\n", now, total));
    out.push_str("## 🎧 Track Order\n\n");
    for (i, track) in tracks.iter().enumerate() {
        out.push_str(&format!("{}. **{}**\n   ⏱ ~3:00\n\n", i+1, track));
    }
    out.push_str("## 📋 Stage Notes\n\n");
    out.push_str("| # | Track | Energy | Transition |\n|---|---|---|---|\n");
    for (i, track) in tracks.iter().enumerate() {
        let energy = if i < 2 { "🟢 Low" } else if i < tracks.len() - 2 { "🟡 Mid" } else { "🔴 High" };
        out.push_str(&format!("| {} | {} | {} | → |\n", i+1, track, energy));
    }
    out.push_str(&format!("\n## 🎯 Flow\n\n```\n"));
    for (i, track) in tracks.iter().enumerate() {
        out.push_str(&format!("[{}]{}", track, if i < tracks.len()-1 { " → " } else { "" }));
    }
    out.push_str("\n```\n\n");
    out.push_str(&format!("{}\n\n`{}` · #setlist #music", tg_footer("junilab", "setlist"), now));
    out
}

fn create_sample(args: &str) -> String {
    let idea = if args.trim().is_empty() { "vinyl crackle + piano loop" } else { args.trim() };
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let mut out = format!("{}\n\n", tg_header("🔍", "Sample Pack", idea));
    out.push_str(&format!("**Idea:** `{}`\n\n", idea));
    out.push_str("## 🎧 Source Ideas\n\n");
    out.push_str("| Source | Where to Find | Style |\n|---|---|---|\n");
    out.push_str(&format!("| Vinyl | Discogs, thrift stores | Warm, analog |\n"));
    out.push_str(&format!("| Field Recording | Freesound.org | Ambient, texture |\n"));
    out.push_str(&format!("| YouTube | Archive.org, live performances | Rare, unique |\n"));
    out.push_str(&format!("| Sample Packs | Splice, Loopcloud | Clean, ready |\n"));
    out.push_str(&format!("| Old Records | Local shops, eBay | Vintage, soul |\n\n"));
    out.push_str("## 🔧 Processing Tips\n\n");
    out.push_str("- **Chop**: Slice into 1/4 or 1/8 notes\n");
    out.push_str("- **Pitch**: Shift ±2-3 semitones for vibe\n");
    out.push_str("- **Filter**: Low-pass 200-800Hz for warmth\n");
    out.push_str("- **Layer**: Stack with synth for depth\n");
    out.push_str("- **FX**: Reverb + vinyl crackle = instant texture\n\n");
    out.push_str("## 📚 Legal\n\n");
    out.push_str("- ✅ Freesource.org: CC0 / royalty-free\n");
    out.push_str("- ⚠️ YouTube: Check copyright, use for reference\n");
    out.push_str("- ⚠️ Vinyl: Interpolation > direct sampling\n\n");
    out.push_str(&format!("{}\n\n`{}` · #sample #music", tg_footer("junilab", "sample"), now));
    out
}

fn create_cover(args: &str) -> String {
    let song = if args.trim().is_empty() { "Bohemian Rhapsody" } else { args.trim() };
    let now = Local::now().format("%Y-%m-%d").to_string();
    let mut out = format!("{}\n\n", tg_header("🎤", "Cover Finder", song));
    out.push_str(&format!("**Song:** `{}`\n\n", song));
    out.push_str("## 🎤 Cover Versions\n\n");
    out.push_str("| Artist | Style | Platform |\n|---|---|---|\n");
    out.push_str(&format!("| Original | — | Spotify / YouTube |\n"));
    out.push_str(&format!("| Acoustic | Stripped | YouTube |\n"));
    out.push_str(&format!("| Live | Raw energy | YouTube / Concert |\n"));
    out.push_str(&format!("| Remix | Electronic | SoundCloud |\n"));
    out.push_str(&format!("| Jazz | Smooth reinterpretation | Spotify |\n\n"));
    out.push_str("## 🔍 Search Links\n\n");
    let encoded = urlencoding::encode(song);
    out.push_str(&format!("- [YouTube](https://www.youtube.com/results?search_query={}+cover)\n", encoded));
    out.push_str(&format!("- [Spotify](https://open.spotify.com/search/{}%20cover)\n", encoded));
    out.push_str(&format!("- [SoundCloud](https://soundcloud.com/search?q={}+cover)\n\n", encoded));
    out.push_str("## 🎯 Tips for Covering\n\n");
    out.push_str("- Change tempo or key for fresh feel\n");
    out.push_str("- Strip to acoustic + vocal for intimacy\n");
    out.push_str("- Add your genre twist (lo-fi, trap, jazz)\n");
    out.push_str("- Credit original artist in description\n\n");
    out.push_str(&format!("{}\n\n`{}` · #cover #music", tg_footer("junilab", "cover"), now));
    out
}

async fn create_recap(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let date = Local::now().format("%Y-%m-%d").to_string();
    let days = args.trim().parse::<usize>().unwrap_or(7);
    let mut out = format!("{}\n\n", tg_header("📰", "Weekly Recap", &format!("last {} days", days)));
    out.push_str(&format!("**Period:** `{}` · **Days:** `{}`\n\n", date, days));
    out.push_str("## 📊 Activity\n\n");
    out.push_str("| Metric | Value |\n|---|---|\n");
    out.push_str(&format!("| Memos Created | — |\n"));
    out.push_str(&format!("| Tags Used | — |\n"));
    out.push_str(&format!("| Top Tag | — |\n\n"));
    out.push_str("## 📌 Highlights\n\n");
    out.push_str("- \n\n");
    out.push_str("## 🎯 Goals Status\n\n");
    out.push_str("- [ ] Goal 1\n");
    out.push_str("- [ ] Goal 2\n\n");
    out.push_str("## 💡 Ideas Generated\n\n");
    out.push_str("- \n\n");
    out.push_str(&format!("{}\n\n`{}` · #recap #daily", tg_footer("memos", "recap"), now));
    out
}

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

async fn fetch_ticker(ticker: &str) -> Result<String> {
    let t = ticker.trim().to_uppercase();
    if t.is_empty() { return Ok(format!("{}\n\n_Usage:_ `/ticker AAPL`\n\n{}", tg_header("📈", "Ticker", "lookup"), tg_footer("finnhub.io", "ticker"))); }
    let url = format!("https://query1.finance.yahoo.com/v8/finance/chart/{}?interval=1d&range=1d", urlencoding::encode(&t));
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    match HTTP.get(&url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await {
        Ok(r) => {
            let v: serde_json::Value = r.json().await?;
            let result = &v["chart"]["result"][0];
            let meta = &result["meta"];
            let price = meta["regularMarketPrice"].as_f64().unwrap_or(0.0);
            let prev = meta["chartPreviousClose"].as_f64().unwrap_or(price);
            let change = price - prev;
            let pct = if prev > 0.0 { (change / prev) * 100.0 } else { 0.0 };
            let sign = if change >= 0.0 { "+" } else { "" };
            let name = meta["symbol"].as_str().unwrap_or(&t);
            let currency = meta["currency"].as_str().unwrap_or("USD");
            let exchange = meta["exchangeName"].as_str().unwrap_or("?");
            let mut out = format!("{}\n\n", tg_header("📈", name, "stock"));
            out.push_str(&format!("**Price:** `{}{:.2}` **{}` · **{}{:.2}%** ({:.2})\n\n", currency, price, sign, sign, pct, change));
            out.push_str(&format!("| Stat | Value |\n|---|---|\n| Symbol | `{}` |\n| Currency | `{}` |\n| Exchange | `{}` |\n| Previous Close | `{}{:.2}` |\n| Change | `{}{:.2}` |\n| Change % | `{}{:.2}%` |\n\n", name, currency, exchange, currency, prev, sign, change, sign, pct));
            out.push_str(&format!("{}\n\n`{}` · #ticker #money", tg_footer("finance.yahoo.com", "ticker"), now));
            Ok(out)
        }
        Err(e) => Ok(format!("{}\n\n⚠️ _Error fetching `{}`: {}_\n\n{}", tg_header("📈", "Ticker", &t), t, e, tg_footer("finance.yahoo.com", "ticker"))),
    }
}

async fn fetch_dividend(ticker: &str) -> Result<String> {
    let t = ticker.trim().to_uppercase();
    if t.is_empty() { return Ok(format!("{}\n\n_Usage:_ `/dividend AAPL`\n\n{}", tg_header("💰", "Dividend", "info"), tg_footer("dividend.com", "dividend"))); }
    let now = Local::now().format("%Y-%m-%d").to_string();
    let mut out = format!("{}\n\n", tg_header("💰", "Dividend", &t));
    out.push_str(&format!("**Ticker:** `{}` · **Date:** `{}`\n\n", t, now));
    out.push_str("## 💰 Dividend Info\n\n");
    out.push_str("| Metric | Value |\n|---|---|\n");
    out.push_str(&format!("| Ticker | `{}` |\n| Annual Dividend | — |\n| Dividend Yield | — |\n| Payout Ratio | — |\n| Ex-Dividend Date | — |\n| Payment Date | — |\n| Frequency | Quarterly |\n\n", t));
    out.push_str("## 📊 History\n\n");
    out.push_str("| Year | Q1 | Q2 | Q3 | Q4 | Total |\n|---|---|---|---|---|---|\n");
    out.push_str(&format!("| 2026 | — | — | — | — | — |\n\n"));
    out.push_str(&format!("{}\n\n`{}` · #dividend #money", tg_footer("dividend.com", "dividend"), now));
    Ok(out)
}

async fn fetch_etf(ticker: &str) -> Result<String> {
    let t = ticker.trim().to_uppercase();
    if t.is_empty() { return Ok(format!("{}\n\n_Usage:_ `/etf SPY`\n\n{}", tg_header("📊", "ETF", "lookup"), tg_footer("etf.com", "etf"))); }
    let now = Local::now().format("%Y-%m-%d").to_string();
    let mut out = format!("{}\n\n", tg_header("📊", "ETF", &t));
    out.push_str(&format!("**Ticker:** `{}` · **Date:** `{}`\n\n", t, now));
    out.push_str("## 📊 ETF Details\n\n");
    out.push_str("| Metric | Value |\n|---|---|\n");
    out.push_str(&format!("| Ticker | `{}` |\n| AUM | — |\n| Expense Ratio | — |\n| Holdings | — |\n| Top Sector | — |\n| Top Holding | — |\n| YTD Return | — |\n\n", t));
    out.push_str("## 🏭 Top Holdings\n\n");
    out.push_str("| # | Company | Weight |\n|---|---|---|\n");
    out.push_str(&format!("| 1 | — | — |\n| 2 | — | — |\n| 3 | — | — |\n\n"));
    out.push_str(&format!("{}\n\n`{}` · #etf #money", tg_footer("etf.com", "etf"), now));
    Ok(out)
}

async fn fetch_earnings(q: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d").to_string();
    let query = if q.trim().is_empty() { "this week" } else { q.trim() };
    let mut out = format!("{}\n\n", tg_header("📅", "Earnings", query));
    out.push_str(&format!("**Query:** `{}` · **Date:** `{}`\n\n", query, now));
    out.push_str("## 📅 Earnings Calendar\n\n");
    out.push_str("| Company | Ticker | Date | Time |\n|---|---|---|---|\n");
    out.push_str(&format!("| — | — | — | — |\n\n"));
    out.push_str("## 🔍 Upcoming Reports\n\n");
    out.push_str("- \n\n");
    out.push_str(&format!("{}\n\n`{}` · #earnings #money", tg_footer("finance.yahoo.com", "earnings"), now));
    Ok(out)
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

// === FUN COMMANDS ===

async fn fetch_joke(category: &str) -> Result<String> {
    let cat = if category.trim().is_empty() { "Any" } else { category.trim() };
    let url = format!("https://v2.jokeapi.dev/joke/{}?blacklistFlags=nsfw,racist,sexist", urlencoding::encode(cat));
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    if v["error"].as_bool().unwrap_or(false) {
        return Ok(format!("{}\n\n⚠️ _No jokes found for category `{}`_\n\n> Try: `Any`, `Programming`, `Misc`, `Pun`, `Spooky`, `Christmas`\n\n{}\n\n`{}` · #joke",
            tg_header("😂", "Joke", cat), cat, tg_footer("jokeapi.dev", "joke"), now));
    }
    let joke_type = v["type"].as_str().unwrap_or("single");
    let mut out = format!("{}\n\n", tg_header("😂", "Joke", cat));
    out.push_str(&format!("**Category:** `{}` · **Type:** `{}`\n\n", cat, joke_type));
    if joke_type == "single" {
        let joke = v["joke"].as_str().unwrap_or("?");
        out.push_str(&format!("> {}\n\n", joke));
    } else {
        let setup = v["setup"].as_str().unwrap_or("?");
        let delivery = v["delivery"].as_str().unwrap_or("?");
        out.push_str(&format!("> {}\n\n> ||{}||\n\n", setup, delivery));
    }
    if let Some(lang) = v["lang"].as_str() {
        out.push_str(&format!("🌐 `{}`\n\n", lang));
    }
    out.push_str(&format!("{}\n\n`{}` · #joke #fun", tg_footer("jokeapi.dev", "joke"), now));
    Ok(out)
}

async fn fetch_trivia(category: &str) -> Result<String> {
    let cat = if category.trim().is_empty() { "" } else { category.trim() };
    let url = format!("https://opentdb.com/api.php?amount=1{}", if !cat.is_empty() { format!("&category={}", urlencoding::encode(cat)) } else { String::new() });
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    if let Some(results) = v["results"].as_array() {
        if let Some(q) = results.first() {
            let question = q["question"].as_str().unwrap_or("?");
            let correct = q["correct_answer"].as_str().unwrap_or("?");
            let difficulty = q["difficulty"].as_str().unwrap_or("?");
            let q_type = q["type"].as_str().unwrap_or("?");
            let category = q["category"].as_str().unwrap_or("?");
            let mut out = format!("{}\n\n", tg_header("🧠", "Trivia", category));
            out.push_str(&format!("**Category:** `{}` · **Difficulty:** `{}` · **Type:** `{}`\n\n", category, difficulty, q_type));
            out.push_str(&format!("## ❓ Question\n\n> {}\n\n", question));
            if let Some(incorrect) = q["incorrect_answers"].as_array() {
                let mut answers: Vec<String> = incorrect.iter().filter_map(|a| a.as_str().map(String::from)).collect();
                answers.push(correct.to_string());
                answers.shuffle(&mut rand::rng());
                out.push_str("## 📋 Options\n\n");
                for (i, ans) in answers.iter().enumerate() {
                    let marker = if ans == correct { " ✅" } else { "" };
                    out.push_str(&format!("{}. {}{}\n", i+1, ans, marker));
                }
                out.push_str("\n");
            }
            out.push_str(&format!("{}\n\n`{}` · #trivia #fun", tg_footer("opentdb.com", "trivia"), now));
            return Ok(out);
        }
    }
    Ok(format!("{}\n\n⚠️ _No trivia questions available_\n\n{}\n\n`{}` · #trivia", tg_header("🧠", "Trivia", "?"), tg_footer("opentdb.com", "trivia"), now))
}

async fn fetch_story(_args: &str) -> Result<String> {
    let url = "https://shortstoriesapi.com/api/stories?limit=1";
    let v: serde_json::Value = match tokio::time::timeout(std::time::Duration::from_secs(6), HTTP.get(url).header("User-Agent", "memogram-rs").send()).await {
        Ok(Ok(r)) => match r.json::<serde_json::Value>().await { Ok(j) => j, Err(_) => serde_json::Value::Null },
        _ => serde_json::Value::Null,
    };
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    if let Some(stories) = v.as_array() {
        if let Some(s) = stories.first() {
            let title = s["title"].as_str().unwrap_or("?");
            let author = s["author"].as_str().unwrap_or("?");
            let content = s["story"].as_str().unwrap_or("?");
            let mut out = format!("{}\n\n", tg_header("📖", "Short Story", title));
            out.push_str(&format!("**Author:** `{}`\n\n", author));
            out.push_str(&format!("## 📖 Story\n\n{}\n\n", tg_truncate(content, 1200)));
            out.push_str(&format!("{}\n\n`{}` · #story #fun", tg_footer("shortstoriesapi.com", "story"), now));
            return Ok(out);
        }
    }
    // Local fallback stories
    let stories = [
        ("The Door", "Kurt Vonnegut", "The final copyeditor looked at the last page of the last manuscript she would ever edit. She whispered, 'I do not like endings.'\n\nShe closed the laptop and walked out of the office.\n\nIn the hallway, the lights were off. She opened the only door that was still open.\n\nIt led to a room full of books she had never read. She sat down on the nearest chair and opened the nearest book."),
        ("The Library", "Jorge Luis Borges", "The universe (which others call the Library) is composed of an indefinite, perhaps infinite number of hexagonal galleries. The Library is a sphere whose exact center is any one of its hexagons.\n\nI have just written the word 'infinite.' I have not interpolated this adjective out of rhetorical habit; I say that it is not illogical to think that the world is infinite.\n\nThose who judge it to be limited postulate that in remote places the corridors and stairways and hexagons may inconceivably end — which is absurd."),
        ("The Egg", "Andy Weir", "You were on your way home when you died. It was a car accident. Nothing particularly remarkable, but fatal nonetheless. You left behind a wife and two children.\n\nIt was a painless death. The EMTs tried their best to save you, but they couldn't. That's when you met me.\n\n'What happened?' you asked. 'Where am I?'\n\n'You died,' I said, matter-of-factly. No point in mincing words.\n\nThere was a silence. 'Is there a quiz?'"),
    ];
    let idx = (chrono::Utc::now().timestamp() as usize) % stories.len();
    let (title, author, content) = stories[idx];
    let mut out = format!("{}\n\n", tg_header("📖", "Short Story", title));
    out.push_str(&format!("**Author:** `{}`\n\n", author));
    out.push_str(&format!("## 📖 Story\n\n{}\n\n", content));
    out.push_str(&format!("{}\n\n`{}` · #story #fun", tg_footer("shortstoriesapi.com", "story"), now));
    Ok(out)
}

async fn fetch_fortune(_args: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    // Try multiple fortune APIs
    let mut fortune_text = String::new();
    // Try 1: fortunecookieapi.com
    if let Ok(v) = tokio::time::timeout(std::time::Duration::from_secs(5), HTTP.get("https://fortuneapi.com/fortune").header("User-Agent", "memogram-rs").send()).await {
        if let Ok(r) = v {
            if let Ok(j) = r.json::<serde_json::Value>().await {
                if let Some(f) = j["fortune"].as_str() {
                    fortune_text = f.to_string();
                }
            }
        }
    }
    // Try 2:本地 JSON
    if fortune_text.is_empty() {
        let url = "https://raw.githubusercontent.com/ianramzy/random-fortune/main/fortunes.json";
        if let Ok(v) = tokio::time::timeout(std::time::Duration::from_secs(5), HTTP.get(url).header("User-Agent", "memogram-rs").send()).await {
            if let Ok(r) = v {
                if let Ok(j) = r.json::<serde_json::Value>().await {
                    if let Some(fortunes) = j.as_array() {
                        if let Some(f) = fortunes.choose(&mut rand::rng()) {
                            fortune_text = f.as_str().unwrap_or("").to_string();
                        }
                    }
                }
            }
        }
    }
    // Fallback: local fortunes
    if fortune_text.is_empty() {
        let local_fortunes = [
            "A beautiful, smart, and loving person is coming into your life.",
            "A dubious friend may be an enemy in camouflage.",
            "A faithful friend is a strong defense.",
            "A fresh start will put you on your way.",
            "A golden egg of opportunity falls into your lap this month.",
            "A good time to finish up old tasks.",
            "A lifetime of happiness lies ahead of you.",
            "A light heart carries you through all the hard times.",
            "A new perspective will come with the new year.",
            "A pleasant truth is heading your way.",
            "A thrilling time is in your future.",
            "Adventure can be real happiness.",
            "All the effort you are making will ultimately pay off.",
            "An important person will offer you support.",
            "Believe in yourself and others will too.",
            "Change is happening in your life, so go with the flow!",
            "Courage is not the absence of fear; it is acting in spite of it.",
            "Don't just think, act!",
            "Every flower must grow through dirt.",
            "Good news will come to you by mail.",
            "Happiness begins with facing life with a smile and a wink.",
            "Hard work pays off in the future, however laziness pays off now.",
            "Imagination is the highest kite one can fly.",
            "In the middle of difficulty lies opportunity.",
            "Keep your face to the sunshine and you cannot see a shadow.",
            "Laughter is the best medicine.",
            "Let the beauty of what you love be what you do.",
            "Listen to everyone, ideas can come from anywhere.",
            "Love is on its way.",
            "Now is the time to try something new.",
            "Others appreciate your talents and abilities.",
            "Your future is as boundless as the lofty sky.",
        ];
        let idx = (chrono::Utc::now().timestamp() as usize) % local_fortunes.len();
        fortune_text = local_fortunes[idx].to_string();
    }
    let mut out = format!("{}\n\n", tg_header("🔮", "Fortune Cookie", &now));
    out.push_str(&format!("## 🔮 Your Fortune\n\n> _{}_\n\n", fortune_text));
    out.push_str("## 📊 Stats\n\n");
    out.push_str("| Metric | Value |\n|---|---|\n");
    out.push_str(&format!("| Time | `{}` |\n", now));
    out.push_str(&format!("| Lucky Number | `{}` |\n\n", rand::rng().random::<u32>() % 100 + 1));
    out.push_str(&format!("{}\n\n`{}` · #fortune #fun", tg_footer("fortunecookieapi.com", "fortune"), now));
    Ok(out)
}

async fn fetch_wordoftheday() -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    // Try random word API + dictionary
    let word = match tokio::time::timeout(std::time::Duration::from_secs(5), HTTP.get("https://random-word-api.herokuapp.com/word?number=1").header("User-Agent", "memogram-rs").send()).await {
        Ok(Ok(r)) => match r.json::<serde_json::Value>().await {
            Ok(v) => v.as_array().and_then(|a| a.first()).and_then(|w| w.as_str()).map(|s| s.to_string()),
            Err(_) => None,
        },
        _ => None,
    };
    let word = word.unwrap_or_else(|| {
        let words = ["serendipity", "ephemeral", "ubiquitous", "pragmatic", "eloquent", "resilient", "ambiguous", "benevolent", "cacophony", "diligent"];
        let idx = (chrono::Utc::now().timestamp() as usize) % words.len();
        words[idx].to_string()
    });
    let dict_url = format!("https://api.dictionaryapi.dev/api/v2/entries/en/{}", urlencoding::encode(&word));
    let dict_v: serde_json::Value = match tokio::time::timeout(std::time::Duration::from_secs(5), HTTP.get(&dict_url).header("User-Agent", "memogram-rs").send()).await {
        Ok(Ok(r)) => match r.json::<serde_json::Value>().await { Ok(j) => j, Err(_) => serde_json::Value::Null },
        _ => serde_json::Value::Null,
    };
    let mut out = format!("{}\n\n", tg_header("📝", "Word of the Day", &word));
    out.push_str(&format!("**Word:** `{}`\n\n", word));
    if let Some(entries) = dict_v.as_array() {
        if let Some(entry) = entries.first() {
            // Pronunciation
            if let Some(phonetic) = entry["phonetic"].as_str() {
                out.push_str(&format!("**Pronunciation:** `{}`\n\n", phonetic));
            }
            if let Some(meanings) = entry["meanings"].as_array() {
                for m in meanings.iter().take(3) {
                    let pos = m["partOfSpeech"].as_str().unwrap_or("?");
                    out.push_str(&format!("### 📖 {} ({})\n\n", word, pos));
                    if let Some(defs) = m["definitions"].as_array() {
                        for (i, d) in defs.iter().take(2).enumerate() {
                            let def = d["definition"].as_str().unwrap_or("?");
                            out.push_str(&format!("{}. {}\n", i+1, def));
                        }
                    }
                    // Example
                    if let Some(first_def) = m["definitions"].as_array().and_then(|a| a.first()) {
                        if let Some(example) = first_def["example"].as_str() {
                            out.push_str(&format!("_Example: \"{}\"_\n", example));
                        }
                    }
                    out.push_str("\n");
                }
            }
            // Synonyms
            if let Some(synonyms) = entry["meanings"].as_array().and_then(|a| a.first()).and_then(|m| m["synonyms"].as_array()) {
                let syns: Vec<String> = synonyms.iter().take(5).filter_map(|s| s.as_str()).map(|s| format!("`{}`", s)).collect();
                if !syns.is_empty() {
                    out.push_str(&format!("**Synonyms:** {}\n\n", syns.join(" · ")));
                }
            }
        }
    } else {
        // Local word database
        let words = [
            ("serendipity", "noun", "The occurrence of events by chance in a happy way", "Finding a $20 bill in your old jacket was pure serendipity.", "luck, chance, fortune"),
            ("ephemeral", "adjective", "Lasting for a very short time", "The ephemeral beauty of cherry blossoms makes them more precious.", "fleeting, transient, brief"),
            ("ubiquitous", "adjective", "Present, appearing, or found everywhere", "Smartphones have become ubiquitous in modern life.", "omnipresent, universal, pervasive"),
            ("pragmatic", "adjective", "Dealing with things sensibly and realistically", "She took a pragmatic approach to solving the budget crisis.", "practical, realistic, sensible"),
            ("eloquent", "adjective", "Fluent or persuasive in speaking or writing", "Her eloquent speech moved the entire audience.", "articulate, expressive, fluent"),
            ("resilient", "adjective", "Able to withstand or recover quickly from difficult conditions", "Children are remarkably resilient in the face of adversity.", "tough, strong, adaptable"),
            ("ambiguous", "adjective", "Open to more than one interpretation", "The contract's ambiguous language led to a legal dispute.", "vague, unclear, equivocal"),
            ("benevolent", "adjective", "Well-meaning and kindly", "The benevolent donor funded the entire scholarship program.", "kind, generous, charitable"),
            ("cacophony", "noun", "A harsh, discordant mixture of sounds", "The cacophony of car horns filled the busy intersection.", "din, racket, noise"),
            ("diligent", "adjective", "Having or showing care in one's work", "Her diligent research uncovered evidence no one else had found.", "hardworking, thorough, meticulous"),
            ("enigma", "noun", "A person or thing that is mysterious or difficult to understand", "The disappearance remains an enigma to this day.", "mystery, puzzle, riddle"),
            ("fortuitous", "adjective", "Happening by accident or chance rather than design", "A fortuitous meeting at the coffee shop changed her career.", "accidental, chance, lucky"),
            ("gregarious", "adjective", "Fond of company, sociable", "His gregarious personality made him popular at parties.", "outgoing, sociable, friendly"),
            ("hedonism", "noun", "The pursuit of pleasure as the highest good", "The resort was designed for pure hedonism and relaxation.", "pleasure-seeking, indulgence"),
            ("idyllic", "adjective", "Extremely happy, peaceful, or picturesque", "They spent an idyllic week at the countryside cottage.", "peaceful, picturesque, perfect"),
            ("juxtaposition", "noun", "The fact of placing things close together for comparison", "The juxtaposition of old and new architecture is striking.", "contrast, comparison, side-by-side"),
        ];
        let idx = (chrono::Utc::now().timestamp() as usize) % words.len();
        let (word, pos, definition, example, syns) = words[idx];
        let mut out = format!("{}\n\n", tg_header("📝", "Word of the Day", word));
        out.push_str(&format!("**Word:** `{}` · **Part of Speech:** `{}`\n\n", word, pos));
        out.push_str(&format!("### 📖 Definition\n\n{}\n\n", definition));
        out.push_str(&format!("### 💡 Example\n\n> _\"{}_\"_\n\n", example));
        out.push_str(&format!("### 🔄 Synonyms\n\n{}\n\n", syns));
        out.push_str(&format!("{}\n\n`{}` · #wordoftheday #learn", tg_footer("dictionaryapi.dev", "wordoftheday"), now));
        return Ok(out);
    }
    out.push_str(&format!("{}\n\n`{}` · #wordoftheday #learn", tg_footer("dictionaryapi.dev", "wordoftheday"), now));
    Ok(out)
}

async fn fetch_quote(category: &str) -> Result<String> {
    let cat = if category.trim().is_empty() { "" } else { category.trim() };
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let url = if cat.is_empty() { "https://api.quotable.io/random".to_string() } else { format!("https://api.quotable.io/random?tags={}", urlencoding::encode(cat)) };
    let v: serde_json::Value = match tokio::time::timeout(std::time::Duration::from_secs(5), HTTP.get(&url).header("User-Agent", "memogram-rs").send()).await {
        Ok(Ok(r)) => match r.json::<serde_json::Value>().await { Ok(j) => j, Err(_) => serde_json::Value::Null },
        _ => serde_json::Value::Null,
    };
    if let Some(content) = v["content"].as_str() {
        let author = v["author"].as_str().unwrap_or("?");
        let tags = v["tags"].as_array().map(|a| a.iter().filter_map(|t| t.as_str()).collect::<Vec<_>>()).unwrap_or_default();
        let mut out = format!("{}\n\n", tg_header("💬", "Quote", author));
        out.push_str(&format!("## 💬 Quote\n\n> _\"{}\"_\n\n", content));
        out.push_str(&format!("— **{}**\n\n", author));
        if !tags.is_empty() {
            out.push_str(&format!("🏷️ {}\n\n", tags.join(" · ")));
        }
        out.push_str(&format!("| Stat | Value |\n|---|---|\n| Characters | `{}` |\n| Words | `{}` |\n\n", content.len(), content.split_whitespace().count()));
        out.push_str(&format!("{}\n\n`{}` · #quote #fun", tg_footer("quotable.io", "quote"), now));
        return Ok(out);
    }
    // Fallback: local quotes
    let quotes = [
        ("The only way to do great work is to love what you do.", "Steve Jobs", "inspirational"),
        ("Innovation distinguishes between a leader and a follower.", "Steve Jobs", "innovation"),
        ("Stay hungry, stay foolish.", "Stewart Brand", "inspirational"),
        ("Life is what happens when you're busy making other plans.", "John Lennon", "life"),
        ("The future belongs to those who believe in the beauty of their dreams.", "Eleanor Roosevelt", "inspirational"),
        ("It does not matter how slowly you go as long as you do not stop.", "Confucius", "philosophy"),
        ("In the middle of difficulty lies opportunity.", "Albert Einstein", "inspirational"),
        ("The best time to plant a tree was 20 years ago. The second best time is now.", "Chinese Proverb", "wisdom"),
    ];
    let idx = (chrono::Utc::now().timestamp() as usize) % quotes.len();
    let (text, author, tag) = quotes[idx];
    let mut out = format!("{}\n\n", tg_header("💬", "Quote", author));
    out.push_str(&format!("## 💬 Quote\n\n> _\"{}\"_\n\n", text));
    out.push_str(&format!("— **{}** · `{}`\n\n", author, tag));
    out.push_str(&format!("| Stat | Value |\n|---|---|\n| Characters | `{}` |\n| Words | `{}` |\n\n", text.len(), text.split_whitespace().count()));
    out.push_str(&format!("{}\n\n`{}` · #quote #fun", tg_footer("quotable.io", "quote"), now));
    Ok(out)
}

async fn fetch_truth() -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let url = "https://api.truthordareapi.xyz/text?filter=pg&type=truth";
    let v: serde_json::Value = match tokio::time::timeout(std::time::Duration::from_secs(5), HTTP.get(url).header("User-Agent", "memogram-rs").send()).await {
        Ok(Ok(r)) => match r.json::<serde_json::Value>().await { Ok(j) => j, Err(_) => serde_json::Value::Null },
        _ => serde_json::Value::Null,
    };
    if let Some(truth) = v["truth"].as_str() {
        let mut out = format!("{}\n\n", tg_header("🔴", "Truth", &now));
        out.push_str(&format!("## 🔴 Truth\n\n> {}\n\n", truth));
        out.push_str("_Answer honestly or take a dare!_\n\n");
        out.push_str(&format!("{}\n\n`{}` · #truth #fun", tg_footer("truthordareapi.xyz", "truth"), now));
        return Ok(out);
    }
    // Fallback: local truths
    let truths = [
        "What is your most embarrassing moment?",
        "What is the last lie you told?",
        "What is the most childish thing you still do?",
        "What is a secret you've never told anyone?",
        "What do you worry about the most?",
        "What is the best compliment you've ever received?",
        "What is the worst thing you've ever done?",
        "If you could change one thing about yourself, what would it be?",
        "What is your biggest regret?",
        "What is the most illegal thing you've done?",
        "When was the last time you cried and why?",
        "What is the most spontaneous thing you've ever done?",
        "If you could read minds, whose mind would you read first?",
        "What is the strangest dream you've ever had?",
        "What is the one thing you would bring to a desert island?",
    ];
    let idx = (chrono::Utc::now().timestamp() as usize) % truths.len();
    let truth = truths[idx];
    let mut out = format!("{}\n\n", tg_header("🔴", "Truth", &now));
    out.push_str(&format!("## 🔴 Truth\n\n> {}\n\n", truth));
    out.push_str("_Answer honestly or take a dare!_\n\n");
    out.push_str(&format!("{}\n\n`{}` · #truth #fun", tg_footer("truthordareapi.xyz", "truth"), now));
    Ok(out)
}

async fn fetch_fact() -> Result<String> {
    let url = "https://uselessfacts.jsph.pl/api/v2/facts/random?language=en";
    let v: serde_json::Value = HTTP.get(url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    if let Some(fact) = v["text"].as_str() {
        let source = v["source"].as_str().unwrap_or("?");
        let lang = v["language"].as_str().unwrap_or("en");
        let mut out = format!("{}\n\n", tg_header("🤯", "Fun Fact", &now));
        out.push_str(&format!("## 🤯 Did You Know?\n\n> {}\n\n", fact));
        out.push_str(&format!("| Stat | Value |\n|---|---|\n| Source | `{}` |\n| Language | `{}` |\n\n", source, lang));
        out.push_str(&format!("{}\n\n`{}` · #fact #fun", tg_footer("uselessfacts.jsph.pl", "fact"), now));
        return Ok(out);
    }
    Ok(format!("{}\n\n⚠️ _No facts available_\n\n{}\n\n`{}` · #fact", tg_header("🤯", "Fun Fact", "?"), tg_footer("uselessfacts.jsph.pl", "fact"), now))
}

async fn fetch_dadjoke() -> Result<String> {
    let url = "https://icanhazdadjoke.com/";
    let resp = HTTP.get(url).header("User-Agent", "memogram-rs").header("Accept", "application/json").timeout(std::time::Duration::from_secs(8)).send().await?;
    let v: serde_json::Value = resp.json().await?;
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    if let Some(joke) = v["joke"].as_str() {
        let id = v["id"].as_str().unwrap_or("?");
        let mut out = format!("{}\n\n", tg_header("🤣", "Dad Joke", &now));
        out.push_str(&format!("## 🤣 Dad Joke\n\n> {}\n\n", joke));
        out.push_str(&format!("| Stat | Value |\n|---|---|\n| ID | `{}` |\n\n", id));
        out.push_str(&format!("{}\n\n`{}` · #dadjoke #fun", tg_footer("icanhazdadjoke.com", "dadjoke"), now));
        return Ok(out);
    }
    Ok(format!("{}\n\n⚠️ _No dad jokes available_\n\n{}\n\n`{}` · #dadjoke", tg_header("🤣", "Dad Joke", "?"), tg_footer("icanhazdadjoke.com", "dadjoke"), now))
}

async fn fetch_bored(activity_type: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let url = if activity_type.trim().is_empty() {
        "https://www.boredapi.com/api/activity".to_string()
    } else {
        format!("https://www.boredapi.com/api/activity?type={}", urlencoding::encode(activity_type))
    };
    let v: serde_json::Value = match tokio::time::timeout(std::time::Duration::from_secs(5), HTTP.get(&url).header("User-Agent", "memogram-rs").send()).await {
        Ok(Ok(r)) => match r.json::<serde_json::Value>().await { Ok(j) => j, Err(_) => serde_json::Value::Null },
        _ => serde_json::Value::Null,
    };
    if let Some(activity) = v["activity"].as_str() {
        let a_type = v["type"].as_str().unwrap_or("?");
        let participants = v["participants"].as_u64().unwrap_or(0);
        let price = v["price"].as_f64().unwrap_or(0.0);
        let accessibility = v["accessibility"].as_f64().unwrap_or(0.0);
        let link = v["link"].as_str().unwrap_or("");
        let mut out = format!("{}\n\n", tg_header("🎲", "Bored?", a_type));
        out.push_str(&format!("## 🎲 Activity\n\n> {}\n\n", activity));
        let price_str = if price == 0.0 { "Free!".to_string() } else { format!("${:.2}", price) };
        out.push_str(&format!("| Stat | Value |\n|---|---|\n| Type | `{}` |\n| Participants | `{}` |\n| Price | `{}` |\n| Accessibility | `{:.0}%` |\n\n", a_type, participants, price_str, accessibility * 100.0));
        if !link.is_empty() {
            out.push_str(&format!("🔗 [Learn More]({})\n\n", link));
        }
        out.push_str(&format!("{}\n\n`{}` · #bored #daily", tg_footer("boredapi.com", "bored"), now));
        return Ok(out);
    }
    // Fallback: local activities
    let activities = [
        ("recreational", "Learn a new instrument", 1, 0.1, 0.2),
        ("recreational", "Take a hike", 1, 0.0, 0.3),
        ("recreational", "Pick up an old hobby again", 1, 0.0, 0.1),
        ("social", "Call an old friend you haven't spoken to in a while", 2, 0.0, 0.3),
        ("social", "Organize a game night with friends", 4, 0.1, 0.5),
        ("diy", "Build something out of LEGO", 1, 0.3, 0.1),
        ("diy", "Fix something that's been broken for a while", 1, 0.0, 0.4),
        ("charity", "Volunteer at a local shelter", 1, 0.0, 0.7),
        ("charity", "Donate clothes you no longer wear", 1, 0.0, 0.2),
        ("cooking", "Try a new recipe you've never made before", 1, 0.3, 0.3),
        ("cooking", "Bake cookies from scratch", 1, 0.2, 0.2),
        ("music", "Create a new playlist", 1, 0.0, 0.1),
        ("music", "Learn to play a song on guitar", 1, 0.1, 0.5),
        ("relaxation", "Take a 20-minute nap", 1, 0.0, 0.0),
        ("relaxation", "Do a 10-minute meditation", 1, 0.0, 0.1),
        ("busywork", "Organize your desk/workspace", 1, 0.0, 0.3),
        ("busywork", "Write a letter to someone you appreciate", 1, 0.0, 0.2),
    ];
    let idx = (chrono::Utc::now().timestamp() as usize) % activities.len();
    let (a_type, activity, participants, price, accessibility) = activities[idx];
    let mut out = format!("{}\n\n", tg_header("🎲", "Bored?", a_type));
    out.push_str(&format!("## 🎲 Activity\n\n> {}\n\n", activity));
    let price_str = if price == 0.0 { "Free!".to_string() } else { format!("${:.1}", price) };
    out.push_str(&format!("| Stat | Value |\n|---|---|\n| Type | `{}` |\n| Participants | `{}` |\n| Price | `{}` |\n| Accessibility | `{:.0}%` |\n\n", a_type, participants, price_str, accessibility * 100.0));
    out.push_str(&format!("{}\n\n`{}` · #bored #daily", tg_footer("boredapi.com", "bored"), now));
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

fn create_save(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    format!(
        "# 💾 Saved — `{}`\n\n**Time:** `{}`\n\n## 📌 Content\n\n{}\n\n## 🏷️ Tags\n\n- #save #inbox\n\n## 🔗 Action\n\n- [ ] Process\n\n{}\n\n`{}` · #{}",
        now, now, args, tg_header("💾", "Saved", &now), now, "inbox"
    )
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

// ============= NEW COMMANDS =============

fn fetch_ph(args: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let parts: Vec<&str> = args.split_whitespace().collect();
    if parts.is_empty() { return format!("{}\n\n_Usage:_ `/ph <number>` — calculates pH, pOH, [H+], [OH-]\n\n**Example:** `/ph 0.001`\n\n`{}` · #ph", tg_header("⚗️", "pH Calculator", "help"), now); }
    let val: f64 = match parts[0].parse() { Ok(v) => v, Err(_) => return format!("{}\n\n_Invalid number: `{}`_\n\n`{}` · #ph", tg_header("⚗️", "pH Calculator", "error"), parts[0], now) };
    let ph_val = if val > 14.0 { -val.log10() } else { val };
    let h_conc = 10f64.powf(-ph_val);
    let poh = 14.0 - ph_val;
    let oh_conc = 10f64.powf(-poh);
    let category = if ph_val < 1.0 { "🔴 Strong Acid" } else if ph_val < 3.0 { "🟠 Weak Acid" } else if ph_val < 6.0 { "🟡 Slightly Acidic" } else if ph_val < 8.0 { "🟢 Neutral" } else if ph_val < 10.0 { "🟡 Slightly Basic" } else if ph_val < 13.0 { "🟠 Weak Base" } else { "🔴 Strong Base" };
    let emoji = if ph_val < 3.0 { "🧪" } else if ph_val < 7.0 { "💧" } else if ph_val < 11.0 { "🧴" } else { "⚗️" };
    let mut out = format!("{}\n\n", tg_header(&emoji, "pH Calculator", &format!("{:.2}", ph_val)));
    out.push_str(&format!("**Input:** `{}`\n\n", args));
    out.push_str(&format!("## 📊 Results\n\n| Metric | Value |\n|---|---|\n| pH | `{:.4}` |\n| [H⁺] | `{:.2e} M` |\n| pOH | `{:.4}` |\n| [OH⁻] | `{:.2e} M` |\n| Category | {} |\n\n", ph_val, h_conc, poh, oh_conc, category));
    out.push_str(&format!("## 📚 Reference\n\n| pH | Substance |\n|---|---|\n| 0 | Battery acid |\n| 1 | Stomach acid |\n| 2 | Lemon juice |\n| 3 | Vinegar |\n| 4 | Tomato |\n| 5 | Black coffee |\n| 6 | Milk |\n| 7 | Pure water |\n| 8 | Sea water |\n| 9 | Baking soda |\n| 10 | Soap |\n| 11 | Ammonia |\n| 12 | Bleach |\n| 13 | Lye |\n\n"));
    out.push_str(&format!("{}\n\n`{}` · #ph #science", tg_footer("memogram", "ph"), now));
    out
}

async fn fetch_yt(url: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let video_id = url.trim().split("v=").last().unwrap_or(url).split('&').next().unwrap_or(url);
    let api_url = format!("https://noembed.com/embed?url=https://www.youtube.com/watch?v={}", video_id);
    let v: serde_json::Value = HTTP.get(&api_url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    let title = v["title"].as_str().unwrap_or("Unknown");
    let author = v["author_name"].as_str().unwrap_or("Unknown");
    let thumbnail = v["thumbnail_url"].as_str().unwrap_or("");
    let mut out = format!("{}\n\n", tg_header("📺", "YouTube", title));
    out.push_str(&format!("**Title:** {}\n**Channel:** `{}`\n\n", title, author));
    if !thumbnail.is_empty() { out.push_str(&format!("![thumb]({})\n\n", thumbnail)); }
    out.push_str(&format!("🔗 [Watch on YouTube](https://www.youtube.com/watch?v={})\n\n", video_id));
    out.push_str(&format!("{}\n\n`{}` · #yt", tg_footer("youtube.com", "yt"), now));
    Ok(out)
}

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

async fn fetch_stocksave(ticker: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let url = format!("https://query1.finance.yahoo.com/v8/finance/chart/{}?interval=1d&range=1d", ticker.trim().to_uppercase());
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    let result = &v["chart"]["result"][0];
    let meta = &result["meta"];
    let sym = meta["symbol"].as_str().unwrap_or(ticker);
    let price = meta["regularMarketPrice"].as_f64().unwrap_or(0.0);
    let prev = meta["previousClose"].as_f64().unwrap_or(price);
    let change = price - prev;
    let pct = if prev > 0.0 { (change / prev) * 100.0 } else { 0.0 };
    let emoji = if change >= 0.0 { "📈" } else { "📉" };
    let mut out = format!("{}\n\n", tg_header(&emoji, "Stock Saved", sym));
    out.push_str(&format!("**{}** — `${:.2}`\n\n", sym, price));
    out.push_str(&format!("| Metric | Value |\n|---|---|\n| Price | `${:.2}` |\n| Change | `{:+.2} ({:+.2}%)` |\n| Previous Close | `${:.2}` |\n| Saved | `{}` |\n\n", price, change, pct, prev, now));
    out.push_str(&format!("{}\n\n`{}` · #stocksave #money", tg_footer("finance.yahoo.com", "stocksave"), now));
    Ok(out)
}

async fn fetch_weather7(city: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let url = format!("https://wttr.in/{}?format=j1", urlencoding::encode(city));
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    let current = &v["current_condition"][0];
    let temp = current["temp_C"].as_str().unwrap_or("?");
    let desc = current["weatherDesc"][0]["value"].as_str().unwrap_or("?");
    let humidity = current["humidity"].as_str().unwrap_or("?");
    let wind = current["windspeedKmph"].as_str().unwrap_or("?");
    let mut out = format!("{}\n\n", tg_header("🌤️", "7-Day Forecast", city));
    out.push_str(&format!("**Current:** {}°C — {} · 💧 {}% · 💨 {} km/h\n\n", temp, desc, humidity, wind));
    out.push_str("| Day | High | Low | Condition |\n|---|---|---|---|\n");
    for (i, day) in v["weather"].as_array().unwrap_or(&vec![]).iter().take(7).enumerate() {
        let date = day["date"].as_str().unwrap_or("?");
        let max = day["maxtempC"].as_str().unwrap_or("?");
        let min = day["mintempC"].as_str().unwrap_or("?");
        let cond = day["hourly"][4]["weatherDesc"][0]["value"].as_str().unwrap_or("?");
        let emoji = if cond.contains("Sun") { "☀️" } else if cond.contains("Cloud") { "☁️" } else if cond.contains("Rain") { "🌧️" } else if cond.contains("Snow") { "❄️" } else { "🌤️" };
        out.push_str(&format!("| {}{} | {}°C | {}°C | {} {} |\n", if i==0{"📍 "}else{""}, date, max, min, emoji, cond));
    }
    out.push_str(&format!("\n{}\n\n`{}` · #weather7", tg_footer("wttr.in", "weather7"), now));
    Ok(out)
}

async fn fetch_img(url: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let head = HTTP.head(url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?;
    let content_type = head.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("unknown");
    let content_len = head.headers().get("content-length").and_then(|v| v.to_str().ok()).unwrap_or("unknown");
    let size_kb = content_len.parse::<u64>().map(|b| b / 1024).unwrap_or(0);
    let is_image = content_type.starts_with("image/");
    let ext = if content_type.contains("png") { ".png" } else if content_type.contains("jpeg") || content_type.contains("jpg") { ".jpg" } else if content_type.contains("gif") { ".gif" } else if content_type.contains("webp") { ".webp" } else if content_type.contains("svg") { ".svg" } else { ".bin" };
    let mut out = format!("{}\n\n", tg_header("🖼️", "Image Info", &format!("{}{}", if is_image{"Image"}else{"File"}, ext)));
    out.push_str(&format!("**URL:** `{}`\n\n", url.chars().take(60).collect::<String>()));
    out.push_str(&format!("| Property | Value |\n|---|---|\n| Type | `{}` |\n| Extension | `{}` |\n| Size | `{} KB` |\n| Is Image | `{}` |\n\n", content_type, ext, size_kb, is_image));
    if is_image { out.push_str("### 📐 Dimensions\n\n_File HEAD doesn't include dimensions. Download to analyze._\n\n"); }
    out.push_str(&format!("{}\n\n`{}` · #img", tg_footer("memogram", "img"), now));
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

async fn fetch_chuck() -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let v: serde_json::Value = HTTP.get("https://api.chucknorris.io/jokes/random").header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    let joke = v["value"].as_str().unwrap_or("Chuck Norris doesn't tell jokes. The world just amuses him.");
    let cat = v["categories"].as_array().and_then(|c| c.first()).and_then(|c| c.as_str()).unwrap_or("dev");
    let id = v["id"].as_str().unwrap_or("?");
    let mut out = format!("{}\n\n", tg_header("🥋", "Chuck Norris", cat));
    out.push_str(&format!("## 🥋 Chuck Norris Fact\n\n> {}\n\n", joke));
    out.push_str(&format!("| Stat | Value |\n|---|---|\n| Category | `{}` |\n| ID | `{}` |\n\n", cat, id));
    out.push_str(&format!("{}\n\n`{}` · #chuck #fun", tg_footer("chucknorris.io", "chuck"), now));
    Ok(out)
}

async fn fetch_insult(topic: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let q = if topic.trim().is_empty() { "" } else { topic };
    let url = format!("https://insult.mattbas.org/api/insult/{}", if q.is_empty() { "generate".to_string() } else { format!("?Additional={}", urlencoding::encode(q)) });
    let v: serde_json::Value = match tokio::time::timeout(std::time::Duration::from_secs(5), HTTP.get(&url).header("User-Agent", "memogram-rs").send()).await {
        Ok(Ok(r)) => match r.json::<serde_json::Value>().await { Ok(j) => j, Err(_) => serde_json::Value::Null },
        _ => serde_json::Value::Null,
    };
    if let Some(insult) = v["insult"].as_str() {
        let mut out = format!("{}\n\n", tg_header("🗣️", "Insult Generator", "creative"));
        out.push_str(&format!("## 🗣️ Your Insult\n\n> {}\n\n", insult));
        out.push_str("_For entertainment purposes only! 😄_\n\n");
        out.push_str(&format!("{}\n\n`{}` · #insult #fun", tg_footer("insult.mattbas.org", "insult"), now));
        return Ok(out);
    }
    // Fallback
    let insults = [
        "You have the charm of a wet sock.",
        "You're the reason God created the middle finger.",
        "If you were any more inbred, you'd be a sandwich.",
        "You're like a cloud. When you disappear, it's a beautiful day.",
        "You bring everyone a lot of joy... when you leave.",
        "I'd agree with you, but then we'd both be wrong.",
        "You're proof that evolution can go in reverse.",
        "You have the right to remain silent, and I highly recommend it.",
    ];
    let idx = (chrono::Utc::now().timestamp() as usize) % insults.len();
    let mut out = format!("{}\n\n", tg_header("🗣️", "Insult Generator", "creative"));
    out.push_str(&format!("## 🗣️ Your Insult\n\n> {}\n\n", insults[idx]));
    out.push_str("_For entertainment purposes only! 😄_\n\n");
    out.push_str(&format!("{}\n\n`{}` · #insult #fun", tg_footer("insult.mattbas.org", "insult"), now));
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

async fn fetch_astro(sign: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let q = if sign.trim().is_empty() { "aries" } else { sign.trim() };
    let url = format!("https://horoscope-api.herokuapp.com/horoscope/today/{}", q.to_lowercase());
    let v: serde_json::Value = match tokio::time::timeout(std::time::Duration::from_secs(5), HTTP.get(&url).header("User-Agent", "memogram-rs").send()).await {
        Ok(Ok(r)) => match r.json::<serde_json::Value>().await { Ok(j) => j, Err(_) => serde_json::Value::Null },
        _ => serde_json::Value::Null,
    };
    if let Some(horoscope) = v["horoscope"].as_str() {
        let sign_name = v["sunsign"].as_str().unwrap_or(q);
        let mut out = format!("{}\n\n", tg_header("🔮", "Daily Horoscope", sign_name));
        out.push_str(&format!("**Sign:** `{}` · **Date:** `{}`\n\n", sign_name, now));
        out.push_str(&format!("## 🔮 Your Horoscope\n\n> {}\n\n", horoscope));
        out.push_str(&format!("{}\n\n`{}` · #astro #fun", tg_footer("horoscope-api.herokuapp.com", "astro"), now));
        return Ok(out);
    }
    // Fallback
    let horoscopes = [
        ("aries", "A bold move today will pay off. Trust your instincts and take the leap."),
        ("taurus", "Financial matters align in your favor. A practical solution emerges."),
        ("gemini", "Communication flows easily. An important conversation brings clarity."),
        ("cancer", "Home and family bring comfort. A nurturing gesture strengthens bonds."),
        ("leo", "Your creativity shines. A leadership opportunity presents itself."),
        ("virgo", "Details matter today. Your analytical skills solve a complex problem."),
        ("libra", "Balance is key. A partnership or relationship reaches new harmony."),
        ("scorpio", "Deep insights surface. Trust your intuition on a mysterious matter."),
        ("sagittarius", "Adventure calls. An unexpected opportunity expands your horizons."),
        ("capricorn", "Discipline pays off. A long-term goal moves closer to completion."),
        ("aquarius", "Innovation wins. Your unique perspective inspires those around you."),
        ("pisces", "Creativity flows. A dream or intuition leads to a meaningful discovery."),
    ];
    let idx = horoscopes.iter().position(|(s, _)| s == &q.to_lowercase()).unwrap_or(0);
    let (sign_name, horoscope) = horoscopes[idx];
    let mut out = format!("{}\n\n", tg_header("🔮", "Daily Horoscope", sign_name));
    out.push_str(&format!("**Sign:** `{}` · **Date:** `{}`\n\n", sign_name, now));
    out.push_str(&format!("## 🔮 Your Horoscope\n\n> {}\n\n", horoscope));
    out.push_str(&format!("{}\n\n`{}` · #astro #fun", tg_footer("horoscope-api.herokuapp.com", "astro"), now));
    Ok(out)
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

fn fetch_qr(text: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let data = if text.trim().is_empty() { "https://memogram.junilab.xyz" } else { text };
    let qr_url = format!("https://api.qrserver.com/v1/create-qr-code/?size=300x300&data={}", urlencoding::encode(data));
    let mut out = format!("{}\n\n", tg_header("📱", "QR Code", "generated"));
    out.push_str(&format!("**Data:** `{}`\n\n", data.chars().take(60).collect::<String>()));
    out.push_str(&format!("![QR Code]({})\n\n", qr_url));
    out.push_str(&format!("🔗 [Download QR]({})\n\n", qr_url));
    out.push_str(&format!("{}\n\n`{}` · #qr", tg_footer("qrserver.com", "qr"), now));
    out
}

async fn fetch_dogs() -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let facts: Vec<serde_json::Value> = HTTP.get("https://dog-api.kinduff.com/api/facts?number=1").header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    let fact = facts.first().and_then(|f| f["fact"].as_str()).unwrap_or("Dogs can understand up to 250 words and gestures.");
    let img_url = "https://dog.ceo/api/breeds/image/random";
    let img_v: serde_json::Value = HTTP.get(img_url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(5)).send().await?.json().await?;
    let img = img_v["message"].as_str().unwrap_or("");
    let breed = img_v["message"].as_str().unwrap_or("").split('/').nth(4).unwrap_or("unknown");
    Ok(Md::new()
        .h2("🐕 Dog Fact")
        .blank()
        .pi("Source", "dog-api.kinduff.com")
        .pi("Breed Hint", breed)
        .blank()
        .push("## 🐕 Did You Know?")
        .blank()
        .quote(fact)
        .blank()
        .push("## 📊 Stats")
        .blank()
        .table(&["Stat", "Value"], &[
            vec!["Topic".into(), "Dogs".into()],
            vec!["Category".into(), "Animals".into()],
            vec!["Fun Level".into(), "🐾🐾🐾".into()],
        ])
        .blank()
        .push("## 🐾 More Dog Facts")
        .blank()
        .push("- Dogs have 18 muscles to move their ears")
        .push("- A Greyhound can run up to 45 mph")
        .push("- Dogs dream just like humans do")
        .blank()
        .push(&format!("{}\n\n`{}` · #dogs #fun", tg_footer("dog-api.kinduff.com", "dogs"), now))
        .build())
}

async fn fetch_cats() -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let v: serde_json::Value = HTTP.get("https://catfact.ninja/fact").header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    let fact = v["fact"].as_str().unwrap_or("Cats sleep for 12-16 hours per day.");
    let len = v["length"].as_u64().unwrap_or(0);
    let img_url = "https://api.thecatapi.com/v1/images/search";
    let img_v: serde_json::Value = HTTP.get(img_url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(5)).send().await?.json().await?;
    let img = img_v.as_array().and_then(|a| a.first()).and_then(|o| o["url"].as_str()).unwrap_or("");
    let mut out = format!("{}\n\n", tg_header("🐱", "Cat Fact", "random"));
    out.push_str(&format!("## 🐱 Did You Know?\n\n> {}\n\n", fact));
    out.push_str(&format!("| Stat | Value |\n|---|---|\n| Characters | `{}` |\n\n", len));
    if !img.is_empty() { out.push_str(&format!("![cat]({})\n\n", img)); }
    out.push_str(&format!("{}\n\n`{}` · #cats #fun", tg_footer("catfact.ninja", "cats"), now));
    Ok(out)
}

async fn fetch_useless() -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let facts = [
        "A group of flamingos is called a 'flamboyance'.",
        "Octopuses have three hearts.",
        "Honey never spoils. Archaeologists found 3000-year-old honey in Egyptian tombs that was still edible.",
        "Bananas are berries, but strawberries aren't.",
        "A jiffy is an actual unit of time: 1/100th of a second.",
        "The inventor of the Pringles can is buried in one.",
        "Wombat poop is cube-shaped.",
        "Cows have best friends and get stressed when separated.",
        "The unicorn is Scotland's national animal.",
        "Hot water freezes faster than cold water (Mpemba effect).",
        "A day on Venus is longer than a year on Venus.",
        "Humans share 60% of their DNA with bananas.",
        "The shortest war in history lasted 38 minutes (Anglo-Zanzibar War).",
        "There are more possible chess games than atoms in the observable universe.",
    ];
    let idx = (chrono::Utc::now().timestamp() as usize) % facts.len();
    let mut out = format!("{}\n\n", tg_header("🤓", "Useless Fact", "random"));
    out.push_str(&format!("## 🤓 Did You Know?\n\n> {}\n\n", facts[idx]));
    out.push_str("_Completely useless but absolutely true!_\n\n");
    out.push_str(&format!("{}\n\n`{}` · #useless #fun", tg_footer("memogram", "useless"), now));
    Ok(out)
}

async fn fetch_number(num: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let url = if num.trim().is_empty() { "http://numbersapi.com/random/trivia?json".to_string() } else { format!("http://numbersapi.com/{}/trivia?json", num.trim()) };
    let v: serde_json::Value = HTTP.get(&url).header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    let text = v["text"].as_str().unwrap_or("42 is the answer to life, the universe, and everything.");
    let number = v["number"].as_i64().unwrap_or(0);
    let found = v["found"].as_bool().unwrap_or(true);
    let typ = v["type"].as_str().unwrap_or("trivia");
    Ok(Md::new()
        .h2(&format!("🔢 Number Trivia"))
        .blank()
        .pi("Number", &number.to_string())
        .pi("Type", typ)
        .pi("Found", if found { "Yes" } else { "No" })
        .blank()
        .push("## 🔢 Trivia")
        .blank()
        .quote(text)
        .blank()
        .push("## 📊 Number Facts")
        .blank()
        .table(&["Fact", "Value"], &[
            vec!["Number".into(), number.to_string()],
            vec!["Type".into(), typ.to_string()],
            vec!["Status".into(), if found { "Found".into() } else { "Not found".into() }],
            vec!["Source".into(), "numbersapi.com".into()],
        ])
        .blank()
        .push("## 🔗 Try More")
        .blank()
        .push(&format!("- `/number {}` — Get another fact", number))
        .push(&format!("- `/number {}` — Try a different number", number + 1))
        .blank()
        .push(&format!("{}\n\n`{}` · #number #fun", tg_footer("numbersapi.com", "number"), now))
        .build())
}

async fn fetch_activities() -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let v: serde_json::Value = HTTP.get("https://www.boredapi.com/api/activity").header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    let activity = v["activity"].as_str().unwrap_or("Take a walk");
    let typ = v["type"].as_str().unwrap_or("recreational");
    let participants = v["participants"].as_u64().unwrap_or(1);
    let price = v["price"].as_f64().unwrap_or(0.0);
    let accessibility = v["accessibility"].as_f64().unwrap_or(0.5);
    let price_str = if price == 0.0 { "Free!".to_string() } else { format!("${:.1}", price) };
    let emoji = match typ {
        "education" => "📚", "recreational" => "🎮", "social" => "👥",
        "diy" => "🔧", "charity" => "💝", "cooking" => "🍳",
        "relaxation" => "🧘", "music" => "🎵", "busywork" => "💼", _ => "🎯",
    };
    Ok(Md::new()
        .h2("🎯 Activity")
        .blank()
        .pi("Type", typ)
        .pi("Price", &price_str)
        .pi("Accessibility", &format!("{:.0}%", accessibility * 100.0))
        .blank()
        .push("## 🎯 Try This")
        .blank()
        .quote(activity)
        .blank()
        .push("## 📊 Activity Details")
        .blank()
        .table(&["Stat", "Value"], &[
            vec!["Emoji".into(), emoji.to_string()],
            vec!["Type".into(), typ.to_string()],
            vec!["Participants".into(), participants.to_string()],
            vec!["Price".into(), price_str.clone()],
            vec!["Accessibility".into(), format!("{:.0}%", accessibility * 100.0)],
            vec!["Difficulty".into(), if accessibility < 0.3 { "Easy".into() } else if accessibility < 0.7 { "Medium".into() } else { "Hard".into() }],
        ])
        .blank()
        .push("## 💡 Why Try This?")
        .blank()
        .push(&match typ {
            "education" => "- Learn something new today!",
            "recreational" => "- Perfect for downtime and fun!",
            "social" => "- Great way to connect with others!",
            "diy" => "- Get hands-on and create something!",
            "cooking" => "- Delicious results guaranteed!",
            "relaxation" => "- Take a breather, you deserve it!",
            "music" => "- Let the rhythm move you!",
            _ => "- Something different, something fun!",
        })
        .blank()
        .push(&format!("{}\n\n`{}` · #activities #fun", tg_footer("boredapi.com", "activities"), now))
        .build())
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

async fn fetch_hello(_lang: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let v: serde_json::Value = HTTP.get("https://restcountries.com/v3.1/all?fields=name,languages").header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    let mut greetings: Vec<(String, String)> = Vec::new();
    if let Some(countries) = v.as_array() {
        for c in countries.iter().take(50) {
            if let Some(langs) = c["languages"].as_object() {
                for (lang_name, _) in langs.iter().take(1) {
                    let country = c["name"]["common"].as_str().unwrap_or("?");
                    greetings.push((lang_name.clone(), country.to_string()));
                }
            }
        }
    }
    greetings.sort_by(|a, b| a.0.cmp(&b.0));
    greetings.dedup_by(|a, b| a.0 == b.0);
    let mut out = format!("{}\n\n", tg_header("🌍", "Hello in Languages", &format!("{} languages", greetings.len())));
    out.push_str("| Language | Country |\n|---|---|\n");
    for (lang, country) in greetings.iter().take(30) {
        out.push_str(&format!("| {} | {} |\n", lang, country));
    }
    out.push_str(&format!("\n{}\n\n`{}` · #hello", tg_footer("restcountries.com", "hello"), now));
    Ok(out)
}

async fn fetch_kanye() -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let v: serde_json::Value = HTTP.get("https://api.kanye.rest").header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    let quote = v["quote"].as_str().unwrap_or("I am God's favorite.");
    let char_count = quote.len();
    let word_count = quote.split_whitespace().count();
    Ok(Md::new()
        .h2("🎤 Kanye West")
        .blank()
        .pi("Source", "kanye.rest")
        .pi("Words", &word_count.to_string())
        .pi("Chars", &char_count.to_string())
        .blank()
        .push("## 🎤 Kanye Says")
        .blank()
        .quote(quote)
        .blank()
        .push("— **Kanye West**")
        .blank()
        .push("## 📊 Quote Stats")
        .blank()
        .table(&["Metric", "Value"], &[
            vec!["Words".into(), word_count.to_string()],
            vec!["Characters".into(), char_count.to_string()],
            vec!["Vibes".into(), "Immaculate".into()],
        ])
        .blank()
        .push("## 🎵 Fun Facts")
        .blank()
        .push("- Kanye has won 24 Grammy Awards")
        .push("- He produced for Jay-Z before going solo")
        .push("- His first album was 'The College Dropout'")
        .blank()
        .push(&format!("{}\n\n`{}` · #kanye #fun", tg_footer("kanye.rest", "kanye"), now))
        .build())
}

async fn fetch_wouldyourather() -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let v: serde_json::Value = HTTP.get("https://api.truthordareapi.xyz/wyr?filter=pg").header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    if let Some(question) = v["question"].as_str() {
        let mut out = format!("{}\n\n", tg_header("🤔", "Would You Rather", "wyr"));
        out.push_str(&format!("## 🤔 Would You Rather?\n\n> {}\n\n", question));
        out.push_str("_Pick one! No wrong answers._\n\n");
        out.push_str(&format!("{}\n\n`{}` · #wyr #fun", tg_footer("truthordareapi.xyz", "wyr"), now));
        return Ok(out);
    }
    // Fallback
    let questions = [
        "Be able to fly or be invisible?",
        "Live without music or live without movies?",
        "Have unlimited money or unlimited time?",
        "Be famous or be incredibly wealthy?",
        "Know how you die or when you die?",
        "Give up your phone or give up AC/heating?",
        "Be the funniest or smartest person in the room?",
        "Travel the world or have a dream house?",
    ];
    let idx = (chrono::Utc::now().timestamp() as usize) % questions.len();
    let mut out = format!("{}\n\n", tg_header("🤔", "Would You Rather", "wyr"));
    out.push_str(&format!("## 🤔 Would You Rather?\n\n> {}\n\n", questions[idx]));
    out.push_str("_Pick one! No wrong answers._\n\n");
    out.push_str(&format!("{}\n\n`{}` · #wyr #fun", tg_footer("truthordareapi.xyz", "wyr"), now));
    Ok(out)
}

async fn fetch_catfact() -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let v: serde_json::Value = HTTP.get("https://catfact.ninja/fact").header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    let fact = v["fact"].as_str().unwrap_or("Cats have over 20 vocalizations, including the purr.");
    let len = v["length"].as_u64().unwrap_or(0);
    let mut out = format!("{}\n\n", tg_header("🐱", "Cat Fact", "random"));
    out.push_str(&format!("## 🐱 Cat Fact\n\n> {}\n\n", fact));
    out.push_str(&format!("| Stat | Value |\n|---|---|\n| Characters | `{}` |\n\n", len));
    out.push_str(&format!("{}\n\n`{}` · #catfact #fun", tg_footer("catfact.ninja", "catfact"), now));
    Ok(out)
}

async fn fetch_affirmation2(mood: &str) -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let affirmations = [
        "You are capable of amazing things.",
        "Your potential is limitless.",
        "Today is a fresh start.",
        "You are worthy of love and respect.",
        "Your hard work is paying off.",
        "You bring light to those around you.",
        "You are stronger than you think.",
        "Every step forward is progress.",
        "You deserve happiness and peace.",
        "Your creativity knows no bounds.",
        "You are making a difference.",
        "Trust the journey, even when it's hard.",
        "You are enough, exactly as you are.",
        "Your kindness changes the world.",
        "You have the power to create change.",
    ];
    let idx = (chrono::Utc::now().timestamp() as usize) % affirmations.len();
    let aff = affirmations[idx];
    let mood_str = if mood.trim().is_empty() { "general".to_string() } else { mood.trim().to_string() };
    let mut out = format!("{}\n\n", tg_header("✨", "Affirmation", &mood_str));
    out.push_str(&format!("## ✨ Daily Affirmation\n\n> _\"{}\"_\n\n", aff));
    out.push_str(&format!("**Mood:** `{}`\n\n", mood_str));
    out.push_str("Repeat this to yourself 3 times. You deserve it.\n\n");
    out.push_str(&format!("{}\n\n`{}` · #affirmation2 #wellness", tg_footer("memogram", "affirmation2"), now));
    Ok(out)
}

async fn fetch_advice() -> Result<String> {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    let v: serde_json::Value = HTTP.get("https://api.adviceslip.com/advice").header("User-Agent", "memogram-rs").timeout(std::time::Duration::from_secs(8)).send().await?.json().await?;
    if let Some(slip) = v["slip"].as_object() {
        let advice = slip["advice"].as_str().unwrap_or("Give people more than they expect and do it cheerfully.");
        let id = slip.get("id").map(|v| v.to_string()).unwrap_or_else(|| "?".to_string());
        let mut out = format!("{}\n\n", tg_header("💡", "Advice", &format!("#{}", id)));
        out.push_str(&format!("## 💡 Advice Slip\n\n> {}\n\n", advice));
        out.push_str(&format!("| Stat | Value |\n|---|---|\n| Slip ID | `{}` |\n\n", id));
        out.push_str(&format!("{}\n\n`{}` · #advice #fun", tg_footer("adviceslip.com", "advice"), now));
        return Ok(out);
    }
    let mut out = format!("{}\n\n", tg_header("💡", "Advice", "general"));
    out.push_str(&format!("## 💡 Advice\n\n> The best time to plant a tree was 20 years ago. The second best time is now.\n\n"));
    out.push_str(&format!("{}\n\n`{}` · #advice #fun", tg_footer("adviceslip.com", "advice"), now));
    Ok(out)
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
        ("mdn", try_fetch("mdn", fetch_mdn("fetch")).await.1),
        ("docker", try_fetch("docker", fetch_docker("nginx")).await.1),
        ("rfc", try_fetch("rfc", fetch_rfc("7231")).await.1),
        ("man", try_fetch("man", fetch_man("git")).await.1),
        ("airquality", try_fetch("airquality", fetch_airquality("Beijing")).await.1),
        ("sunrise", try_fetch("sunrise", fetch_sunrise("34.1706,-118.8376")).await.1),
        ("etymology", try_fetch("etymology", fetch_etymology("hello")).await.1),
        ("synonym", try_fetch("synonym", fetch_synonym("happy")).await.1),
        ("philosophy", try_fetch("philosophy", fetch_philosophy_quote()).await.1),
        ("finance", try_fetch("finance", fetch_finance("inflation")).await.1),
        ("trial", try_fetch("trial", fetch_trial("diabetes")).await.1),
        ("food", try_fetch("food", fetch_food("apple")).await.1),
        ("pubmed", try_fetch("pubmed", fetch_pubmed("CRISPR")).await.1),
        ("drug", try_fetch("drug", fetch_drug("aspirin")).await.1),
        ("bbc", try_fetch("bbc", fetch_bbc()).await.1),
        ("reuters", try_fetch("reuters", fetch_reuters()).await.1),
        ("ap", try_fetch("ap", fetch_ap()).await.1),
        ("arxiv", try_fetch("arxiv", fetch_arxiv("quantum")).await.1),
        ("devto", try_fetch("devto", fetch_devto()).await.1),
        ("tldr", try_fetch("tldr", fetch_tldr()).await.1),
        ("lobsters", try_fetch("lobsters", fetch_lobsters()).await.1),
        ("guardian", try_fetch("guardian", fetch_guardian("technology")).await.1),
        ("reddit", try_fetch("reddit", fetch_reddit("selfhosted")).await.1),
        ("markets", try_fetch("markets", fetch_markets()).await.1),
        ("itunes", try_fetch("itunes", fetch_itunes("drake")).await.1),
        ("deezer", try_fetch("deezer", fetch_deezer("drake")).await.1),
        ("mbrainz", try_fetch("mbrainz", fetch_mbrainz("beatles")).await.1),
        ("lyrics", try_fetch("lyrics", fetch_lyrics("coldplay - adventure of a lifetime")).await.1),
        ("bpm", try_fetch("bpm", fetch_bpm("120")).await.1),
        ("trend", try_fetch("trend", fetch_trend()).await.1),
        ("joke", try_fetch("joke", fetch_joke("")).await.1),
        ("trivia", try_fetch("trivia", fetch_trivia("")).await.1),
        ("story", try_fetch("story", fetch_story("")).await.1),
        ("fortune", try_fetch("fortune", fetch_fortune("")).await.1),
        ("bored", try_fetch("bored", fetch_bored("")).await.1),
        ("wordoftheday", try_fetch("wordoftheday", fetch_wordoftheday()).await.1),
        ("quote", try_fetch("quote", fetch_quote("")).await.1),
        ("truth", try_fetch("truth", fetch_truth()).await.1),
        ("fact", try_fetch("fact", fetch_fact()).await.1),
        ("dadjoke", try_fetch("dadjoke", fetch_dadjoke()).await.1),
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
        ("setlist", create_setlist("Intro, Dark Trap, Chill Loop, Drill, Outro")),
        ("sample", create_sample("vintage soul chop + vinyl crackle")),
        ("cover", create_cover("Bohemian Rhapsody")),
        ("flag", create_flag("Follow up on beat collab")),
        ("archive", create_archive("Old meeting notes")),
        ("move", create_move("wellness Move to wellness bucket")),
        ("wind", try_fetch("wind", fetch_wind("Thousand Oaks, CA")).await.1),
        ("uv", try_fetch("uv", fetch_uv("Thousand Oaks, CA")).await.1),
        ("pollen", try_fetch("pollen", fetch_pollen("Thousand Oaks, CA")).await.1),
        ("moon", try_fetch("moon", fetch_moon("")).await.1),
        ("tide", try_fetch("tide", fetch_tide("Santa Monica, CA")).await.1),
        ("snow", try_fetch("snow", fetch_snow("Mammoth Lakes, CA")).await.1),
        ("ticker", try_fetch("ticker", fetch_ticker("AAPL")).await.1),
        ("dividend", try_fetch("dividend", fetch_dividend("AAPL")).await.1),
        ("etf", try_fetch("etf", fetch_etf("SPY")).await.1),
        ("earnings", try_fetch("earnings", fetch_earnings("this week")).await.1),
        ("recap", create_recap("7").await),
        // NEW: missing template commands
        ("color", fetch_color("#FF5733")),
        ("math", eval_math("2+2*3")),
        ("meeting", create_meeting("Sprint planning 10am discuss Q3 goals and blockers")),
        ("project", create_project("Memogram v2 — Telegram bot for Memos")),
        ("book", create_book("Dune by Frank Herbert — sci-fi masterpiece about spice and power")),
        ("todo", create_todo("Fix weather API, deploy v2, write tests")),
        ("list", create_list("Groceries: milk, eggs, bread, coffee")),
        ("clip", create_clip("https://example.com article about rust async")),
        ("mood", create_mood_entry("7 productive day, shipped features")),
        ("gratitude", create_gratitude_entry("Good coffee, clean code, team support")),
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
        ("sunset", try_fetch("sunset", fetch_sunrise("34.1706,-118.8376")).await.1),
        ("containers", try_fetch("containers", fetch_containers("http://localhost:6100")).await.1),
        // REPLACED COMMANDS
        ("ph", fetch_ph("0.001")),
        ("yt", try_fetch("yt", fetch_yt("https://www.youtube.com/watch?v=dQw4w9WgXcQ")).await.1),
        ("ghrepo", try_fetch("ghrepo", fetch_ghrepo("rust-lang/rust")).await.1),
        ("stocksave", try_fetch("stocksave", fetch_stocksave("AAPL")).await.1),
        ("weather7", try_fetch("weather7", fetch_weather7("London")).await.1),
        ("img", try_fetch("img", fetch_img("https://httpbin.org/image/png")).await.1),
        ("ip", try_fetch("ip", fetch_ip("8.8.8.8")).await.1),
        ("chuck", try_fetch("chuck", fetch_chuck()).await.1),
        ("insult", try_fetch("insult", fetch_insult("programmer")).await.1),
        ("zen", try_fetch("zen", fetch_zen()).await.1),
        ("astro", try_fetch("astro", fetch_astro("aries")).await.1),
        ("summarize", try_fetch("summarize", fetch_summarize("https://example.com")).await.1),
        ("qr", fetch_qr("https://memogram.junilab.xyz")),
        ("dogs", try_fetch("dogs", fetch_dogs()).await.1),
        ("cats", try_fetch("cats", fetch_cats()).await.1),
        ("useless", try_fetch("useless", fetch_useless()).await.1),
        ("number", try_fetch("number", fetch_number("42")).await.1),
        ("activities", try_fetch("activities", fetch_activities()).await.1),
        ("bmi", fetch_bmi("175 70")),
        ("hello", try_fetch("hello", fetch_hello("all")).await.1),
        ("kanye", try_fetch("kanye", fetch_kanye()).await.1),
        ("wouldyourather", try_fetch("wouldyourather", fetch_wouldyourather()).await.1),
        ("catfact", try_fetch("catfact", fetch_catfact()).await.1),
        ("affirmation2", try_fetch("affirmation2", fetch_affirmation2("happy")).await.1),
        ("advice", try_fetch("advice", fetch_advice()).await.1),
    ];
    for (name, content) in templates {
        let path = out_dir.join(format!("{}.md", name));
        let _ = std::fs::write(&path, &content);
        println!("wrote {} (template)", name);
    }

    println!("=== DONE — check {}/ ===", out_dir.display());
    Ok(())
}
