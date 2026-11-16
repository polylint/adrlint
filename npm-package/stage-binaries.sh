#!/usr/bin/env bash
# Stage the published release binaries into the eight npm platform packages.
#
#   ./stage-binaries.sh --artifacts <dir>   # the release archives, as published
#
# <dir> holds the archives exactly as they are attached to the GitHub release
# (adrlint-<triple>.tar.gz / .zip) plus adrlint_<version>_checksums.txt. Every
# archive is verified against that manifest before it is unpacked, so the bytes
# shipped on npm are the bytes the release checksums describe. Re-building a
# binary here instead would ship something no checksum covers.
set -euo pipefail

PACKAGE_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$PACKAGE_DIR/.." && pwd)"
INVOCATION_DIR="$PWD"

VERSION="$(node -p "require('$PACKAGE_DIR/package.json').version")"

# platform-package-directory:release-triple
TARGETS=(
  "darwin-arm64:aarch64-apple-darwin"
  "darwin-x64:x86_64-apple-darwin"
  "linux-arm64-gnu:aarch64-unknown-linux-gnu"
  "linux-arm64-musl:aarch64-unknown-linux-musl"
  "linux-x64-gnu:x86_64-unknown-linux-gnu"
  "linux-x64-musl:x86_64-unknown-linux-musl"
  "win32-arm64:aarch64-pc-windows-msvc"
  "win32-x64:x86_64-pc-windows-msvc"
)

usage() {
  echo "usage: $0 --artifacts <dir>" >&2
  exit 2
}

[[ "${1:-}" == "--artifacts" && -n "${2:-}" ]] || usage
ARTIFACTS="$2"
# Resolve against the directory the script was invoked from, before anything changes directory.
[[ "$ARTIFACTS" == /* ]] || ARTIFACTS="$INVOCATION_DIR/$ARTIFACTS"
[[ -d "$ARTIFACTS" ]] || {
  echo "error: not a directory: $ARTIFACTS" >&2
  exit 1
}
ARTIFACTS="$(cd "$ARTIFACTS" && pwd)"

CHECKSUMS="$ARTIFACTS/adrlint_${VERSION}_checksums.txt"
[[ -f "$CHECKSUMS" ]] || {
  echo "error: missing checksum manifest $CHECKSUMS" >&2
  exit 1
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

# Every package directory must be named in TARGETS and vice versa, and the umbrella must depend on
# each one at exactly this version; otherwise a platform would be published that nothing installs,
# or installed at a version that was never published.
check_inventory() {
  local listed on_disk deps expected_deps target
  listed="$(printf '%s\n' "${TARGETS[@]%%:*}" | sort)"
  expected_deps="$(for target in "${TARGETS[@]}"; do echo "@polylint/adrlint-${target%%:*}@$VERSION"; done | sort)"
  on_disk="$(find "$PACKAGE_DIR/platforms" -mindepth 1 -maxdepth 1 -type d -exec basename {} \; | sort)"
  [[ "$listed" == "$on_disk" ]] || {
    echo "error: platforms/ does not match TARGETS" >&2
    return 1
  }
  deps="$(node -p "Object.entries(require('$PACKAGE_DIR/package.json').optionalDependencies || {}) \
    .map(([name, version]) => name + '@' + version).sort().join('\n')")"
  [[ "$deps" == "$expected_deps" ]] || {
    echo "error: optionalDependencies do not pin every platform package at $VERSION" >&2
    return 1
  }
}

stage_one() {
  local name="$1" triple="$2"
  local dir="$PACKAGE_DIR/platforms/$name"
  local dest="$dir/bin"
  local archive member expected actual pkg_name pkg_version

  pkg_name="$(node -p "require('$dir/package.json').name")"
  pkg_version="$(node -p "require('$dir/package.json').version")"
  [[ "$pkg_name" == "@polylint/adrlint-$name" && "$pkg_version" == "$VERSION" ]] || {
    echo "error: $dir/package.json is $pkg_name@$pkg_version, expected @polylint/adrlint-$name@$VERSION" >&2
    return 1
  }

  if [[ "$triple" == *windows* ]]; then
    archive="adrlint-$triple.zip"
    member="adrlint.exe"
  else
    archive="adrlint-$triple.tar.gz"
    member="adrlint"
  fi
  [[ -f "$ARTIFACTS/$archive" ]] || {
    echo "error: missing archive $ARTIFACTS/$archive" >&2
    return 1
  }

  expected="$(awk -v name="$archive" '{ file = $2; sub(/^\*/, "", file); sub(/^\.\//, "", file) } file == name { print $1 }' "$CHECKSUMS")"
  [[ -n "$expected" ]] || {
    echo "error: $CHECKSUMS lists no checksum for $archive" >&2
    return 1
  }
  actual="$(sha256_of "$ARTIFACTS/$archive")"
  [[ "$actual" == "$expected" ]] || {
    echo "error: checksum mismatch for $archive: expected $expected, got $actual" >&2
    return 1
  }

  rm -rf "$dest"
  mkdir -p "$dest"
  # Extract only the one expected member: nothing else in the archive reaches the package.
  if [[ "$triple" == *windows* ]]; then
    unzip -q -o "$ARTIFACTS/$archive" "$member" -d "$dest"
  else
    tar -xzf "$ARTIFACTS/$archive" -C "$dest" "$member"
  fi
  [[ -f "$dest/$member" && ! -L "$dest/$member" ]] || {
    echo "error: $archive contained no regular file $member" >&2
    return 1
  }
  chmod 0755 "$dest/$member"
  cp "$REPO_ROOT/LICENSE" "$dir/LICENSE"
  echo "  ok $name <- $archive"
}

check_inventory
echo "Staging adrlint $VERSION binaries from $ARTIFACTS"
for target in "${TARGETS[@]}"; do
  stage_one "${target%%:*}" "${target##*:}"
done
