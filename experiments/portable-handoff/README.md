# Auto Studio Portable Handoff Experiment

This isolated Rust workspace proves one cross-DAW contract without modifying
the protocol-hashed Q0 apparatus: semantic track hints resolve to versioned
instrument profiles, and a Type-1 Standard MIDI file carries track names,
tempo/meter/markers, CC0/CC32 Bank Select and Program Change. A separate
`instrument-assignments.json` records the intended profile, preset, library
hash and local-use decision.

It does not automate a DAW UI, produce a native DAW project, render stems, or
guarantee identical sound when a DAW substitutes its own instrument.

The same workspace now includes a strict DAW qualification harness. It binds
one unchanged handoff package to exact DAW targets, produces a result template,
and verifies evidence before allowing `pass`. A target cannot pass while its
exact version is blocked or unknown.

## Reproduce

```bash
cargo test --manifest-path experiments/portable-handoff/Cargo.toml --all-targets
cargo clippy --manifest-path experiments/portable-handoff/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path experiments/portable-handoff/Cargo.toml --check

cargo run --manifest-path experiments/portable-handoff/Cargo.toml -- \
  compile \
  --input experiments/music-quality/evidence/pilot/l1-song-hook/spec.json \
  --output-dir experiments/portable-handoff/evidence/pilot/l1-song-hook/portable-handoff-v1

cargo run --manifest-path experiments/portable-handoff/Cargo.toml -- \
  prepare-matrix \
  --handoff-dir experiments/portable-handoff/evidence/pilot/l1-song-hook/portable-handoff-v1 \
  --targets experiments/portable-handoff/environment/daw-qualification-targets-v1.json \
  --output-dir experiments/portable-handoff/evidence/pilot/l1-song-hook/daw-qualification-v1

cargo run --manifest-path experiments/portable-handoff/Cargo.toml -- \
  verify-matrix \
  --handoff-dir experiments/portable-handoff/evidence/pilot/l1-song-hook/portable-handoff-v1 \
  --plan experiments/portable-handoff/evidence/pilot/l1-song-hook/daw-qualification-v1/qualification-plan.json \
  --results experiments/portable-handoff/evidence/pilot/l1-song-hook/daw-qualification-v1/qualification-results.json \
  --evidence-root experiments/portable-handoff/evidence/pilot/l1-song-hook/daw-qualification-v1 \
  --output experiments/portable-handoff/evidence/pilot/l1-song-hook/daw-qualification-v1/qualification-summary.json
```

The output contains `spec.json`, `instrument-assignments.json`,
`composition.mid` and an integrity `manifest.json`. GeneralUser GS remains a
local validation asset and is not copied into the output or approved for
product redistribution.

The post-lock Creator observation lives in
`environment/portable-handoff-pilot-v1.json`. Frozen Q0 v2/v3 inputs remain in
`../music-quality/` and must not be rewritten.

## DAW qualification contract

`environment/daw-qualification-targets-v1.json` currently lists Steinberg
Cubase, PreSonus Studio One Pro and Image-Line FL Studio as required but
`blocked`: none is installed on the current qualification host, so no exact
version/platform has been frozen. The generated Pilot summary therefore says
`0 pass / 0 fail / 3 not_run` and `all_required_targets_passed: false`. This is
the intended result, not a skipped test.

For each available DAW:

1. freeze its exact version and platform in the target file and change
   `readiness` to `ready` before regenerating the plan;
2. calculate and record the DAW executable SHA-256 in the result;
3. import the unchanged `composition.mid` without repair and record track,
   tempo/meter, marker and Program Change observations;
4. save/reopen a native project, make one intentional musical edit and export
   the edited MIDI;
5. store a PNG/JPEG screenshot, saved project and edited MIDI under the evidence
   root, then record each relative path, byte count and SHA-256 in
   `qualification-results.json`;
6. run `verify-matrix`. The verifier rejects missing/hash-mismatched evidence,
   unsafe paths, version mismatch, failed required checks, a blocked target, or
   an edited MIDI with unchanged channel events.

A DAW may report Program Change as `honored`, `ignored` or `remapped`; all are
valid observations for Portable Handoff. A marker may be `preserved` or
`not_exposed`, but not `lost`, for a passing result. The remaining check fields
use the explicit values `passed` or `failed`.
