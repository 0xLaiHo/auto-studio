#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${DEEPSEEK_API_KEY:-}" ]]; then
  echo "SKIP: DEEPSEEK_API_KEY is not available to this process." >&2
  exit 77
fi

cargo test -p autostudio-provider --test deepseek_live -- --ignored --nocapture
