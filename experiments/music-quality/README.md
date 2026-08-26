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

The post-lock cross-DAW precursor is intentionally isolated in
[`../portable-handoff/`](../portable-handoff/README.md); it depends on this
library but does not modify the protocol-hashed Q0 source or input files.
The same isolated crate now generates and verifies a local-only six-sample
Q0-Content review package from the frozen v3 corpus. That package is the next
Creator workflow; the cross-DAW matrix is deferred to M5/Gate E.

The real Provider adapter reads only `DEEPSEEK_API_KEY`; it records normalized
visible output, usage and latency, but never the key or Provider private
reasoning. Mode B persists every turn before the next call and can resume its
third turn after interruption.

## Evidence boundaries

- `evidence/pilot/` is excluded from formal scoring.
- `evidence/formal/mode-a/` and `mode-b/` contain machine-verifiable outputs.
- `evidence/formal-v3-l4/` is reserved for the six-pair L4 rebaseline. Every
  run is bound to `protocol-v3-l4.lock.json`; v2 evidence is never overwritten.
  The formal v3 result is 6/6 valid and compiled; human Mode C/Keep/editing is
  still pending.
- Q0 v3 permits at most one additional Mode B turn, and only when turn 3 is
  strict JSON whose sole failures are the frozen global note/CC budgets. The
  full before/after turns remain evidence; no event is silently clipped.
- Mode C needs actual Creator feedback and cannot be synthesized by the runner.
- The generated `../portable-handoff/evidence/content-review/q0-content-v1/`
  contains six fixed 48 kHz stereo previews, Portable MIDI, protocol bindings,
  immutable hashes and mutable `feedback.json`. It is ignored by Git because
  the WAVs are local evaluation evidence. See the portable-handoff README for
  reproducible prepare/verify commands.
- Blind Keep scoring and continued editing are human evidence. A compiled MIDI
  file or a model self-score cannot substitute for them.
- GeneralUser GS is referenced as a local evaluation asset only and is not
  copied into this repository or approved for product redistribution.
- `../portable-handoff/evidence/pilot/l1-song-hook/portable-handoff-v1/` is a
  machine-verified precursor, not a production DAW
  exporter. It contains Type-1 MIDI plus `instrument-assignments.json`; each
  musical track starts with CC0/CC32 Bank Select and Program Change. This is
  portable musical intent for Cubase, Studio One, FL Studio and other SMF
  importers, but a DAW may ignore Program Change or map it to a different local
  instrument.
- `../portable-handoff/evidence/pilot/l1-song-hook/daw-qualification-v1/`
  binds that unchanged package to the required Cubase, Studio One Pro and FL
  Studio matrix. The verifier is implemented, but its human execution is
  deferred to M5/Gate E. The current honest result remains
  `0 pass / 0 fail / 3 not_run` because those DAWs are not installed and exact
  versions are not frozen on this host; this does not block `CONTENT-GO`.
- Exact sound across DAWs requires WAV/stems or the same qualified
  sampler/plugin/content state. Those paths are not implemented by Q0.
- v2/v3 protocol-hashed `schema/*-v1`, `daw-environment-v1.json` and
  `instrument-mapping-v1.json` remain unchanged. The post-lock extension uses
  `../portable-handoff/environment/instrument-catalog-portable-v1.json` and
  `../portable-handoff/environment/portable-handoff-pilot-v1.json`; do not
  rewrite the frozen files to record new observations.

The non-automatable steps are defined in [`HUMAN-GATES.md`](HUMAN-GATES.md).
