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
  Local library · citation graphs · watch mode · PDF downloads · plugin system · MCP server built in.
</p>

<!-- demo gif here -->

```
┌───────────────────────────────────────────────────────────────────────┐
│  papyrus  v1.0.0         [arXiv] [S2] [PubMed] [CrossRef]   fetching  │
├───────────────────────────────────────────────────────────────────────┤
│  Filters: [q: "neural scaling"] [from: 2023] [has-pdf] [cat: cs.AI]   │
├────────────────────────────┬──────────────────────────────────────────┤
│  Results (47 found)        │  Detail View                             │
│  ──────────────────        │  ─────────────────────────────────       │
│  1. Scaling Laws for...    │  Title:      Scaling Laws for Neural...  │
│   2. Neural Scaling and... │  Authors:    Hoffmann, J. et al.         │
│   3. Emergent Abilities... │  Date:       2022-03-29                  │
│   4. Training Compute-...  │  Source:     arXiv [2203.15556]          │
│   5. Beyond Neural Scal... │  Citations:  1,842                       │
│   6. Revisiting Scaling... │  Categories: cs.LG, cs.CL                │
│                            │  Journal:     -                          │
│                            │  DOI:        10.48550/arXiv.2203.15556   │
│                            │                                          │
│                            │  Abstract:                               │
│                            │  We investigate the optimal...           │
│                            │                                          │
│                            │  [p] PDF  [Enter] HTML  [b] BibTeX       │
├────────────────────────────┴──────────────────────────────────────────┤
│  [/] Search  [f] Filters  [e] Export  [r] Refresh  [q] Quit  [?] Help │
└───────────────────────────────────────────────────────────────────────┘
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

# Pre-filled search (auto-saves results to local library)
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
| `--last-months` | | u32 | Papers published in the last N months (year-boundary safe) |

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

### `papyrus library`  -  local paper library

All search results are automatically saved to a local SQLite library at `~/.local/share/papyrus/papyrus.db`. The library supports full-text search, tagging, reading status, notes, and collections.

```bash
# Full-text search (title + abstract)
papyrus library search "attention mechanism"

# Include PDF full-text in search (if downloaded)
papyrus library search "gradient vanishing" --fulltext

# Show library statistics
papyrus library stats

# Set reading status: unread | reading | read | reviewed
papyrus library status <paper-id> read

# Add a note
papyrus library note <paper-id> "Key paper for my lit review section 3"

# Set priority 1–5
papyrus library priority <paper-id> 5

# Tag a paper (multiple tags)
papyrus library tag <paper-id> nlp transformers attention

# Remove a tag
papyrus library untag <paper-id> nlp

# Create and manage collections
papyrus library create-collection "My Lit Review"
papyrus library list-collections

# Find potential duplicates
papyrus library duplicates

# Export a literature review (sorted by priority, then citations)
papyrus library export-review --output review.json
papyrus library export-review --output review.bib --format bibtex
```

Paper IDs are full UUIDs shown in library search output or retrievable from JSON output (`id` field).

### `papyrus cite-graph`  -  citation graph engine

Builds a local citation graph using Semantic Scholar data. Stores nodes and edges in SQLite for offline traversal.

```bash
# Fetch and store references + citations for a paper (by S2 paper ID)
papyrus cite-graph fetch <s2-paper-id>

# Walk backwards through references (depth 1 = direct refs)
papyrus cite-graph ancestors <s2-paper-id> --depth 2

# Walk forward through citations
papyrus cite-graph descendants <s2-paper-id> --depth 1

# Find shared references between two papers
papyrus cite-graph common <s2-paper-id-1> <s2-paper-id-2>

# Show highest-cited root nodes in your local graph
papyrus cite-graph seminal --limit 10
```

Semantic Scholar paper IDs are the hex IDs returned in `source_id` when searching with `-s semantic`.

### `papyrus watch`  -  watch mode for new papers

Saves persistent queries that can be re-run to detect new papers. Outputs JSONL for piping to cron jobs or notification scripts.

```bash
# Add a watch (sources: arxiv, semantic_scholar, pubmed, crossref)
papyrus watch add "diffusion models" --sources "arxiv,semantic_scholar" --name "Diffusion research"

# List saved watches
papyrus watch list

# Run all watches — prints JSONL for new papers only
papyrus watch run

# Run in a specific output mode
papyrus watch run --output-mode jsonl

