#  papyrus

<p align="center">
  <strong>Search academic papers from your terminal  -  fast, filterable, exportable, agent-ready.</strong>
</p>

<p align="center">
  <a href="https://crates.io/crates/papyrus"><img src="https://img.shields.io/crates/v/papyrus?style=for-the-badge" alt="crates.io"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-blue.svg?style=for-the-badge" alt="MIT License"></a>
</p>

<p align="center">
  A Ratatui TUI that queries arXiv, Semantic Scholar, PubMed, and CrossRef simultaneously.<br>
  Navigate with vim keys · view abstracts inline · copy DOIs · open PDFs · export to JSON, CSV, or BibTeX.<br>
  Responses are cached to disk. Per-source rate limits enforced. MCP server built in.
</p>

<!-- demo gif here -->

```
┌─────────────────────────────────────────────────────────────────────┐
│  papyrus  v0.1.0         [arXiv] [S2] [PubMed] [CrossRef]  ⠙ fetching│
├──────────────────────────────────────────────────────────────────────┤
│  Filters: [q: "neural scaling"] [from: 2023] [has-pdf] [cat: cs.AI] │
├────────────────────────────┬─────────────────────────────────────────┤
│  Results (47 found)        │  Detail View                            │
│  ──────────────────        │  ─────────────────────────────────      │
│  1. Scaling Laws for...   │  Title:      Scaling Laws for Neural... │
│   2. Neural Scaling and... │  Authors:    Hoffmann, J. et al.        │
│   3. Emergent Abilities... │  Date:       2022-03-29                 │
│   4. Training Compute-...  │  Source:     arXiv [2203.15556]         │
│   5. Beyond Neural Scal... │  Citations:  1,842                      │
│   6. Revisiting Scaling... │  Categories: cs.LG, cs.CL               │
│                            │  Journal:     -                           │
│                            │  DOI:        10.48550/arXiv.2203.15556  │
│                            │                                         │
│                            │  Abstract:                              │
│                            │  We investigate the optimal...          │
│                            │                                         │
│                            │  [p] PDF  [Enter] HTML  [b] BibTeX      │
├────────────────────────────┴─────────────────────────────────────────┤
│  [/] Search  [f] Filters  [e] Export  [r] Refresh  [q] Quit  [?] Help│
└──────────────────────────────────────────────────────────────────────┘
```

---

## Installation

```bash
git clone https://github.com/ptysiuuu/papyrus
cd papyrus
cargo install --path .
```
---

## Quick start

```bash
# Interactive TUI
papyrus

# Pre-filled search
papyrus -q "large language models" --from 2024 --has-pdf

# Headless / pipeline mode
papyrus --no-tui -q "diffusion models" -n 50 | jq '.[].pdf_url'

# Stream results line by line (NDJSON)
papyrus --no-tui --output-mode jsonl -q "diffusion models" -n 50

# MCP server for Claude Code / other agents
papyrus serve

# Force fresh fetch, skip cache
papyrus -q "transformers" --no-cache
```

---

## CLI flags

### Core

| Flag | Short | Type | Description |
|------|-------|------|-------------|
| `--query` | `-q` | String | Full-text keyword query. Supports quoted phrases: `"neural scaling"` |
| `--source` | `-s` | Vec | Sources to query: `arxiv`, `semantic`, `pubmed`, `crossref`, `all` |

### Content filters

| Flag | Short | Type | Description |
|------|-------|------|-------------|
| `--author` | `-a` | Vec | Filter by author. Repeatable: `-a "Hinton" -a "LeCun"` |
| `--title` | | String | Search within titles only |
| `--abstract` | | String | Search within abstracts only |
| `--category` | `-c` | Vec | Subject category: `cs.AI`, `physics`, `medicine`, ... |
| `--journal` | `-j` | String | Filter by journal or venue name |
| `--doi` | | String | Fetch a specific paper by DOI |
| `--arxiv-id` | | String | Fetch a specific paper by arXiv ID (e.g. `2301.07041`) |

### Date filters

| Flag | Short | Type | Description |
|------|-------|------|-------------|
| `--from` | | String | Published on or after. Formats: `YYYY`, `YYYY-MM`, `YYYY-MM-DD` |
| `--to` | | String | Published on or before. Same formats as `--from` |
| `--year` | `-y` | u16 | Shorthand for `--from YYYY --to YYYY` |
| `--last-days` | | u32 | Papers published in the last N days |
| `--last-months` | | u32 | Papers published in the last N months |

### Quality filters

| Flag | Type | Description |
|------|------|-------------|
| `--min-citations` | u32 | Minimum citation count |
| `--max-citations` | u32 | Maximum citation count |
| `--has-pdf` | flag | Only papers with a freely accessible PDF |
| `--has-code` | flag | Only papers linked to a code repository |
| `--peer-reviewed` | flag | Exclude preprints |
| `--preprint-only` | flag | Only preprints (arXiv, bioRxiv, ...) |
| `--open-access` | flag | Only open-access papers |

