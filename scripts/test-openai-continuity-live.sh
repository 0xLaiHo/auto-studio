#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${OPENAI_API_KEY:-}" ]]; then
  echo "SKIP: OPENAI_API_KEY is not available to this process." >&2
  exit 77
fi

# Keep this qualification pinned to the low-cost model. The Planning harness
# already caps the Run at eight Provider turns and 4,096 output tokens per turn.
export OPENAI_LIVE_MODEL="gpt-5-mini"

cargo test -p autostudio-provider --test openai_continuity_live -- --ignored --nocapture
