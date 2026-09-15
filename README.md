# memogram-rs

Low-footprint Rust rewrite of [memogram](https://github.com/usememos/memogram) — Telegram → Memos bridge, single binary + Docker. **45 polished markdown commands, 11 buckets, ~10MB RSS.**

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

### News & Research
```
/hn                  HackerNews top 5       /arxiv <q>        arXiv papers
/define <word>       Dictionary             /wiki <q>         Wikipedia
/brief <topic>       Research brief         /compare <a> vs <b> Comparison
/paper <query>       Paper deep-dive        /tutorial <topic> Guided how-to
/translate <text>    Translate              /book <text>      Book note card
/youtube <url>       Summarize video
```

### Dev
```
/gh <q>              GitHub search/repo     /ip <addr>        IP lookup
/containers          Service health
```

### Finance & Money
```
/fx <pair>           Exchange rate          /stock <ticker>   Stock price
/crypto <coin>       Crypto price           /markets          Market indices
/finance <term>      Explain term           /compound <p> <r> <y>  Interest calc
/hustle <skill>      Side hustle ideas      /income <src> <amt> Track income
/portfolio           Track holdings         /alerts           Price alerts
```

### Bio & Health
```
/pubmed <q>          PubMed papers          /trial <q>        Clinical trials
/food <query>        Nutrition facts        /exercise <a> <d> Log exercise
/energy <1-10>       Log energy             /water <amt>      Log water
/read <title> <a>    Log reading
```

### Wellness
```
/mood <text>         Log mood               /habit <task>     Track habit
/mood gratitude ...  Log gratitude          /mood journal ... Journal entry
/mood reflection ... Log reflection         /stress <n>       Log stress
/meditation <n>      Log meditation
```

### Planning
```
/goal <goal>         Set a goal             /deadline <d> <t> Track deadline
/priority <l> <t>    Set priority
```

### Inbox
```
/idea <text>         Capture idea           /braindump <t>    Thought dump
/summarize <url>     Summarize URL          /transcribe <t>   Voice-to-text
```

### Daily
```
/daily               Daily template          /digest            Today's memo summary
/streak              Writing streak          /morning <text>    Morning check-in
/evening <text>      Evening reflection      /checkin <m> <e>   Quick check-in
/log <text>          Daily log               /summary <text>    Day summary
```

## Buckets

| Bucket | Purpose |
|--------|---------|
| `inbox` | Default capture, ideas, links, snippets |
| `news` | HN, arXiv |
| `dev` | GitHub, containers, IP |
| `learn` | Wiki, definitions, research, books, YouTube |
| `bio` | PubMed, trials, nutrition, exercise |
| `money` | FX, stocks, crypto, finance, hustle, income |
| `wellness` | Mood, gratitude, journal, reflection, habits, stress |
| `planning` | Goals, deadlines, priorities |
| `daily` | Check-ins, logs, summaries, digest, streak |
| `weather` | Forecasts, air quality |

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
