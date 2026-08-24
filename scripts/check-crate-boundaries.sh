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

printf 'Ship 0 crate boundaries verified\n'