### Output / pagination

| Flag | Short | Type | Description |
|------|-------|------|-------------|
| `--limit` | `-n` | u32 | Max results per source. Default: `20`. Max: `500` |
| `--offset` | | u32 | Skip first N results |
| `--sort` | | String | `relevance` (default), `date-desc`, `date-asc`, `citations-desc` |
| `--output` | `-o` | Path | Export file. Extension sets format: `.json`, `.csv`, `.bib` |
| `--format` | `-f` | String | Override export format: `json`, `csv`, `bibtex` |
| `--no-tui` | | flag | Headless mode  -  print to stdout |
| `--output-mode` | | String | Output format in `--no-tui`: `json` (default), `jsonl`, `pretty` |
| `--quiet` | | flag | Suppress progress output in `--no-tui` mode |
| `--no-cache` | | flag | Bypass disk cache and force a fresh fetch |

### Config / misc

| Flag | Type | Description |
|------|------|-------------|
| `--config` | Path | Config file path. Default: `~/.config/papyrus/config.toml` |
| `--api-key` | String | Runtime key override  -  applies to all keyed sources |
| `--timeout` | u32 | HTTP timeout in seconds. Default: `15` |
| `--retries` | u32 | Retries on failure. Default: `3` |
| `--concurrent` | u32 | Max concurrent requests. Default: `4` |
| `--verbose` | `-v` | flag | Log HTTP requests to stderr |

---

## Subcommands

### `papyrus keys`  -  API key management

```bash
papyrus keys set semantic <key>   # store a Semantic Scholar key
papyrus keys set pubmed <key>     # store a PubMed key
papyrus keys list                 # show configured keys (values masked)
papyrus keys remove semantic      # delete a stored key
```

Keys are written to `~/.config/papyrus/config.toml` under `[api_keys]`. Valid source names: `semantic`, `pubmed`.

### `papyrus cache`  -  disk cache management

```bash
papyrus cache stats   # entry count and disk usage
papyrus cache clear   # delete all cached responses
```

### `papyrus serve`  -  MCP server

```bash
papyrus serve         # start MCP server over stdio
```

