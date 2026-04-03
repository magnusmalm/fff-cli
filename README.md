# fff

A fast file finder and grep for the terminal. Fuzzy search with typo tolerance, content grep with bigram pre-filtering, frecency-ranked results.

Built on [fff-core](https://github.com/dmtrKovalenko/fff.nvim) (the engine behind fff.nvim), exposed as a standalone CLI.

## Benchmarks

Tested on buildroot (13k files), x86-64 Linux. See `bench.sh` for methodology.

| Operation | fff | Competitor | Result |
|-----------|-----|-----------|--------|
| Grep `CONFIG_` | 13.9 ms | ripgrep 33.5 ms | **fff 2.4x faster** |
| Grep `wpa_supplicant` (rare) | 14.6 ms | ripgrep 33.4 ms | **fff 2.3x faster** |
| `--filter Makefile` (stdin) | 1.5 ms | fzf 2.6 ms | **fff 1.7x faster** |
| `search Makefile` (from index) | 11.5 ms | fzf 2.6 ms | fzf 4.5x faster* |

\* Index-based search pays ~10ms to load the file index from disk. The `--filter` mode avoids this entirely and beats fzf. The index enables features fzf can't do: frecency ranking, git-status boosting, and bigram-accelerated grep.

## Installation

### From source (recommended)

Requires Rust nightly (edition 2024 dependency):

```sh
cargo install --git https://github.com/magnusmalm/fff-cli
```

### Build locally

```sh
git clone https://github.com/magnusmalm/fff-cli
cd fff-cli
cargo build --release
cp target/release/fff ~/.local/bin/   # or anywhere on your $PATH
```

## Quick start

```sh
# Fuzzy file search (auto-indexes on first run)
fff search main.rs

# Shorthand: omit the subcommand
fff main.rs

# Grep file contents (2x faster than ripgrep)
fff grep "fn main"

# Pipe mode: drop-in fzf replacement (no index needed)
git ls-files | fff --filter controller
find . -name '*.rs' | fff --filter auth -n 10
```

## Usage

### `fff search <query>`

Fuzzy file search by name. Typo-tolerant (SIMD Smith-Waterman matching). Builds an index on first run (`.fff/` in project root), subsequent searches load from disk.

```sh
fff search Makefile           # exact-ish match
fff search Makeifle           # typo — still finds Makefile
fff search "src/ handler"     # constrain to src/ directory
fff search "*.rs controller"  # constrain to .rs files
fff search "git:modified"     # only modified files
```

Constraints can be combined:

```sh
fff search "git:modified src/**/*.rs !test/"
```

### `fff grep <pattern>`

Content search across all indexed files. Uses a bigram inverted index to skip files that can't possibly match before doing any I/O.

```sh
fff grep "fn main"                   # literal search (default)
fff grep -e "fn\s+\w+.*Result"      # regex mode
fff grep --fuzzy "mutex_lock"        # fuzzy content matching
fff grep -B 2 -A 5 "TODO"           # context lines
fff grep "*.rs CONFIG_"              # grep only in .rs files
```

### `fff --filter <query>`

Read lines from stdin, fuzzy match, print results. No index needed. Drop-in replacement for `fzf --filter`.

```sh
git ls-files | fff --filter auth
find . -type f | fff --filter test -n 20
cat urls.txt | fff --filter api
```

### `fff index [path]`

Build or rebuild the file index. Normally runs automatically on first search.

```sh
fff index                    # index current project
fff index --force            # force rebuild
fff index /path/to/project   # index a specific directory
```

### `fff watch`

Keep the index updated by watching the filesystem for changes.

```sh
fff watch    # runs until Ctrl-C
```

### `fff completions <shell>`

Generate shell completion scripts.

```sh
# bash
fff completions bash > ~/.local/share/bash-completion/completions/fff

# zsh
fff completions zsh > "${fpath[1]}/_fff"

# fish
fff completions fish > ~/.config/fish/completions/fff.fish
```

## Global options

| Flag | Description |
|------|-------------|
| `-C, --directory <PATH>` | Project root (defaults to git root or cwd) |
| `-n, --max-results <N>` | Maximum results, default 50 |
| `--json` | Output as NDJSON (one JSON object per line) |
| `--debug` | Show scoring breakdown per result |
| `--frecency-db <PATH>` | Override frecency database path |

## How it works

### Index (`fff index`)

1. Walks the file tree using the `ignore` crate (respects `.gitignore`, `.ignore`)
2. Builds a **bigram inverted index** — for each consecutive character pair in every file's content, a bitset records which files contain it
3. Writes three files to `.fff/`:
   - `files.bin` — file list in a compact binary format (mmap-friendly string table + fixed records)
   - `bigram.bin` — the bigram index (65536-entry lookup table + dense bitset columns)
   - `manifest.bin` — file count, git HEAD hash, timestamp for staleness detection

### Search (`fff search`)

1. Loads `files.bin` via mmap
2. Runs SIMD-accelerated fuzzy matching ([neo_frizbee](https://docs.rs/neo_frizbee)) across all file paths in parallel
3. Scores results using: fuzzy match quality + frecency (frequency + recency of access) + git status boost (modified files rank higher) + filename bonus + directory distance penalty
4. Returns top N results sorted by score

### Grep (`fff grep`)

1. Loads the bigram index
2. Extracts bigrams from the search pattern, ANDs the corresponding bitset columns — this eliminates ~90% of files before any file I/O
3. For surviving candidates: mmaps each file and runs the grep matcher (literal, regex, or fuzzy)
4. Returns matches with file path, line number, column, and content

### Filter (`fff --filter`)

Pure pipe mode — reads lines from stdin, runs parallel SIMD fuzzy matching, prints sorted results. No index, no disk I/O beyond stdin. This is how fff beats fzf: same streaming model but with a faster matcher.

### Frecency

Shares the LMDB frecency database with [fff.nvim](https://github.com/dmtrKovalenko/fff.nvim) and the fff MCP server. Files you open frequently or recently get boosted in search results. Auto-detected at:

1. `~/.cache/nvim/fff_nvim` (shared with neovim plugin)
2. `.fff/frecency` (project-local)
3. `--frecency-db` or `FFF_FRECENCY_DB` env var (explicit override)

### Staleness

The index tracks the git HEAD hash and a timestamp. On each search:
- HEAD changed since indexing: warns, uses stale index (still fast)
- Index older than 30 minutes: warns
- No index found: auto-builds before searching

Run `fff index --force` to rebuild, or `fff watch` for live updates.

## JSON output

All commands support `--json` for machine-readable NDJSON output:

```sh
fff search main --json
# {"path":"src/main.rs","score":74,"match_type":"exact_filename","frecency":0}

fff grep "fn main" --json
# {"path":"src/main.rs","line_number":15,"column":0,"line":"fn main() {","byte_offset":234,"is_definition":true}
```

## Project structure

```
src/
  main.rs              entry point, clap dispatch, mimalloc allocator
  cli.rs               argument definitions
  error.rs             error types, exit codes (0=match, 1=no match, 2=error)
  commands/
    search.rs          fff search — index-based fuzzy file search
    grep.rs            fff grep — bigram-filtered content search
    filter.rs          fff --filter — stdin pipe fuzzy matching
    index.rs           fff index — build/rebuild the index
    watch.rs           fff watch — filesystem watcher
  index/
    mod.rs             index lifecycle: build, load, staleness, frecency
    format.rs          binary serialization (v2: string table + fixed records)
    staleness.rs       git HEAD + timestamp freshness checks
  output/
    human.rs           colored terminal output
    json.rs            NDJSON output
```

## Dependencies

fff-cli wraps [fff-core](https://github.com/dmtrKovalenko/fff.nvim) (the `fff-search` crate) which provides:

- SIMD fuzzy matching via [neo_frizbee](https://docs.rs/neo_frizbee) (AVX2/NEON Smith-Waterman)
- Custom case-insensitive `memmem` with AVX2 packed-pair scanning
- LMDB-backed frecency tracking via [heed](https://docs.rs/heed)
- `.gitignore`-aware file walking via [ignore](https://docs.rs/ignore)
- Parallel processing via [rayon](https://docs.rs/rayon)

The CLI adds: [clap](https://docs.rs/clap) for argument parsing, [mimalloc](https://docs.rs/mimalloc) as the global allocator, [crossterm](https://docs.rs/crossterm) for colored output.

## Requirements

- **Rust nightly** — fff-core uses edition 2024 (the `rust-toolchain.toml` handles this automatically)
- **Git** — for `.gitignore` support and staleness detection (optional but recommended)

## License

MIT
