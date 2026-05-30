# papyrus

**Search academic papers from your terminal — fast, filterable, exportable.**

A Ratatui TUI for querying arXiv, Semantic Scholar, PubMed, and CrossRef simultaneously. Navigate results with vim keys, view abstracts inline, copy DOIs, open PDFs, and export to JSON, CSV, or BibTeX — all without leaving the terminal.

<!-- demo gif here -->
```
┌─────────────────────────────────────────────────────────────────────┐
│  papyrus  v0.1.0         [arXiv] [S2] [PubMed] [CrossRef]  ⠙ fetching│
├──────────────────────────────────────────────────────────────────────┤
│  Filters: [q: "neural scaling"] [from: 2023] [has-pdf] [cat: cs.AI] │
├────────────────────────────┬─────────────────────────────────────────┤
│  Results (47 found)        │  Detail View                            │
│  ──────────────────        │  ─────────────────────────────────      │
│▶  1. Scaling Laws for…     │  Title: Scaling Laws for Neural…        │
│   2. Neural Scaling and…   │  Authors: Hoffmann, J. et al.           │
│   3. Emergent Abilities…   │  Date:   2022-03-29                     │
│   4. Training Compute-…    │  Source: arXiv [2203.15556]             │
│   5. Beyond Neural Scal…   │  Citations: 1,842                       │
│   6. Revisiting Scaling…   │  Categories: cs.LG, cs.CL              │
│                            │  Journal: —                             │
│                            │  DOI: 10.48550/arXiv.2203.15556         │
│                            │                                         │
│                            │  Abstract:                              │
│                            │  We investigate the optimal…            │
│                            │                                         │
│                            │  [p] PDF  [Enter] HTML  [b] BibTeX      │
├────────────────────────────┴─────────────────────────────────────────┤
│  [/] Search  [f] Filters  [e] Export  [r] Refresh  [q] Quit  [?] Help│
└──────────────────────────────────────────────────────────────────────┘
```

## Installation

```bash
cargo install papyrus
```

Or build from source:

```bash
git clone https://github.com/your-username/papyrus
cd papyrus
cargo build --release
# binary at ./target/release/papyrus
```

## Usage

```bash
# Interactive TUI — opens with no arguments
papyrus

# TUI with pre-filled search
papyrus -q "large language models" --from 2024 --has-pdf

# Batch / headless mode
papyrus --no-tui -q "diffusion models" -n 50 | jq '.[].pdf_url'
```

## CLI Flags

### Core

| Flag | Short | Type | Description |
|------|-------|------|-------------|
| `--query` | `-q` | String | Full-text keyword query. Supports quoted phrases: `"neural scaling"` |
| `--source` | `-s` | Vec | Sources to query. Values: `arxiv`, `semantic`, `pubmed`, `crossref`, `all` |

### Content Filters

| Flag | Short | Type | Description |
|------|-------|------|-------------|
| `--author` | `-a` | Vec | Filter by author name(s). Repeatable: `-a "Hinton" -a "LeCun"` |
| `--title` | | String | Search within titles only |
| `--abstract` | | String | Search within abstracts only |
| `--category` | `-c` | Vec | Subject category. E.g. `cs.AI`, `physics`, `medicine` |
| `--journal` | `-j` | String | Filter by journal/venue name |
| `--doi` | | String | Fetch a specific paper by DOI |
| `--arxiv-id` | | String | Fetch a specific paper by arXiv ID (e.g. `2301.07041`) |

### Date Filters

| Flag | Short | Type | Description |
|------|-------|------|-------------|
| `--from` | | String | Published on or after. Formats: `YYYY`, `YYYY-MM`, `YYYY-MM-DD` |
| `--to` | | String | Published on or before. Same formats as `--from` |
| `--year` | `-y` | u16 | Shorthand for `--from YYYY --to YYYY` |
| `--last-days` | | u32 | Papers published in the last N days |
| `--last-months` | | u32 | Papers published in the last N months |

### Quality Filters

| Flag | Type | Description |
|------|------|-------------|
| `--min-citations` | u32 | Minimum citation count |
| `--max-citations` | u32 | Maximum citation count |
| `--has-pdf` | flag | Only papers with a freely accessible PDF link |
| `--has-code` | flag | Only papers linked to a code repository |
| `--peer-reviewed` | flag | Exclude preprints |
| `--preprint-only` | flag | Only preprints (arXiv, bioRxiv, etc.) |
| `--open-access` | flag | Only open-access papers |

### Output / Pagination

