# memogram-rs

Low-footprint Rust rewrite of [memogram](https://github.com/usememos/memogram) — Telegram → Memos bridge, single binary + Docker. **77 polished markdown commands, 11 buckets, ~10MB RSS.** — detailed document memos with tables/graphs, not text-message snippets.

## Cheatsheet

```
/start <pat>        Link account           /search <q>       Search memos
/help               Show commands          /recent           Last 20 memos
/inbox              Untagged memos         /tags             List all tags
/count <tag>        Count by tag           /undo             Delete last
/pin                Pin/unpin last         /note <text>      Quick note
```

### Research
```
/define <word>       Dictionary             /wiki <q>         Wikipedia
/cheat <q>           Cheat sheet            /translate <text>  Translate
/etymology <word>    Word origin            /synonym <word>    Synonyms
/finance <term>      Finance explainer      /philosophy        Random quote
```

### News
```
/hn                  HackerNews top 5       /arxiv <q>        arXiv papers
/devto               dev.to top             /ph               Product Hunt
```

### Finance (learn-focused)
```
/fx <pair>           Exchange rate          /stock <ticker>   Stock price
/crypto <coin>       Crypto price           /markets          Market indices
/finance <term>      Explain term           /compound <p> <r> <y>  Interest calc
```

### Science & Bio
```
/pubmed <q>          PubMed papers          /drug <name>      Drug info
/genome <gene>       Gene search            /protein <id>     Protein info
/trial <q>           Clinical trials        /food <query>     Nutrition facts
/mood <note>         Log mood               /habit <task>     Track habit
```

### Dev
```
/gh <q>              GitHub search          /npm <pkg>        NPM info
/pypi <pkg>          PyPI info              /crates <pkg>     crates.io info
/containers          Docker health          /stackoverflow <q> SO search
```

### Weather
```
/weather <city>      Current + 3-day        /forecast <city>  7-day forecast
/sunrise <loc>       Sunrise/sunset         /sunset <loc>     Sunset/sunrise
/airquality <loc>    AQI                  
```

### Utilities
```
/math <expr>         Evaluate math          /color <hex>      Color preview
/ip <addr>           IP lookup              /qr <text>        QR code
/hash <text>         SHA-256                /base64 <text>    Encode/decode
/json <text>         Pretty JSON            /uuid             Generate UUID
/pass <len>          Password               /remind <m> <msg> Reminder
```

### Quick Capture
```
/meeting <text>      Meeting notes          /project <text>   Project doc
/recipe <text>       Recipe card            /book <text>      Book note
/todo <text>         Checklist              /list <text>      Bulleted list
/clip <text>         Bookmark               /proscons <t>     Pros vs cons
/flashcard <q> | <a> Flashcard              /remind <m> <msg> Reminder
```

### Stoic
```
/meditation <n>      Log meditation         /affirmation <n>  Affirmation
/reflection <n>      Reflection             /wisdom           Stoic quote
/journal <note>      Journal entry
```

### Planning
```
/goal <goal>         Set goal               /deadline <d> <t> Track deadline
/plan <text>         Daily plan             /review <text>    Weekly review
/priority <l> <t>    Set priority
```

### Inbox
```
/idea <text>         Capture idea           /braindump <t>    Thought dump
/link <url> <desc>   Save link              /snippet <code>   Code snippet
/save <text>         Save anything
```

### Daily
```
/morning <text>      Morning check-in       /evening <text>   Evening reflection
/checkin <text>      Daily check-in         /log <text>       Daily log
/summary <text>      Day summary
```

### Life
```
/sleep <hrs> <q>     Log sleep              /energy <1-10>    Log energy
/exercise <a> <d>    Log exercise           /water <amt>      Log water
/read <title> <a>    Log reading
```

## 11 Buckets

| Bucket | Purpose |
|--------|---------|
| `inbox` | Default capture |
| `news` | HN, arXiv, Dev.to, PH |
| `dev` | GitHub, npm, PyPI, crates |
| `learn` | Wiki, definitions, research |
| `bio` | PubMed, drugs, genomes, trials, nutrition |
| `money` | FX, stocks, crypto, finance explainers, compound |
| `life` | Mood, gratitude, habits |
| `planning` | Goals, deadlines, reviews |
| `daily` | Check-ins, logs, summaries |
| `stoic` | Philosophy, meditation |
| `weather` | Forecasts, air quality |

## Quick start

```bash
cp .env.example .env
cargo run --release
```

## Env

`MEMOS_URL` `BOT_TOKEN` `BOT_TOKENS_JSON` `ALLOWED_USERNAMES` `DATA`

## Tech

teloxide · reqwest (rustls) · tokio · quick-xml

## License

MIT
