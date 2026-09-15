# memogram-rs

Low-footprint Rust rewrite of [memogram](https://github.com/usememos/memogram) — Telegram → Memos bridge, single binary + Docker. **~70 commands, 11 buckets, ~16MB binary.**

Personal memo maker that outputs rich markdown documents for making money, learning, health, habits, and staying in your field.

## Cheatsheet

```
/start <pat>        Link account           /search <q>       Search memos
/help               Show commands          /recent           Last 20 memos
/inbox              Untagged memos         /tags             List all tags
/count <tag>        Count by tag           /undo             Delete last
/pin                Pin/unpin last         /save <text>      Save anything
/streak             Writing streak         /daily            Daily template
/digest             Today's memo summary
```

### News & Updates
```
/hn                  HackerNews top 5       /arxiv <q>        arXiv papers
/lobsters            Lobsters hot stories   /ph               Product Hunt
```

### Learn & Research
```
/define <word>       Dictionary             /wiki <q>         Wikipedia
/brief <topic>       Research brief         /compare <a> vs <b> Comparison
/paper <query>       Paper deep-dive        /tutorial <topic> Guided how-to
/translate <text>    Translate              /book <text>      Book note card
/youtube <url>       Summarize video
```

### Dev
```
/gh <q>              GitHub search/repo     /ip <addr>        IP lookup
/containers          Service health         /ports            Port reference
/dns <domain>        DNS lookup             /timestamp        Epoch ↔ time
```

### Weather
```
/weather <city>      Current + 3-day        /wind <city>      Wind forecast
/uv <loc>            UV index               /moon             Moon phase
```

### Finance & Money
```
/fx <pair>           Exchange rate          /stock <ticker>   Stock price
/crypto <coin>       Crypto price           /markets          Market indices
/finance <term>      Explain term           /compound <p> <r> <y>  Interest calc
/hustle <skill>      Side hustle ideas      /income <src> <amt> Track income
/portfolio           Track holdings         /alerts           Price alerts
```

### Bioengineering & Pre-Health
```
/pubmed <q>          PubMed papers          /trial <q>        Clinical trials
/patent <query>      Patent search          /species <name>   Taxonomy lookup
/lab <protocol>      Lab protocol template  /food <query>     Nutrition facts
/prereqs <track>     Health prof prereqs    /mcat <topic>     MCAT study guide
/clinical <a> <h>    Log clinical hours     /shadow <dr> <h>  Log shadowing
/ethics <scenario>   Medical ethics case
```

### Health & Habits
```
/mood <text>         Log mood               /habit <task>     Track habit
/mood gratitude ...  Log gratitude          /mood journal ... Journal entry
/mood reflection ... Log reflection         /stress <n>       Log stress
/meditation <n>      Log meditation         /sleep <hrs> <q>  Log sleep
/energy <1-10>       Log energy             /exercise <a> <d> Log exercise
/water <amt>         Log water              /read <title> <a> Log reading
```

### Planning & Goals
```
/goal <goal>         Set a goal (SMART)     /deadline <d> <t> Track deadline
/priority <l> <t>    Set priority           /project <text>   Project doc
/todo <text>         Checklist              /weekly           Weekly review
/retro <sprint>      Sprint retrospective
```

### Daily
```
/morning <text>      Morning check-in       /evening <text>   Evening reflection
/checkin <m> <e>     Quick check-in         /log <text>       Daily log
/summary <text>      Day summary
```

### Inbox
```
/idea <text>         Capture idea           /braindump <t>    Thought dump
/summarize <url>     Summarize URL          /transcribe <t>   Voice-to-text
/list <text>         Bulleted list
```

## Buckets

| Bucket | Commands | Purpose |
|--------|----------|---------|
| `bio` | 10 | PubMed, trials, patents, species, lab, prereqs, MCAT, clinical, shadowing, ethics |
| `learn` | 9 | Wiki, definitions, research, books, YouTube, papers |
| `daily` | 11 | Check-ins, logs, summaries, digest, streak |
| `money` | 8 | FX, stocks, crypto, finance, hustle, income |
| `planning` | 7 | Goals, deadlines, priorities, projects, todos, weekly, retro |
| `inbox` | 6 | Ideas, links, snippets, save, transcribe, list |
| `wellness` | 5 | Mood, habits, stress, meditation, gratitude |
| `weather` | 4 | Forecast, wind, UV, moon |
| `news` | 4 | HN, arXiv, Lobsters, Product Hunt |
| `dev` | 6 | GitHub, containers, IP, ports, DNS, timestamp |

## Setup

1. Create a [Memos](https://usememos.com) PAT with `meal:memo` scope
2. Create 10 more PATs for the 10 non-inbox buckets (optional — per-bot routing)
3. Get a Telegram bot token from @BotFather
4. Run:
```bash
docker run -e BOT_TOKEN=... -e MEMOS_URL=... -e BARK_URL=... -e NTFY_URL=... ghcr.io/0124212/memogram-rs:latest
```

## Env

```
BOT_TOKEN            Telegram bot token (required)
MEMOS_URL            Memos instance URL (required)
ADMIN_USERNAME       Your Memos username (default: admin)
ALLOWED_USERNAMES    Comma-separated allowed Telegram users
DATA                 Path to token store (default: ./data.txt)
BOT_TOKENS_JSON      JSON map of bucket → Memos PAT
BARK_URL             Bark push notification URL (optional)
NTFY_URL             ntfy push notification URL (optional)
```
