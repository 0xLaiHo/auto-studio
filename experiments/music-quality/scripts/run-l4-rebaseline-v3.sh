#!/usr/bin/env bash
set -euo pipefail

experiment_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="${experiment_root}/target/release/autostudio-music-quality"
protocol_lock="${experiment_root}/protocol-v3-l4.lock.json"
output_root="${1:-${experiment_root}/evidence/formal-v3-l4}"
parallelism="${Q0_PARALLELISM:-1}"

if [[ -z "${DEEPSEEK_API_KEY:-}" ]]; then
  echo "DEEPSEEK_API_KEY is required" >&2
  exit 2
fi
if [[ "${DEEPSEEK_MODEL:-deepseek-v4-pro}" != "deepseek-v4-pro" ]]; then
  echo "Q0 v3 requires DEEPSEEK_MODEL=deepseek-v4-pro" >&2
  exit 2
fi
if [[ "${DEEPSEEK_THINKING_LEVEL:-high}" != "high" ]]; then
  echo "Q0 v3 requires DEEPSEEK_THINKING_LEVEL=high" >&2
  exit 2
fi
if [[ "${DEEPSEEK_BASE_URL:-https://api.deepseek.com}" != "https://api.deepseek.com" ]]; then
  echo "Q0 v3 requires DEEPSEEK_BASE_URL=https://api.deepseek.com" >&2
  exit 2
fi
if [[ ! "${parallelism}" =~ ^[1-3]$ ]]; then
  echo "Q0_PARALLELISM must be an integer in 1..=3" >&2
  exit 2
fi
if [[ ! -f "${protocol_lock}" ]]; then
  echo "missing frozen protocol: ${protocol_lock}" >&2
  exit 2
fi

if [[ ! -x "${binary}" ]]; then
  cargo build --release --manifest-path "${experiment_root}/Cargo.toml"
fi

l4_briefs=(
  l4-song-neon
  l4-song-intimate
  l4-video-chase
  l4-video-emotional
  l4-orchestral-argument
  l4-electronic-microcity
)

run_one() {
  local brief_id="$1"
  local output_dir="${output_root}/mode-b/${brief_id}"
  local log_file="${output_dir}/runner.log"
  mkdir -p "${output_dir}"

  if [[ -f "${output_dir}/run.json" ]]; then
    echo "skip immutable run: mode=b brief=${brief_id}"
    return 0
  fi

  if [[ -f "${output_dir}/turn-01.json" ]]; then
    "${binary}" resume-b \
      --brief-id "${brief_id}" \
      --output-dir "${output_dir}" \
      --protocol-lock "${protocol_lock}" >"${log_file}" 2>&1
  else
    "${binary}" run \
      --mode b \
      --brief-id "${brief_id}" \
      --output-dir "${output_dir}" \
      --protocol-lock "${protocol_lock}" >"${log_file}" 2>&1
  fi
  echo "finished: mode=b brief=${brief_id}"
}

export binary output_root protocol_lock
export -f run_one

mkdir -p "${output_root}"
printf 'b %s\n' "${l4_briefs[@]}" >"${output_root}/formal-jobs.txt"

xargs -r -n 1 -P "${parallelism}" bash -c 'run_one "$1"' _ \
  < <(printf '%s\n' "${l4_briefs[@]}")

"${binary}" verify-formal \
  --evidence-root "${output_root}" \
  --output "${output_root}/formal-summary.json" \
  --protocol-lock "${protocol_lock}"

echo "Q0 v3 L4 rebaseline verified: ${output_root}"