| Flag | Short | Type | Description |
|------|-------|------|-------------|
| `--limit` | `-n` | u32 | Max results per source. Default: `20`. Max: `500` |
| `--offset` | | u32 | Skip first N results |
| `--sort` | | String | `relevance` (default), `date-desc`, `date-asc`, `citations-desc` |
| `--output` | `-o` | Path | Export results. Extension sets format: `.json`, `.csv`, `.bib` |
| `--format` | `-f` | String | Override export format: `json`, `csv`, `bibtex` |
| `--no-tui` | | flag | Headless/batch mode — print JSON to stdout |
| `--quiet` | | flag | Suppress progress output in `--no-tui` mode |

### Config / Misc

| Flag | Type | Description |
|------|------|-------------|
| `--config` | Path | Config file path. Default: `~/.config/papyrus/config.toml` |
| `--api-key` | String | API key override (Semantic Scholar, PubMed) |
| `--timeout` | u32 | HTTP timeout in seconds. Default: `15` |
| `--retries` | u32 | Retries on failure. Default: `3` |
| `--concurrent` | u32 | Max concurrent requests. Default: `4` |
| `--verbose` | `-v` | flag | Log HTTP requests to stderr |

## TUI Keybindings

| Key | Action |
|-----|--------|
| `/` | Open search input modal |
| `f` | Open filter modal |
| `e` | Open export modal |
| `r` | Re-run current search |
| `q` | Quit |
| `?` | Help screen |
| `j` / `↓` | Move down in results |
| `k` / `↑` | Move up in results |
| `J` / `Shift+↓` | Scroll detail panel down |
| `K` / `Shift+↑` | Scroll detail panel up |
| `g` | Jump to first result |
| `G` | Jump to last result |
| `Tab` | Switch focus: results ↔ detail |
| `Enter` | Open paper HTML in browser |
| `p` | Open PDF in browser |
| `c` | Open code repository |
| `d` | Copy DOI to clipboard |
| `y` | Copy title to clipboard |
| `b` | Add paper to BibTeX buffer |
| `t` | Add/edit tag on current paper |
| `Ctrl+F` | Fuzzy-filter loaded results |
| `n` / `N` | Next / previous page |
| `Esc` | Close modal / cancel |
| `Ctrl+C` | Force quit |

## Configuration

Config file: `~/.config/papyrus/config.toml`

```toml
[general]
default_sources = ["arxiv", "semantic"]
default_limit = 20
timeout_seconds = 15
retries = 3
concurrent_requests = 4
default_sort = "relevance"

[api_keys]
semantic_scholar = ""   # Optional — increases rate limit from 100/5min to 1/sec
pubmed = ""             # Optional — increases rate limit from 3/sec to 10/sec

[output]
default_export_path = "~/papers"
default_format = "json"

[network]
user_agent = "papyrus/0.1.0 (mailto:you@example.com)"
polite_email = ""       # Used in CrossRef polite pool for priority access

[ui]
show_abstracts_in_list = false
color_theme = "dark"    # "dark" | "light"
date_format = "%Y-%m-%d"
```

Logs are written to `~/.local/share/papyrus/`.

## Data Sources

| Source | API | Key Required |
|--------|-----|-------------|
| [arXiv](https://arxiv.org/help/api/) | Atom/XML | No |
| [Semantic Scholar](https://api.semanticscholar.org/) | JSON REST | Optional (higher rate limit) |
| [PubMed](https://www.ncbi.nlm.nih.gov/home/develop/api/) | XML E-utilities | Optional (higher rate limit) |
| [CrossRef](https://www.crossref.org/documentation/retrieve-metadata/rest-api/) | JSON REST | No (polite pool via email) |

## Examples

```bash
# Interactive TUI: LLM papers from 2024 with PDF
papyrus -q "large language models" --from 2024 --has-pdf

# Batch: top 100 cited RL papers, export to BibTeX
papyrus --no-tui -q "reinforcement learning" --sort citations-desc -n 100 -o papers.bib

# Specific author, peer-reviewed, last 6 months
papyrus -a "Yann LeCun" --peer-reviewed --last-months 6

# Fetch by arXiv ID
papyrus --arxiv-id 2301.07041

# Multi-source, cs.AI + cs.LG, 2022–2024
papyrus -q "transformer" -c cs.AI -c cs.LG --from 2022 --to 2024 -s arxiv -s semantic

# Pipeline: extract all PDF links
papyrus --no-tui -q "diffusion models" --from 2023 --has-pdf -n 50 | jq '.[].pdf_url'
```

## License

<!-- license badge here -->
MIT
