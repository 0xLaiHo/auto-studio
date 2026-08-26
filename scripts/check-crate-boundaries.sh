#!/usr/bin/env sh
set -eu

core_tree="$(cargo tree -p autostudio-core --edges normal --prefix none)"
for forbidden in axum tokio rusqlite reqwest hound tauri cpal symphonia; do
  if printf '%s\n' "$core_tree" | grep -Eq "^${forbidden} v"; then
    printf 'autostudio-core must not depend on %s\n' "$forbidden" >&2
    exit 1
  fi
done

for client in autostudio-desktop autostudio-tui; do
  client_tree="$(cargo tree -p "$client" --edges normal --prefix none)"
  for forbidden in autostudio-storage autostudio-provider autostudio-media rusqlite; do
    if printf '%s\n' "$client_tree" | grep -Eq "^${forbidden} v"; then
      printf '%s must use Core API instead of depending on %s\n' "$client" "$forbidden" >&2
      exit 1
    fi
  done
done

member_count="$(cargo metadata --no-deps --format-version 1 | python -c 'import json,sys; print(len(json.load(sys.stdin)["workspace_members"]))')"
if [ "$member_count" -ne 8 ]; then
  printf 'Ship 0 must remain at 5 library crates + 3 application entries; found %s members\n' "$member_count" >&2
  exit 1
fi

cargo metadata --no-deps --format-version 1 | python -c '
import json, sys
metadata = json.load(sys.stdin)
packages = {package["name"]: package for package in metadata["packages"]}
provider = packages["autostudio-provider"]
if provider["features"].get("default") != []:
    raise SystemExit("autostudio-provider default features must remain LLM-only")
core = packages["core-daemon"]
dependency = next(item for item in core["dependencies"] if item["name"] == "autostudio-provider")
if "legacy-generation" in dependency["features"]:
    raise SystemExit("core-daemon must not enable autostudio-provider/legacy-generation")
'

if rg -n \
  'GenerationAdapter|GenerationCoordinator|DeterministicGenerationAdapter|execute_agent_run|reconcile_agent_run|refresh_agent_run' \
  apps/core-daemon/src apps/tui/src apps/desktop/src apps/desktop/src-tauri/src \
  --glob '*.rs' --glob '*.ts' --glob '*.tsx'; then
  printf 'production applications must not expose the legacy Generation runtime\n' >&2
  exit 1
fi

printf 'Ship 0 crate boundaries verified\n'