# Remove a watch by ID
papyrus watch remove <watch-id>
```

Each JSONL line from `watch run` includes `__watch_name` and `__watch_query` metadata fields alongside the full paper object. Subsequent runs skip already-seen papers.

**Cron example:**

```bash
# Check for new papers every morning and log them
0 8 * * * papyrus watch run >> ~/papers/new-papers.jsonl
```

### `papyrus similar`  -  paper similarity & recommendations

Find papers similar to a given one, either via Semantic Scholar's recommendations API or offline using TF-IDF cosine similarity against your local library.

```bash
# S2 API recommendations (requires S2 paper ID)
papyrus similar <s2-paper-id> --limit 10

# Offline TF-IDF similarity from your local library
papyrus similar <paper-uuid> --from-library --limit 5
```

The `--from-library` mode ranks all papers in your library by cosine similarity to the query paper's title and abstract — no network required.

### `papyrus download`  -  PDF download & indexing

Downloads PDFs and organizes them by year and first author. Optionally extracts full text via `pdftotext` and indexes it in the library for `--fulltext` search.

```bash
# Download a single paper by UUID (from your library)
papyrus download <paper-uuid>

# Specify a target directory
papyrus download <paper-uuid> --dir ~/papers

# Download all papers with PDF URLs from the last search
papyrus download --all --dir ~/papers
```

PDFs are saved as `<year>/<author>/<year>_<author>_<title-slug>.pdf`. If `pdftotext` is on your PATH, full text is extracted and stored in the library automatically.

### `papyrus plugins`  -  plugin system

Extend papyrus with custom data sources. Plugins are external binaries that communicate over a JSON stdin/stdout protocol.

```bash
# List installed plugins
papyrus plugins list

# Install a plugin (copies directory to plugins dir)
papyrus plugins install ./my-plugin/
```

**Plugin directory:** `~/.config/papyrus/plugins/`

Each plugin lives in its own subdirectory and contains a `manifest.toml`:

```toml
name        = "biorxiv"
version     = "1.0.0"
description = "bioRxiv preprint search"
binary      = "biorxiv-plugin"
sources     = ["biorxiv"]
```

When a search runs, papyrus sends a JSON request to the plugin binary on stdin and reads a JSON response from stdout:

```json
// Request (stdin)
{ "action": "search", "query": "CRISPR", "limit": 20, "date_from": "2024-01-01" }

// Response (stdout)
{ "papers": [{ "id": "...", "title": "...", "authors": [...], "year": 2024 }], "total": 42 }
```

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

Exposes 7 MCP tools for AI agents. See the [agent integration](#agent-integration) section.

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
| `i` | Save selected paper to local library |
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
user_agent   = "papyrus/1.0.0 (mailto:you@example.com)"
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
| Library database | `~/.local/share/papyrus/papyrus.db` |
| Response cache | `~/.local/share/papyrus/cache/` |
| Downloaded PDFs | `~/papers/` (configurable via `--dir`) |
| Plugins | `~/.config/papyrus/plugins/` |

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

## Deduplication

Results fetched from multiple sources are automatically deduplicated before display or export. The dedup pipeline runs in order:

1. **DOI match** — same DOI → merge
2. **arXiv ID match** — same arXiv ID → merge
3. **Fuzzy title match** — Jaccard similarity ≥ 0.85 on character trigrams → merge

When merging, the record with more fields populated is kept (richest-record strategy). Title normalization folds Unicode Latin-extended characters and strips punctuation before comparison.

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

Seven tools are exposed:

| Tool | Description |
|------|-------------|
| `search_papers` | Search across all configured sources; returns full paper objects |
| `get_paper` | Fetch a single paper by DOI, arXiv ID, or PubMed ID |
| `export_papers` | Export paper IDs from a previous search to JSON / CSV / BibTeX |
| `literature_review` | Multi-source search with dedup, sorted by citations; agent-optimized output |
| `explore_citations` | Fetch and traverse the citation graph for a paper (BFS, configurable depth) |
| `check_watches` | Check saved watches for new papers; optionally mark as seen |
| `similar_papers` | Find papers similar to a given one via S2 API or offline TF-IDF |

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

# Watch for new papers and pipe to a notification script
papyrus watch run | jq -r '.__watch_name + ": " + .title' | notify-send -
```

---

## Examples

```bash
# LLM papers from 2024 with PDF, interactive TUI (auto-saved to library)
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

# Build a citation graph and find seminal papers
papyrus cite-graph fetch 204e3073870fae3d05bcbc2f6a8e263d9b72e776
papyrus cite-graph seminal --limit 10

# Find papers similar to one in your library
papyrus similar <paper-uuid> --from-library --limit 5

# Set up a watch and run it from cron
papyrus watch add "large language models" --sources "arxiv" --name "LLM watch"
papyrus watch run >> ~/papers/new-papers.jsonl

# Download a paper PDF with full-text indexing
papyrus download <paper-uuid> --dir ~/papers
```

---

## License

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
