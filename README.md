# jitpipe

**JIT-compiled semantic Unix pipes (`sgrep`, `sawk`, `sjq`) — samples ~25 rows of `stdin`, synthesizes a deterministic filter, and streams the rest at native Rust speed.**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2021-orange.svg)](Cargo.toml)

---

## Overview

Classic Unix text utilities (`grep`, `awk`, `jq`) are fast and composable, but require brittle regular expressions or rigid schema knowledge when confronted with messy application logs, inconsistent delimiters, or deeply nested JSONL streams.

Existing AI CLI pipes fail at scale in two ways:
1. **Per-line LLM calls** take minutes and cost dollars on a 500,000-line log file.
2. **Embedding search tools** cannot perform exact field projection, numeric comparisons (`latency_ms > 500`), or structured JSON transformation.

`jitpipe` applies **Just-In-Time (JIT) compilation** to Unix streams across four binaries (`jitpipe`, `sgrep`, `sawk`, `sjq`):

1. **Sample (`~25 rows`)**: Buffers only the first `N` rows from `stdin` (default `25`, terminating immediately on short streams without blocking).
2. **Synthesize & Verify**: Synthesizes a deterministic compiled `Regex`, column/predicate `AwkFilter`, or JSON `JqFilter` from the sample rows + your query (via an LLM endpoint or built-in offline heuristic analyzer) and validates it against the sample buffer.
3. **Uncork & Stream at Native Speed**: Replays the buffered sample rows in exact order and streams the remaining millions of rows through a zero-allocation `read_line` loop with a 128 KB `BufWriter`—handling downstream `SIGPIPE` / broken pipes (`head -n 5`) cleanly with exit code `0`.

```text
  stdin (millions of rows)
         |
         +--> [1] Sample first ~25 rows --> Synthesize & verify deterministic filter
         |                                                 |
         v                                                 v
  [2] Uncork sample + remainder ----------> Zero-Allocation Native Rust Streamer
                                                           |
                                                           v
                                                    stdout (Unix pipe)
```

## Installation

```bash
git clone https://github.com/abhinavm-web/jitpipe.git
cd jitpipe
cargo install --path .
```

Installing the crate builds four binaries:
- `sgrep` — Semantic line filter (compiles to native `regex::Regex`)
- `sawk` — Semantic column extractor & row predicate engine
- `sjq` — Semantic JSONL field projector, dot-path extractor, and predicate filter
- `jitpipe` — Unified multi-tool dispatcher (`jitpipe grep`, `jitpipe awk`, `jitpipe jq`)

## Usage

### `sgrep` — Semantic Grep

```bash
# Filter logs using natural-language intent or regex patterns
cat access.log | sgrep "5xx server errors"

# Standard grep flags: -i (ignore case), -v (invert), -n (line numbers), -c (count), -m (max count)
cat app.log | sgrep -n -m 100 "timeout or connection reset"

# Inspect the synthesized regex and throughput metrics on stderr
cat access.log | sgrep --explain "failed auth requests"
```

### `sawk` — Semantic Awk

```bash
# Extract columns by template or field expression
cat metrics.log | sawk '{print $1, $NF}'

# Automatic or explicit delimiter handling (-F) with row predicates
cat latency.csv | sawk -F, '$3 > 500 {print $1, $3}'
```

### `sjq` — Semantic Jq for JSONL Streams

```bash
# Project specific fields into compact JSON objects
cat events.jsonl | sjq '{user_id, latency_ms}'

# Filter JSONL rows by predicate and extract raw unquoted strings (-r)
cat events.jsonl | sjq -r '.service where latency_ms >= 250'

# Unroll arrays and skip malformed non-JSON log lines cleanly
cat mixed.jsonl | sjq --ignore-malformed '.items[]'
```

### Caching Compiled Filters (`--save-filter` / `--load-filter`)

Synthesize once, then run in CI or production scripts with zero synthesis latency:

```bash
# Save the synthesized filter definition to disk
cat sample.log | sgrep --save-filter .jitpipe/errors.json "database deadlock or timeout"

# Reuse the compiled filter directly without sampling or synthesis
cat prod-10gb.log | sgrep --load-filter .jitpipe/errors.json
```

## Environment Variables (Optional LLM Synthesis)

By default, `jitpipe` uses its fast offline heuristic synthesizer when no API key is set, or calls an OpenAI/Anthropic-compatible endpoint when configured:

- `OPENAI_API_KEY` / `ANTHROPIC_API_KEY`: Enables LLM-backed filter synthesis on the 25-row sample.
- `JITPIPE_LLM_URL` / `JITPIPE_MODEL`: Override the synthesis API endpoint or model (e.g., local Ollama `http://127.0.0.1:11434/v1/chat/completions`).
- `--heuristic`: Force 100% offline heuristic synthesis even when API keys are present.

## Architecture

```text
src/
├── lib.rs              # Top-level pipeline runners (run_sgrep, run_sawk, run_sjq)
├── main.rs             # Unified `jitpipe` binary entry point
├── bin/
│   ├── sgrep.rs        # Standalone `sgrep` binary
│   ├── sawk.rs         # Standalone `sawk` binary
│   └── sjq.rs          # Standalone `sjq` binary
├── cli.rs              # Clap argument definitions for all 4 binaries
├── sampler.rs          # Non-blocking N-row stream sampler & buffer uncorker
├── filter/
│   ├── grep.rs         # Zero-copy Cow<'a, str> regex matcher
│   ├── awk.rs          # Delimiter splitter, $1..$NF template projector & numeric predicates
│   └── jq.rs           # JSONL dot-path, object projection, array unrolling & predicates
├── synth/
│   ├── heuristic.rs    # Offline pattern, delimiter, and schema synthesizer
│   ├── llm.rs          # HTTP LLM synthesizer with sample self-verification & fallback
│   ├── cache.rs        # JSON filter serialization (--save-filter / --load-filter)
│   └── mock.rs         # Deterministic synthesizer for test harnesses
└── engine/
    ├── streamer.rs     # Zero-allocation single-buffer remainder streaming engine
    └── pipe.rs         # Cross-platform SIGPIPE / Windows broken-pipe (109/232) handler
```

## Development & Verification

```bash
cargo test
cargo clippy --all-targets -- -D warnings
```

## License

[MIT](LICENSE)
