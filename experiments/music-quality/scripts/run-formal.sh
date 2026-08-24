#!/usr/bin/env bash
set -euo pipefail

experiment_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="${experiment_root}/target/release/autostudio-music-quality"
output_root="${1:-${experiment_root}/evidence/formal}"
parallelism="${Q0_PARALLELISM:-3}"

if [[ -z "${DEEPSEEK_API_KEY:-}" ]]; then
  echo "DEEPSEEK_API_KEY is required" >&2
  exit 2
fi

if [[ ! -x "${binary}" ]]; then
  cargo build --release --manifest-path "${experiment_root}/Cargo.toml"
fi

mode_a=(
  l1-song-hook
  l2-electronic-groove
  l3-video-cue
  l4-song-neon
)

mode_b=(
  l1-song-hook
  l1-video-motif
  l2-electronic-groove
  l2-orchestral-ostinato
  l3-verse-chorus
  l3-video-cue
  l4-song-neon
  l4-song-intimate
  l4-video-chase
  l4-video-emotional
  l4-orchestral-argument
  l4-electronic-microcity
)

run_one() {
  local mode="$1"
  local brief_id="$2"
  local output_dir="${output_root}/mode-${mode}/${brief_id}"
  local log_file="${output_dir}/runner.log"
  mkdir -p "${output_dir}"

  if [[ -f "${output_dir}/run.json" ]]; then
    echo "skip completed artifact: mode=${mode} brief=${brief_id}"
    return 0
  fi

  if [[ "${mode}" == "b" && -f "${output_dir}/turn-01.json" && -f "${output_dir}/turn-02.json" ]]; then
    "${binary}" resume-b \
      --brief-id "${brief_id}" \
      --output-dir "${output_dir}" >"${log_file}" 2>&1
  else
    "${binary}" run \
      --mode "${mode}" \
      --brief-id "${brief_id}" \
      --output-dir "${output_dir}" >"${log_file}" 2>&1
  fi
  echo "finished: mode=${mode} brief=${brief_id}"
}

export experiment_root binary output_root
export -f run_one

mkdir -p "${output_root}"
printf 'a %s\n' "${mode_a[@]}" >"${output_root}/formal-jobs.txt"
printf 'b %s\n' "${mode_b[@]}" >>"${output_root}/formal-jobs.txt"

xargs -r -n 2 -P "${parallelism}" bash -c 'run_one "$1" "$2"' _ \
  <"${output_root}/formal-jobs.txt"

echo "formal A/B generation completed: ${output_root}"
