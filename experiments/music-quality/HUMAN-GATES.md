# Q0 Human and DAW Gates

These steps intentionally cannot be automated or replaced with an LLM
self-score. Complete them as the Creator/evaluator after formal Mode A and B
verification succeeds.

## 1. Bitwig import smoke

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

## 2. Real Creator feedback for Mode C

For each of the six frozen L4 Briefs, inspect its Mode B result and write one or
two concrete production intentions. Do not ask another LLM to invent the
feedback. Run:

```bash
experiments/music-quality/target/release/autostudio-music-quality run \
  --mode c \
  --brief-id <l4-brief-id> \
  --base-spec experiments/music-quality/evidence/formal/mode-b/<l4-brief-id>/spec.json \
  --feedback '<creator feedback 1>' \
  --feedback '<optional creator feedback 2>' \
  --output-dir experiments/music-quality/evidence/formal/mode-c/<l4-brief-id>
```

The command rejects empty feedback, more than two feedback rounds and invalid
base specs.

## 3. Freeze the anonymous evaluator package

After Mode C completes, run:

```bash
experiments/music-quality/target/release/autostudio-music-quality prepare-blind \
  --evidence-root experiments/music-quality/evidence/formal \
  --output-dir experiments/music-quality/evidence/blind
```

Open only `evidence/blind/evaluator/` while scoring. Do not open
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
