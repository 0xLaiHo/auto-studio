# Q0 Human and DAW Gates

These steps intentionally cannot be automated or replaced with an LLM
self-score. Complete them as the Creator/evaluator after formal Mode A and B
verification succeeds.

The v2 11/12 device Gate remains immutable, but its invalid
`l4-orchestral-argument` run has no legal base spec. Before requesting Creator
feedback, `evidence/formal-v3-l4/formal-summary.json` must therefore show an
exact, protocol-bound 6/6 valid and compiled L4 Mode B rebaseline. This
prerequisite is now `PASS`; the remaining sections still require a human.

## 1. Pilot DAW import smoke

Use the frozen recipe in `environment/daw-environment-v1.json` and the pilot
MIDI at `evidence/pilot/l1-song-hook/composition.mid`.

Record all of the following before changing `LIVE-PENDING` to `PASS`:

- Bitwig Studio 6.0.11 imports three semantic MIDI tracks without repairing the
  file;
- tempo, 4/4 meter and section marker information are present where Bitwig
  exposes them;
- the fixed GeneralUser GS mapping can be assigned without changing notes;
- the project saves, closes and reopens;
- the saved `.bwproject` hash and one screenshot are stored with the pilot.

Observed on 2026-08-25: the Creator imported the three tracks, manually loaded
GeneralUser GS presets for Piano/Lead/Bass, and saved/reopened
`arrangement-smoke/c-a2358748b226/c-a2358748b226.bwproject` (SHA-256
`582e93c5653ee69dc9d1324ad4f9af43a8418cf9a088f6f85d44931937e9694f`).
The post-lock observation is recorded separately in
`../portable-handoff/environment/portable-handoff-pilot-v1.json`; the protocol-hashed
`daw-environment-v1.json` remains byte-for-byte unchanged.
This is useful manual Pilot evidence, but the formal Gate remains
`LIVE-PENDING` until its screenshot and checklist are stored with the evidence
and the edited-MIDI requirement in section 5 is completed. It does not prove a
Bitwig-specific integration; none is planned.

The independent `../portable-handoff/` machine slice uses
`environment/instrument-catalog-portable-v1.json` and writes the same Pilot
intent as Type-1 MIDI with per-track CC0/CC32 Bank Select, Program Change and an
`instrument-assignments.json` manifest. Import it unchanged into every DAW in
the future qualification matrix. Record whether each DAW preserves tempo,
markers, tracks and events, and whether it honors or ignores the GM program.
Do not manually repair the MIDI before recording the result.

The qualification plan and current honest result live in
`../portable-handoff/evidence/pilot/l1-song-hook/daw-qualification-v1/`.
Cubase, Studio One Pro and FL Studio are all `not_run` because they are not
installed on the current host and no exact version has been frozen. To execute
one target, first update the isolated
`../portable-handoff/environment/daw-qualification-targets-v1.json`, regenerate
the plan, perform the import/save/edit/export steps, fill the generated result
with relative evidence paths and hashes, then run:

```bash
cargo run --manifest-path experiments/portable-handoff/Cargo.toml -- \
  verify-matrix \
  --handoff-dir experiments/portable-handoff/evidence/pilot/l1-song-hook/portable-handoff-v1 \
  --plan experiments/portable-handoff/evidence/pilot/l1-song-hook/daw-qualification-v1/qualification-plan.json \
  --results experiments/portable-handoff/evidence/pilot/l1-song-hook/daw-qualification-v1/qualification-results.json \
  --evidence-root experiments/portable-handoff/evidence/pilot/l1-song-hook/daw-qualification-v1 \
  --output experiments/portable-handoff/evidence/pilot/l1-song-hook/daw-qualification-v1/qualification-summary.json
```

The command validates evidence integrity; it does not drive the DAW UI or
replace the Creator's checklist.

## 2. Real Creator feedback for Mode C

For each of the six frozen L4 Briefs, inspect its Mode B result and write one or
two concrete production intentions. Do not ask another LLM to invent the
feedback. Run:

```bash
experiments/music-quality/target/release/autostudio-music-quality run \
  --mode c \
  --brief-id <l4-brief-id> \
  --base-spec experiments/music-quality/evidence/formal-v3-l4/mode-b/<l4-brief-id>/spec.json \
  --feedback '<creator feedback 1>' \
  --feedback '<optional creator feedback 2>' \
  --output-dir experiments/music-quality/evidence/formal-v3-l4/mode-c/<l4-brief-id> \
  --protocol-lock experiments/music-quality/protocol-v3-l4.lock.json
```

The command rejects empty feedback, more than two feedback rounds and invalid
base specs.

## 3. Freeze the anonymous evaluator package

After Mode C completes, run:

```bash
experiments/music-quality/target/release/autostudio-music-quality prepare-blind \
  --evidence-root experiments/music-quality/evidence/formal-v3-l4 \
  --output-dir experiments/music-quality/evidence/blind-v3-l4
```

Open only `evidence/blind-v3-l4/evaluator/` while scoring. Do not open
`blind-map.private.json` until every score and editing session is closed.

## 4. Blind Keep and content score

Import each anonymous Candidate using the exact same Bitwig recipe and sound
mapping. Fill `evaluator/evaluation.csv`. A `Keep` means you would preserve the
result as a real creative starting point; “interesting” or “sounds okay” is not
enough.

## 5. Actual continued editing

For each kept L4 Candidate, perform at least one intentional musical edit,
save/reopen the project and export the edited MIDI. Record operation counts,
time and the edited MIDI SHA-256 in the evaluation CSV. Playback-only sessions
do not count.

Only after these records exist may the private map be joined with scores and
the frozen `GO / REVISE / NO-GO / INVALID` thresholds be evaluated. A negative
main-model result also requires the separately credentialed second strong model
specified by the protocol.
