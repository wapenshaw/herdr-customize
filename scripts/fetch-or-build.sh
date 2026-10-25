#!/bin/sh
# Herdr's [[build]] step on Linux and macOS.
#
# Downloads the release binary for this checkout's version and platform,
# checks it against the release's SHA256SUMS, and installs it where the
# manifest runs it. Anything missing or wrong (no release for this version,
# no network, a checksum mismatch, an unknown platform) falls back to
# building from source with cargo, so installing never gets harder than a
# plain `cargo build`.
#
# HERDR_CUSTOMIZE_BUILD_FROM_SOURCE=1 skips the download.
# HERDR_CUSTOMIZE_RELEASE_BASE points the download somewhere else (tests).

set -u

REPO=wapenshaw/herdr-customize
ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
OUT="$ROOT/target/release/herdr-customize"
BASE=${HERDR_CUSTOMIZE_RELEASE_BASE:-https://github.com/$REPO/releases/download}
tmp=

build() {
	echo "herdr-customize: $1; building from source." >&2
	[ -n "$tmp" ] && rm -rf "$tmp"
	if ! command -v cargo >/dev/null 2>&1; then
		echo "herdr-customize: cargo was not found. Install Rust from https://rustup.rs, then run: herdr plugin install $REPO" >&2
		exit 1
	fi
	cd "$ROOT" || exit 1
	exec cargo build --release --locked
}

fetch() {
	if command -v curl >/dev/null 2>&1; then
		curl -fsSL --retry 2 -o "$2" "$1"
	elif command -v wget >/dev/null 2>&1; then
		wget -q -O "$2" "$1"
	else
		return 1
	fi
}

sha256() {
	if command -v sha256sum >/dev/null 2>&1; then
		sha256sum "$1" | cut -d ' ' -f 1
	elif command -v shasum >/dev/null 2>&1; then
		shasum -a 256 "$1" | cut -d ' ' -f 1
	else
		return 1
	fi
}

[ -n "${HERDR_CUSTOMIZE_BUILD_FROM_SOURCE:-}" ] && build "HERDR_CUSTOMIZE_BUILD_FROM_SOURCE is set"

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$ROOT/Cargo.toml" | head -n 1)
[ -n "$version" ] || build "Cargo.toml has no version"

case "$(uname -s)" in
Linux) os=unknown-linux-musl ;;
Darwin) os=apple-darwin ;;
*) build "no release binary for $(uname -s)" ;;
esac
case "$(uname -m)" in
x86_64 | amd64) arch=x86_64 ;;
aarch64 | arm64) arch=aarch64 ;;
*) build "no release binary for $(uname -m)" ;;
esac
asset="herdr-customize-$arch-$os"

tmp=$(mktemp -d) || build "cannot create a temporary directory"
url="$BASE/v$version"
fetch "$url/$asset" "$tmp/bin" || build "no $asset for v$version"
fetch "$url/SHA256SUMS" "$tmp/sums" || build "no checksums for v$version"
want=$(awk -v a="$asset" '$2 == a || $2 == "*" a { print $1 }' "$tmp/sums")
got=$(sha256 "$tmp/bin") || build "no sha256sum or shasum to check the download"
[ -n "$want" ] && [ "$want" = "$got" ] || build "checksum mismatch for $asset"
mkdir -p "$(dirname "$OUT")" && chmod 755 "$tmp/bin" && mv -f "$tmp/bin" "$OUT" || build "cannot install $OUT"
rm -rf "$tmp"
echo "herdr-customize: installed the v$version release binary ($asset)."
