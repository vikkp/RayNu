#!/usr/bin/env bash
# Install the Verus binary frozen in verus-version.toml (ADR-008 / M3.15).
# Downloads only the pinned release tag + verifies sha256 and embedded commit.
# Never uses GitHub "latest" or rolling pre-release channels.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PIN="$ROOT/verus-version.toml"
VERUS_HOME="${VERUS_HOME:-$ROOT/target/verus}"

toml_get() {
  local key="$1"
  sed -n "s/^${key} = \"\\([^\"]*\\)\"/\\1/p" "$PIN" | head -1
}

if [[ ! -f "$PIN" ]]; then
  echo "error: missing $PIN" >&2
  exit 1
fi

version="$(toml_get version)"
tag="$(toml_get tag)"
commit="$(toml_get commit)"
toolchain="$(toml_get toolchain)"
asset="$(toml_get asset_linux)"
sha256_expect="$(toml_get sha256_linux)"

if [[ -z "$version" || "$version" == "unpinned-scaffold" ]]; then
  echo "error: verus-version.toml has no concrete version pin" >&2
  exit 1
fi
if [[ -z "$tag" || -z "$commit" || -z "$toolchain" || -z "$asset" || -z "$sha256_expect" ]]; then
  echo "error: verus-version.toml missing tag/commit/toolchain/asset_linux/sha256_linux" >&2
  exit 1
fi
if [[ "$tag" == *latest* || "$asset" == *latest* ]]; then
  echo "error: pin must not reference 'latest' (use an exact release tag)" >&2
  exit 1
fi
if [[ "$tag" == *rolling* ]]; then
  echo "error: pin must not use rolling pre-release tags (use a weekly point release)" >&2
  exit 1
fi

url="https://github.com/verus-lang/verus/releases/download/${tag}/${asset}"
stamp="$VERUS_HOME/.raynu-verus-pin"
# stamp records version|commit|sha256 so a pin bump forces reinstall
stamp_val="${version}|${commit}|${sha256_expect}"

echo "==> Frozen Verus pin"
echo "    version:   $version"
echo "    tag:       $tag"
echo "    commit:    $commit"
echo "    toolchain: $toolchain"
echo "    asset:     $asset"
echo "    sha256:    $sha256_expect"

if [[ -x "$VERUS_HOME/verus" && -f "$stamp" && "$(cat "$stamp")" == "$stamp_val" ]]; then
  echo "==> Verus $version already installed at $VERUS_HOME (pin stamp match)"
else
  echo "==> Installing Verus $version → $VERUS_HOME"
  rm -rf "$VERUS_HOME"
  mkdir -p "$VERUS_HOME"
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  curl -fsSL -o "$tmp/$asset" "$url"

  echo "==> Verifying sha256 of $asset"
  got="$(sha256sum "$tmp/$asset" | awk '{print $1}')"
  if [[ "$got" != "$sha256_expect" ]]; then
    echo "error: sha256 mismatch for $asset" >&2
    echo "  expected: $sha256_expect" >&2
    echo "  got:      $got" >&2
    exit 1
  fi

  unzip -q "$tmp/$asset" -d "$tmp"
  src="$(find "$tmp" -maxdepth 2 -type f -name verus | head -1)"
  if [[ -z "$src" ]]; then
    echo "error: verus binary missing from $asset" >&2
    exit 1
  fi
  src_dir="$(dirname "$src")"
  cp -a "$src_dir"/. "$VERUS_HOME/"
  echo "$stamp_val" >"$stamp"
fi

# Verify the installed binary's version.json matches the frozen pin.
vj="$VERUS_HOME/version.json"
if [[ ! -f "$vj" ]]; then
  echo "error: missing $vj after install" >&2
  exit 1
fi
vj_version="$(sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$vj" | head -1)"
vj_commit="$(sed -n 's/.*"commit"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$vj" | head -1)"
vj_toolchain="$(sed -n 's/.*"toolchain"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$vj" | head -1)"
if [[ "$vj_version" != "$version" || "$vj_commit" != "$commit" || "$vj_toolchain" != "$toolchain" ]]; then
  echo "error: installed version.json does not match verus-version.toml pin" >&2
  echo "  pin:  version=$version commit=$commit toolchain=$toolchain" >&2
  echo "  got:  version=$vj_version commit=$vj_commit toolchain=$vj_toolchain" >&2
  exit 1
fi

if ! command -v rustup >/dev/null 2>&1; then
  echo "error: rustup required to install Verus toolchain $toolchain" >&2
  exit 1
fi

echo "==> rustup install $toolchain (no-op if present)"
rustup install "$toolchain"

echo "==> Verus home: $VERUS_HOME"
echo "==> Add to PATH: export PATH=\"$VERUS_HOME:\$PATH\""
