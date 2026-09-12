#!/usr/bin/env bash
# Install the pinned Verus release and verify the ghost model.
# Downloads only the exact release tag in verus-version.toml and checks its
# sha256. Never uses GitHub "latest" or a rolling pre-release channel.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PIN="$ROOT/verus-version.toml"
VERUS_HOME="${VERUS_HOME:-$ROOT/.verus}"

get() { sed -n "s/^$1 = \"\([^\"]*\)\"/\1/p" "$PIN" | head -1; }
tag="$(get tag)"; commit="$(get commit)"; toolchain="$(get toolchain)"
asset="$(get asset_linux)"; want="$(get sha256_linux)"

echo "==> pin: $tag"
echo "    commit:    $commit"
echo "    toolchain: $toolchain"
echo "    sha256:    $want"

case "$tag" in *latest*|*rolling*) echo "error: pin is not an exact release" >&2; exit 1;; esac

if [[ ! -x "$VERUS_HOME/verus" ]]; then
  mkdir -p "$VERUS_HOME"
  url="https://github.com/verus-lang/verus/releases/download/${tag}/${asset}"
  echo "==> downloading $url"
  curl -fsSL -o "$VERUS_HOME/$asset" "$url"
  got="$(sha256sum "$VERUS_HOME/$asset" | cut -d' ' -f1)"
  if [[ "$got" != "$want" ]]; then
    echo "error: sha256 mismatch" >&2
    echo "  expected $want" >&2
    echo "  got      $got" >&2
    exit 1
  fi
  echo "==> sha256 OK"
  unzip -q -o "$VERUS_HOME/$asset" -d "$VERUS_HOME"
  inner="$(find "$VERUS_HOME" -maxdepth 2 -name verus -type f | head -1)"
  [[ -n "$inner" ]] && VERUS_HOME="$(dirname "$inner")"
fi

echo "==> rust toolchain required by this Verus binary: $toolchain"
rustup toolchain install "${toolchain%-x86_64-unknown-linux-gnu}" 2>/dev/null || true

export PATH="$VERUS_HOME:$PATH"
cd "$ROOT"
echo "==> cargo verus verify -p ept_model"
cargo verus verify -p ept_model

echo
echo "Expected last line: 80 verified, 0 errors"
