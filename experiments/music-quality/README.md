# Auto Studio Q0 Music Quality Experiment

This is an isolated, non-production Rust workspace for answering one question:
can the frozen LLM setup create editable musical decisions worth keeping? It
does not implement Auto Studio's production Agent Harness or Audio Engine.

## Reproducible commands

```bash
cargo test --manifest-path experiments/music-quality/Cargo.toml
cargo clippy --manifest-path experiments/music-quality/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path experiments/music-quality/Cargo.toml --check

DEEPSEEK_API_KEY=... experiments/music-quality/scripts/run-formal.sh

# v2 结果保持只读；使用独立目录重跑全部 6 个 L4 Mode B
DEEPSEEK_API_KEY=... experiments/music-quality/scripts/run-l4-rebaseline-v3.sh
```

The real Provider adapter reads only `DEEPSEEK_API_KEY`; it records normalized
visible output, usage and latency, but never the key or Provider private
reasoning. Mode B persists every turn before the next call and can resume its
third turn after interruption.

## Evidence boundaries

- `evidence/pilot/` is excluded from formal scoring.
- `evidence/formal/mode-a/` and `mode-b/` contain machine-verifiable outputs.
- `evidence/formal-v3-l4/` is reserved for the six-pair L4 rebaseline. Every
  run is bound to `protocol-v3-l4.lock.json`; v2 evidence is never overwritten.
- Q0 v3 permits at most one additional Mode B turn, and only when turn 3 is
  strict JSON whose sole failures are the frozen global note/CC budgets. The
  full before/after turns remain evidence; no event is silently clipped.
- Mode C needs actual Creator feedback and cannot be synthesized by the runner.
- Blind Keep scoring and continued editing are human evidence. A compiled MIDI
  file or a model self-score cannot substitute for them.
- GeneralUser GS is referenced as a local evaluation asset only and is not
  copied into this repository or approved for product redistribution.

The non-automatable steps are defined in [`HUMAN-GATES.md`](HUMAN-GATES.md).
