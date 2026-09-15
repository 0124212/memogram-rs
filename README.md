# memogram-rs

Low-footprint Rust rewrite of [memogram](https://github.com/usememos/memogram) — Telegram → Memos bridge, single binary + Docker. **79 commands (70 routed across 10 buckets × 7 + 9 core), 16MB binary.**

Every command pulls real data from a live API or returns evidence-based structured content. Every memo tagged `#memogram-rs`. Two-way sync with Vikunja for tasks.

## Core (9)
```
/start <pat>    Link Telegram → Memos    /search <q>    Full-text search memos
/help           List all commands        /undo          Delete last memo
/pin            Pin/unpin last memo      /remind <m> <msg>  Bark + ntfy push timer
/inbox          Untagged memos           /portfolio     Track holdings
/alerts         Price alerts
```

## bio (7)
| Command | Source | Data |
|---------|--------|------|
| `/pubmed <q>` | PubMed (35M papers) | Paper titles, authors, dates |
| `/trial <q>` | ClinicalTrials.gov (400K+) | Trial titles, status, phases |
| `/molecule <name>` | PubChem (110M compounds) | Formula, weight, SMILES, Lipinski, safety |
| `/pathway <query>` | NCBI + Reactome + UniProt | Gene info, pathways, GO terms, protein |
| `/amino <code>` | Biochemistry reference | 20 amino acids: properties, codons, role |
| `/genome <gene>` | NCBI Gene (40M) | Gene info, type, chromosome, summary |
| `/protein <id>` | UniProt (250M entries) | Protein data |

## dev (7)
| Command | Source | Data |
|---------|--------|------|
| `/gh <q>` | GitHub (200M repos) | Repo info, languages, issues |
| `/http <url>` | Any URL | Status, timing, headers, security audit, body preview |
| `/man <cmd>` | cheat.sh + tldr (150+ tools) | Quick reference + detailed examples |
| `/css <prop>` | MDN Web Docs | CSS property docs |
| `/html <elem>` | MDN Web Docs | HTML element docs |
| `/astro <topic>` | Astro docs | Framework reference |
| `/grep <pat>` | cheat.sh | ripgrep/awk/sed patterns |

## news (7)
| Command | Source | Data |
|---------|--------|------|
| `/hn` | HackerNews API | Top 5 stories: title, score, comments |
| `/arxiv <q>` | arXiv (2M+ papers) | Paper titles, authors, abstracts |
| `/lobsters` | Lobsters (100K+ stories) | Top 15: title, score, comments, tags |
| `/ph` | Product Hunt GraphQL | Today's products: votes, tags |
| `/scholar <q>` | Google Scholar (200M papers) | Titles, snippets, authors |
| `/reddit <sub>` | Reddit JSON (100M+ posts) | Top 15: score, comments, flair |
| `/news <topic>` | HN Algolia (200K+ stories) | Search results with dates |

## learn (7)
| Command | Source | Data |
|---------|--------|------|
| `/define <word>` | Wiktionary (700K words) | Definition, pronunciation, etymology |
| `/wiki <q>` | Wikipedia (6M articles) | Summary, image, URL |
| `/translate <text>` | LibreTranslate (100+ languages) | Translation + back-verification |
| `/book <text>` | Open Library (40M books) | Cover, author, pages, subjects |
| `/paper <query>` | arXiv API | Title, authors, abstract, PDF |
| `/youtube <url>` | Invidious API (800M videos) | Title, channel, length, description |
| `/learn <topic>` | Wikipedia + arXiv + YouTube | Overview + related topics + papers + videos + learning roadmap + progress tracker |

## wellness (7)
| Command | Source | Data |
|---------|--------|------|
| `/food <query>` | OpenFoodFacts (1M+ foods) | Macros, Nutri-Score, ingredients, allergens |
| `/workout <muscle>` | ExerciseDB (11K exercises) | Steps, form tips, muscles, equipment |
| `/recipe <cuisine>` | TheMealDB (300+ recipes) | Ingredients table + instructions + video |
| `/therapy <situation>` | CBT + ZenQuotes | CBT thought record + stoic grounding + evidence-based steps |
| `/posture <issue>` | Physiotherapy research | 8 issues: evidence-cited exercises, clinical sources |
| `/calories <act> <min>` | API Ninjas (3K activities) | Calories, MET, zones, weekly projection |
| `/stretch <muscle>` | ExerciseDB (11K exercises) | Stretch routines, hold times, tips |

## money (7)
| Command | Source | Data |
|---------|--------|------|
| `/fx <pair>` | Live exchange rates | Real-time conversions |
| `/stock <ticker>` | Yahoo Finance | Live price, 5-day history, volume |
| `/crypto <coin>` | CoinGecko | Price, market cap, ATH/ATL |
| `/markets` | Yahoo Finance | S&P, NASDAQ, DOW, BTC, ETH |
| `/invest <amt> <yrs>` | S&P 500 historical averages | Year-by-year projection, Rule of 72, inflation-adjusted |
| `/hustle <skill>` | BLS + Upwork/Toptal data | Occupation growth + freelance rates + platforms |
| `/salary <title>` | BLS OEWS (800+ occupations) | Median, top 10%, bottom 25%, growth, education |

## tasks (7) — Vikunja-powered
| Command | Source | Data |
|---------|--------|------|
| `/todo <text>` | Vikunja API | Creates task + memo with link |
| `/deadline <date> <task>` | Vikunja API | Task with due date + days remaining |
| `/goal <goal>` | Vikunja API | Project + 3 starter tasks + link |
| `/project <name>` | Vikunja API | Creates Vikunja project |
| `/weekly` | Vikunja API | Completed/open tasks across all projects |
| `/overdue` | Vikunja API | Overdue tasks with days late + urgency |
| `/standup` | Vikunja API | Today's completed + in-progress + blockers |

## memos (7)
| Command | Source | Data |
|---------|--------|------|
| `/memos` | wttr.in + Memos API | Weather + memo count + auto-creates entry |
| `/streak` | Memos API (200 memos) | Current/longest streak, heatmap, consistency |
| `/digest` | Memos API (50 memos) | Today's memos: timestamps, previews, word count |
| `/insight` | Memos API (100 memos) | Your most-used topics, deepest writing, re-read suggestion |
| `/read <url>` | Jina.ai Reader | Full article text + word count + read time |
| `/queue` | Memos API | Saved bookmarks reading list |
| `/review` | Memos API (100 memos) | Week highlights + tag breakdown + reflection |

## inbox (7)
| Command | Source | Data |
|---------|--------|------|
| `/save <text>` | Memos API + Jina.ai | URL auto-detect → metadata, word count, review date |
| `/summarize <url>` | Web scraping | URL summary in markdown |
| `/clip <url>` | Jina.ai Reader | Title, description, read time, domain, excerpt |
| `/note <text>` | Memos API + Wikipedia | Word count, auto-detect URLs/emails, Wikipedia topic tags, revisit date |
| `/research <url>` | Firecrawl/web scrape | Full article analysis: key points, action items, next steps |
| `/concept <topic>` | Memos API + Wikipedia + mermaid | Your notes + wiki summary + connected concepts + graph |
| `/flashback <topic>` | Memos API | Oldest vs newest memo, evolution timeline |

## music (7)
| Command | Source | Data |
|---------|--------|------|
| `/chord <name>` | Theory engine | Notes, intervals, guitar fingering, inversions |
| `/scale <root> <type>` | Theory engine | Notes, diatonic triads, relative key, practice |
| `/progress <key>` | Theory engine | Pop, blues, jazz, Andalusian progressions |
| `/circle` | Theory engine | Full circle of fifths + key of the day |
| `/song <title>` | MusicBrainz (30M+ recordings) | Artist, length, releases, links |
| `/artist <name>` | MusicBrainz (2M+ artists) | Type, country, years, tags, top releases |
| `/tempo <bpm>` | Math | ms per beat, delay times for pedals + DAW |

## Buckets
| Bucket | Commands | Purpose |
|--------|----------|---------|
| `bio` | 7 | PubMed, trials, molecules, pathways, amino acids, genomes, proteins |
| `dev` | 7 | GitHub, HTTP inspector, man pages, CSS/HTML/Astro docs, grep patterns |
| `news` | 7 | HN, arXiv, Lobsters, Product Hunt, Scholar, Reddit, general news |
| `learn` | 7 | Definitions, Wikipedia, translations, books, papers, YouTube, learning overviews |
| `wellness` | 7 | Food, workout, recipe, therapy (CBT+stoic), posture, calories, stretching |
| `money` | 7 | FX, stocks, crypto, markets, investing guide, side hustles, salary data |
| `tasks` | 7 | Vikunja CRUD (todo/goal/project/deadline) + weekly review + overdue + standup |
| `memos` | 7 | Daily note, streak, digest, insights, article reader, reading queue, weekly review |
| `inbox` | 7 | Save, summarize, clip, note, research, concept map, flashback |
| `music` | 7 | Chords, scales, progressions, circle of fifths, song/artist lookup, tempo |

## Setup
```bash
docker run \
  -e BOT_TOKEN=... \
  -e MEMOS_URL=... \
  -e BARK_URL=... \
  -e NTFY_URL=... \
  -e VIKUNJA_URL=... \
  -e VIKUNJA_TOKEN=... \
  -e API_NINJAS_KEY=... \
  ghcr.io/0124212/memogram-rs:latest
```

## Env
| Variable | Required | Purpose |
|----------|----------|---------|
| `BOT_TOKEN` | ✅ | Telegram bot token |
| `MEMOS_URL` | ✅ | Memos instance URL |
| `ADMIN_USERNAME` | | Your Memos username (default: admin) |
| `ALLOWED_USERNAMES` | | Comma-separated Telegram users |
| `DATA` | | Path to token store (default: ./data.txt) |
| `BOT_TOKENS_JSON` | | JSON map of bucket → Memos PAT |
| `BARK_URL` | | Bark push notification URL |
| `NTFY_URL` | | ntfy push notification URL |
| `VIKUNJA_URL` | | Vikunja instance URL |
| `VIKUNJA_TOKEN` | | Vikunja API token |
| `API_NINJAS_KEY` | | api.ninjas.com key (nutrition, calories) |
| `FIRECRAWL_KEY` | | Firecrawl key (research deep analysis) |
