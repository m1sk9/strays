#!/bin/sh
# herdr [[build]] step: installs the strays binary at bin/strays.
#
# Downloads the release archive matching this checkout's plugin version and
# verifies its SHA-256, so installing needs no Rust toolchain. Any miss (no
# archive for this platform, network failure, checksum mismatch) falls back to
# building from source rather than failing the install.
set -u

repo="m1sk9/strays"

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
out="$root/bin/strays"

have() { command -v "$1" >/dev/null 2>&1; }

install_binary() {
  # Renamed into place instead of copied over: a running strays pins the old
  # inode, which Linux refuses to write to (ETXTBSY) and which macOS kills on
  # the next launch once its pages no longer match the cached code signature.
  mkdir -p "$root/bin" &&
    cp "$1" "$out.tmp" &&
    chmod +x "$out.tmp" &&
    mv -f "$out.tmp" "$out" && return 0
  rm -f "$out.tmp"
  echo "strays: could not install the binary at $out." >&2
  return 1
}

build_from_source() {
  # herdr may be launched without ~/.cargo/bin on PATH.
  [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
  if ! have cargo; then
    echo "strays: cargo not found; install Rust from https://rustup.rs and reinstall the plugin." >&2
    exit 1
  fi
  (cd "$root" && cargo build --release --locked) || exit 1
  install_binary "$root/target/release/strays" || exit 1
  exit 0
}

fallback() {
  echo "strays: $1; building from source instead." >&2
  [ -n "${tmp:-}" ] && rm -rf "$tmp"
  build_from_source
}

download() {
  if have curl; then
    curl -fsSL --connect-timeout 15 -o "$2" "$1"
  elif have wget; then
    wget -q --timeout=15 -O "$2" "$1"
  else
    return 1
  fi
}

sha256_of() {
  if have sha256sum; then
    sha256sum "$1" | awk '{print $1}'
  elif have shasum; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    return 1
  fi
}

case "$(uname -s)/$(uname -m)" in
  Darwin/arm64) target="aarch64-apple-darwin" ;;
  Linux/x86_64) target="x86_64-unknown-linux-musl" ;;
  Linux/aarch64 | Linux/arm64) target="aarch64-unknown-linux-musl" ;;
  *) fallback "no prebuilt binary for $(uname -s)/$(uname -m)" ;;
esac

# The plugin manifest's version, not Cargo.toml's: release-please bumps both,
# but the manifest is what herdr reports as the installed version.
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$root/herdr-plugin.toml" | head -n 1)
[ -n "$version" ] || fallback "could not read the version from herdr-plugin.toml"

archive="strays-$target.tar.gz"
base="https://github.com/$repo/releases/download/v$version"
tmp=$(mktemp -d) || fallback "could not create a temporary directory"

download "$base/$archive" "$tmp/$archive" || fallback "could not download $archive for v$version"
download "$base/$archive.sha256" "$tmp/$archive.sha256" || fallback "could not download the checksum for $archive"

expected=$(awk '{print $1}' "$tmp/$archive.sha256")
actual=$(sha256_of "$tmp/$archive") || fallback "no sha256sum or shasum to verify $archive"
[ -n "$expected" ] && [ "$expected" = "$actual" ] || fallback "checksum mismatch for $archive"

tar -xzf "$tmp/$archive" -C "$tmp" || fallback "could not extract $archive"
install_binary "$tmp/strays"
status=$?
rm -rf "$tmp"
exit "$status"
