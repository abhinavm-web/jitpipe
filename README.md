# jitpipe

**JIT-compiled semantic Unix pipes (`sgrep`, `sawk`, `sjq`) — LLM synthesizes a parser on 20 rows, runs 10M rows at native speed.**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](#roadmap--mvp-milestones)
[![Architecture: MVP](https://img.shields.io/badge/Architecture-MVP-orange.svg)](#roadmap--mvp-milestones)

---

## The Problem

Unix gave us `grep`, `awk`, `sed`, and `jq` for deterministic, structured text. Today's data—messy multi-line application logs, semi-structured traces, scraped HTML, CSV dialects, and JSONL dumps—breaks brittle regexes.

Existing AI CLI tools fail at scale in two opposite ways:
1. **Calling an LLM per line/chunk** (`llm`, naive semantic grep): Piping 500,000 log lines through an API takes 20+ minutes and burns \$15+.
2. **Local embeddings (`semtools`)**: Great for similarity ranking, useless for exact structured extraction, numeric predicates (`latency > 800ms`), or JSON projection.

## The Unclaimed Wedge: How It Works

`jitpipe` brings **Just-In-Time (JIT) compiler economics** to semantic Unix pipes (`sgrep`, `sawk`, `sjq`):

1. **Sample (`~25 rows`)**: Buffers only the first small slice of `stdin` (plus diverse outlier lines via reservoir sampling).
2. **Synthesize & Verify**: Sends the 25-row sample + your natural-language query to an LLM (local Ollama/MLX or cloud Flash model) to generate a **deterministic parser/filter** (compiled regex, Rust/WASM, or embedded QuickJS), and immediately validates it against the sample rows.
3. **Stream at Native Speed**: Uncorks the buffer and streams the remaining **10,000,000+ rows** through the synthesized deterministic filter at native compiled speed—zero LLM calls per row, fractions of a cent total, with an optional `--fallback-llm` lane only for lines that violate the learned schema.

```text
  stdin (10M log lines)
         │
         ├──► [1] Sample first ~25 lines ──► LLM synthesizes & tests deterministic filter
         │                                               │
         ▼                                               ▼
  [2] Uncork stream ──────────────────────► Native Compiled Engine (Regex / WASM / QuickJS)
                                                         │
                                            (Optional) Outlier rows ──► LLM fallback lane
                                                         │
                                                         ▼
                                                  stdout (Unix pipe)
```

### Target CLI Experience (`< 60 second aha moment`)

> Note: Below is the target MVP CLI specification currently under construction.

```bash
# Semantic grep: filter messy heterogeneous logs at native speed
cat prod.log | sgrep "requests where auth succeeded but downstream timed out"

# Semantic jq / awk: extract structured JSON from unstructured text streams
cat access.log | sjq "{user_id, service, latency_ms} where latency_ms > 500"

# Inspect the JIT-compiled parser & token cost
cat access.log | sgrep --explain "5xx errors from mobile clients"
# [jitpipe] Sampled 25 lines -> synthesized deterministic filter in 1.1s ($0.0004)
# [jitpipe] Processed 4,820,000 lines in 0.78s (6.1M lines/sec) | Escalated rows: 0
```

## Roadmap & MVP Milestones

- [ ] **Milestone 1 — Stream Sampler & Sandbox Runtime**: Rust CLI reading `stdin` with header/reservoir sampling and embedded deterministic execution (Regex + QuickJS/WASM sandbox).
- [ ] **Milestone 2 — JIT Synthesis & Self-Test Loop**: LLM prompt + automated verification harness ensuring the generated filter parses the sampled rows cleanly without panics.
- [ ] **Milestone 3 — `sgrep`, `sawk`, `sjq` Core Binaries**: Single-binary multi-call CLI supporting line filtering, field projection, JSON emission, and `--explain` / `--save-filter` caching.
- [ ] **Milestone 4 — Hybrid Outlier Escalation**: Schema-drift detector that routes only anomalous rows to a fast local/cloud model while keeping 99.9% of rows on the native fast path.

## License

[MIT](LICENSE)
