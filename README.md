# memogram-rs

Low-footprint Rust rewrite of [memogram](https://github.com/usememos/memogram) — Telegram → Memos bridge, single binary + Docker.

Keeps memogram defaults (inbox-ping, Telegram file forwarding) and adds 73 bot commands across 11 topic buckets.

## Architecture

```
Telegram (@memomommy_bot)
    ↓
memogram-rs (Rust binary, ~10MB RSS)
    ↓
Memos API (11 topic buckets)
```

Each bucket is a separate Memos user with its own personal access token. Commands auto-tag memos with topic-specific hashtags.

## 11 Topic Buckets

| Bucket | Purpose | Tags |
|--------|---------|------|
| `inbox` | Default memo capture | `#inbox` |
| `news` | HN, arXiv, Dev.to, Product Hunt | `#hn` `#arxiv` `#devto` `#ph` |
| `dev` | GitHub, npm, PyPI, crates, containers | `#gh` `#npm` `#pypi` `#crates` `#ops` |
| `learn` | Wiki, definitions, cheat sheets, reading | `#wiki` `#define` `#cheat` `#read` `#deepresearch` |
| `bio` | Biology, medicine, PubMed, genomes | `#pubmed` `#drug` `#genome` `#protein` |
| `money` | FX rates, stocks, crypto | `#fx` `#stock` `#crypto` |
| `life` | Stoicism, mood, gratitude, habits | `#stoic` `#mood` `#gratitude` `#habit` |
| `planning` | Calendar, tasks, reminders | `#calendar` `#tasks` |
| `daily` | Weekly reviews, today's notes | `#week` `#today` `#reminders` |
| `stoic` | Philosophy quotes | `#quote` |
| `weather` | Forecasts, sunrise/sunset | `#weather` `#sunrise` |

## 73 Commands

### Core
`/start` `/help` `/inbox` `/recent` `/search <q>` `/tags` `/count <filter>` `/undo` `/pin`

### Research & Reference
`/define <word>` `/wiki <q>` `/cheat <q>` `/translate <text>` `/deepresearch <q>` `/etymology <word>` `/synonym <word>` `/philosophy <q>`

### News & Discovery
`/hn` `/arxiv <q>` `/devto` `/ph`

### Finance
`/fx <pair>` `/stock <ticker>` `/crypto <coin>` `/markets` `/portfolio <ticker>` `/alerts <ticker>`

### Science & Bio
`/pubmed <q>` `/drug <name>` `/genome <gene>` `/protein <id>` `/mood <text>` `/gratitude <text>` `/habit <text>`

### Dev & Infra
`/gh <q>` `/npm <pkg>` `/pypi <pkg>` `/crates <pkg>` `/containers` `/stackoverflow <q>`

### Utilities
`/weather <city>` `/forecast <city>` `/sunrise` `/airquality` `/math <expr>` `/color <hex>` `/ip` `/qr <text>` `/hash <text>` `/base64 <text>` `/json <text>` `/uuid` `/pass`

### Quick Capture
`/note <text>` `/meeting <text>` `/project <text>` `/recipe <text>` `/book <text>` `/todo <text>` `/list <text>` `/clip <text>` `/proscons <text>` `/flashcard <text>` `/remind <text>`

### Stoic
`/meditation <note>` `/affirmation <note>` `/reflection <note>` `/wisdom` `/journal <note>`

### Planning
`/goal <goal>` `/deadline <date> <task>` `/plan <text>` `/review <text>` `/priority <level> <task>`

### Inbox
`/idea <text>` `/braindump <text>` `/link <url> <desc>` `/snippet <code>` `/save <text>`

### Daily
`/morning <text>` `/evening <text>` `/checkin <text>` `/log <text>` `/summary <text>`

### Life
`/sleep <hours> <quality>` `/energy <level> <note>` `/exercise <activity> <duration>` `/water <amount> <note>` `/read <title> <author>`

## Quick start

```bash
cp .env.example .env  # set MEMOS_URL, BOT_TOKEN, BOT_TOKENS_JSON
cargo run --release
# or
docker build -t memogram-rs . && docker run --env-file .env memogram-rs
```

## Env

- `MEMOS_URL` — your Memos instance URL
- `BOT_TOKEN` — Telegram bot token
- `BOT_TOKENS_JSON` — JSON map of bucket → Memos PAT
- `ALLOWED_USERNAMES` — comma-separated Telegram usernames (optional)
- `DATA` — path to data directory for inbox state

## Tech Stack

- **Runtime:** tokio async
- **Telegram:** teloxide
- **HTTP:** reqwest (rustls)
- **XML parsing:** quick-xml (PubMed, arXiv)
- **Storage:** Memos API (no local DB)

Built with `teloxide`, `reqwest` (rustls), `tokio`.

## License

MIT