Exposes `search_papers`, `get_paper`, and `export_papers` as MCP tools. See the [agent integration](#agent-integration) section for host config.

### `papyrus schema`  -  JSON schema

```bash
papyrus schema input    # search filter schema
papyrus schema output   # paper output schema
papyrus schema all      # both, as { "input": ..., "output": ... }
```

---

## TUI keybindings

| Key | Action |
|-----|--------|
| `/` | Open search modal |
| `f` | Open filter modal |
| `e` | Open export modal |
| `r` | Re-run current search |
| `q` | Quit |
| `?` | Help screen |
| `j` / `↓` | Move down in results |
| `k` / `↑` | Move up in results |
| `J` / `Shift+↓` | Scroll detail panel down |
| `K` / `Shift+↑` | Scroll detail panel up |
| `g` / `G` | Jump to first / last result |
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

---

## Configuration

Config file: `~/.config/papyrus/config.toml`  -  created automatically on first run.

```toml
[general]
default_sources    = ["arxiv", "semantic"]
default_limit      = 20
timeout_seconds    = 15
retries            = 3
concurrent_requests = 4
default_sort       = "relevance"
cache_ttl_minutes  = 60          # 0 to disable caching

[api_keys]
# Semantic Scholar  -  https://www.semanticscholar.org/product/api
# semantic_scholar = ""
# PubMed  -  https://www.ncbi.nlm.nih.gov/account/
# pubmed = ""

[output]
default_export_path = "~/papers"
default_format      = "json"

[network]
user_agent   = "papyrus/0.1.0 (mailto:you@example.com)"
polite_email = ""    # CrossRef polite pool  -  better rate limits

[ui]
show_abstracts_in_list = false
color_theme            = "dark"   # "dark" | "light"
date_format            = "%Y-%m-%d"
```

### Paths

| Purpose | Default path |
|---------|-------------|
| Config | `~/.config/papyrus/config.toml` |
| Response cache | `~/.local/share/papyrus/cache/` |
| Logs | `~/.local/share/papyrus/` |

---

## API keys

Keys unlock higher rate limits. Resolution order  -  first match wins:

1. `--api-key` CLI flag *(applies to all keyed sources)*
2. Environment variables: `PAPYRUS_SEMANTIC_KEY`, `PAPYRUS_PUBMED_KEY`
3. `[api_keys]` section in `config.toml`

The env var path is useful in CI or scripts without touching the config file:

```bash
PAPYRUS_SEMANTIC_KEY=sk-xxx papyrus --no-tui -q "transformers" -s semantic -n 100
```

---

## Rate limits

Rate limits are enforced with a token-bucket algorithm per source  -  no sleep-based throttling. On HTTP 429, the affected source backs off per the `Retry-After` header and retries once; other sources continue unaffected.

| Source | Without key | With key |
|--------|------------|----------|
| arXiv | 1 req / 3 s |  -  |
| Semantic Scholar | 100 req / 5 min | 1 req / s |
| PubMed | 3 req / s | 10 req / s |
| CrossRef | 4 req / s (polite pool) |  -  |

---

## Response cache

Responses are cached at `~/.local/share/papyrus/cache/` as gzip-compressed JSON, keyed by a SHA-256 hash of the query + source. Default TTL is 1 hour (`cache_ttl_minutes` in config). Sources that returned cached results show a `[cached]` badge in the TUI header.

```bash
papyrus cache stats              # 12 entries, 48.3 KB on disk
papyrus cache clear              # wipe all entries
papyrus -q "..." --no-cache      # bypass cache for this run
```

---

## Data sources

| Source | API | Key |
|--------|-----|-----|
| [arXiv](https://arxiv.org/help/api/) | Atom/XML | Not required |
| [Semantic Scholar](https://api.semanticscholar.org/) | JSON REST | Optional  -  [get one](https://www.semanticscholar.org/product/api) |
| [PubMed](https://www.ncbi.nlm.nih.gov/home/develop/api/) | XML E-utilities | Optional  -  [get one](https://www.ncbi.nlm.nih.gov/account/) |
| [CrossRef](https://www.crossref.org/documentation/retrieve-metadata/rest-api/) | JSON REST | Not required (set `polite_email` for priority) |

---

## Agent integration

papyrus exposes a first-class interface for AI agents and shell scripts.

### MCP server (Claude Code, any MCP host)

`papyrus serve` launches an MCP server over stdio. Add it to your MCP host config once and call its tools from any conversation:

```json
{
  "mcpServers": {
    "papyrus": {
      "command": "papyrus",
      "args": ["serve"]
    }
  }
}
```

Three tools are exposed:

| Tool | Description |
|------|-------------|
| `search_papers` | Search across all configured sources; returns full paper objects |
| `get_paper` | Fetch a single paper by DOI, arXiv ID, or PubMed ID |
| `export_papers` | Export paper IDs from a previous search to JSON / CSV / BibTeX |

Full input/output schemas are registered in the tool manifest so the host can auto-generate parameters without hardcoding them.

### JSON schema subcommand

```bash
papyrus schema input    # FilterSet schema  -  what search_papers accepts
papyrus schema output   # Paper schema  -  what search_papers returns
papyrus schema all      # Both, as { "input": ..., "output": ... }
```

Agents can call this once at startup to learn the interface dynamically.

### Output modes (`--no-tui` only)

| Mode | Description |
|------|-------------|
| `--output-mode json` | Array of Paper objects. Default. |
| `--output-mode jsonl` | One Paper object per line (NDJSON), emitted per-source as results arrive. Final line is a `{"__meta": true, ...}` summary. |
| `--output-mode pretty` | Human-readable table: index, title, year, source, citations. |

### Exit codes

| Code | Meaning |
|------|---------|
| `0` | Full success  -  all sources responded |
| `1` | Partial success  -  some sources failed; results from others are in stdout |
| `2` | Total failure  -  no results from any source |
| `3` | Input error  -  bad arguments or config (JSON error on stderr) |
| `4` | Rate limited  -  all sources hit rate limits |

Exit code 1 is not a reason to abort. Inspect `sources_degraded` in the JSONL `__meta` line and decide whether to retry the missing sources.

### Shell / script agents

```bash
# Discover the schema first
papyrus schema input > /tmp/papyrus_input_schema.json

# Stream results as they arrive, skip the meta line
papyrus --no-tui --output-mode jsonl -q "attention mechanism" --from 2023 \
  | while IFS= read -r line; do
      echo "$line" | jq 'select(.__meta == null) | .title'
    done

# Check exit code for partial failure
papyrus --no-tui -q "transformers" -s all -n 100 -o results.bib
[ $? -eq 1 ] && echo "Warning: some sources failed, results may be incomplete"
```

---

## Examples

```bash
# LLM papers from 2024 with PDF, interactive TUI
papyrus -q "large language models" --from 2024 --has-pdf

# Top 100 cited RL papers, exported to BibTeX
papyrus --no-tui -q "reinforcement learning" --sort citations-desc -n 100 -o papers.bib

# Specific author, peer-reviewed, last 6 months
papyrus -a "Yann LeCun" --peer-reviewed --last-months 6

# Fetch by arXiv ID
papyrus --arxiv-id 2301.07041

# Multi-source, two categories, date range
papyrus -q "transformer" -c cs.AI -c cs.LG --from 2022 --to 2024 -s arxiv -s semantic

# Extract all PDF links, skipping cache
papyrus --no-tui -q "diffusion models" --from 2023 --has-pdf -n 50 --no-cache | jq '.[].pdf_url'

# Configure API key, then run at higher rate limit
papyrus keys set semantic sk-...
PAPYRUS_PUBMED_KEY=xxx papyrus -q "CRISPR" -s pubmed -n 100
```

---

## License

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
