# memogram-rs

Low-footprint Rust rewrite of [memogram](https://github.com/usememos/memogram) — Telegram → Memos bridge, single binary + Docker.

Keeps memogram defaults (inbox-ping, Telegram file forwarding) and adds 40+ bot commands.

## Commands

```
/start, /search <q>, /tags, /recent, /count <filter>, /daily
/hn, /weather <city>, /define <word>, /wiki <q>, /cheat <q>, /gh <q>
/fx <pair>, /containers, /lobsters <q>, /stock <ticker>, /crypto <coin>
/translate <text>, /color <hex>, /forecast <city>, /pass, /uuid, /ip
/qr <text>, /hash <text>, /base64 <text>, /json <text>, /remind <text>
/portfolio <ticker>, /alerts <ticker>, /markets, /arxiv <q>, /devto, /ph
/inbox, /undo, /pin, /note <text>, /meeting <text>, /project <text>
/recipe <text>, /book <text>, /todo <text>, /list <text>, /clip <text>
/proscons <text>, /flashcard <text>
```

## Quick start

```bash
cp .env.example .env  # set MEMOS_URL, MEMOS_TOKEN, BOT_TOKEN, ALLOWED_USERS
cargo run --release
# or
docker build -t memogram-rs . && docker run --env-file .env memogram-rs
```

## Env

- `MEMOS_URL` — your Memos instance URL
- `MEMOS_TOKEN` — Memos access token
- `TELEGRAM_BOT_TOKEN` (or `BOT_TOKEN`) — Telegram bot token
- `ALLOWED_USERS` — comma-separated Telegram usernames (optional)
- `STORE_PATH` — local JSON store path for inbox state

Built with `teloxide`, `reqwest` (rustls), `tokio`.

## License

MIT
