#!/usr/bin/env bash
# ADR-021 E12: docs/openapi/v1.yaml lists the same routes as mgmt/openapi.rs,
# and each route is a string in mgmt/.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

yaml="docs/openapi/v1.yaml"
rs="mgmt/openapi.rs"
[[ -f "$yaml" && -f "$rs" ]]

if ! grep -q 'version: "1"' "$yaml"; then
  echo "error: $yaml info.version is not 1" >&2
  exit 1
fi

yaml_paths="$(awk '/^  \// { sub(/^  /,""); sub(/:$/,""); print }' "$yaml" | sort)"
rs_paths="$(awk -F'"' '/^[[:space:]]*"\// { print $2 }' "$rs" | sort)"

if [[ "$yaml_paths" != "$rs_paths" ]]; then
  echo "error: OpenAPI v1 paths and mgmt/openapi.rs differ" >&2
  diff -u <(printf '%s\n' "$rs_paths") <(printf '%s\n' "$yaml_paths") || true
  exit 1
fi

missing=0
while IFS= read -r path; do
  [[ -z "$path" ]] && continue
  if ! grep -R -q -F -- "$path" mgmt --include='*.rs'; then
    echo "error: $path is in the contract and not in mgmt/" >&2
    missing=1
  fi
done <<< "$yaml_paths"

if [[ "$missing" -ne 0 ]]; then
  exit 1
fi

echo "RAYNU-V-E12-OPENAPI-OK"
