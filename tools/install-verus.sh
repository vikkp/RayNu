#!/usr/bin/env bash
# Install the Verus binary pinned in verus-version.toml (ADR-008 / M3.15).
# Default install root: $ROOT/target/verus (gitignored via /target/).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PIN="$ROOT/verus-version.toml"
VERUS_HOME="${VERUS_HOME:-$ROOT/target/verus}"

if [[ ! -f "$PIN" ]]; then
  echo "error: missing $PIN" >&2
  exit 1
fi

version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$PIN" | head -1)"
tag="$(sed -n 's/^tag = "\([^"]*\)"/\1/p' "$PIN" | head -1)"
toolchain="$(sed -n 's/^toolchain = "\([^"]*\)"/\1/p' "$PIN" | head -1)"

if [[ -z "$version" || "$version" == "unpinned-scaffold" ]]; then
  echo "error: verus-version.toml has no concrete version pin" >&2
  exit 1
fi
if [[ -z "$tag" || -z "$toolchain" ]]; then
  echo "error: verus-version.toml missing tag or toolchain" >&2
  exit 1
fi

asset="verus-${version}-x86-linux.zip"
url="https://github.com/verus-lang/verus/releases/download/${tag}/${asset}"
stamp="$VERUS_HOME/.raynu-verus-pin"

if [[ -x "$VERUS_HOME/verus" && -f "$stamp" && "$(cat "$stamp")" == "$version" ]]; then
  echo "==> Verus $version already installed at $VERUS_HOME"
else
  echo "==> Installing Verus $version → $VERUS_HOME"
  rm -rf "$VERUS_HOME"
  mkdir -p "$VERUS_HOME"
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  curl -fsSL -o "$tmp/$asset" "$url"
  unzip -q "$tmp/$asset" -d "$tmp"
  # Release layout: verus-x86-linux/…
  src="$(find "$tmp" -maxdepth 2 -type f -name verus | head -1)"
  if [[ -z "$src" ]]; then
    echo "error: verus binary missing from $asset" >&2
    exit 1
  fi
  src_dir="$(dirname "$src")"
  cp -a "$src_dir"/. "$VERUS_HOME/"
  echo "$version" >"$stamp"
fi

if ! command -v rustup >/dev/null 2>&1; then
  echo "error: rustup required to install Verus toolchain $toolchain" >&2
  exit 1
fi

echo "==> rustup install $toolchain (no-op if present)"
rustup install "$toolchain"

echo "==> Verus home: $VERUS_HOME"
echo "==> Add to PATH: export PATH=\"$VERUS_HOME:\$PATH\""
