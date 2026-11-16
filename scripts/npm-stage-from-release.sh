#!/usr/bin/env bash
# Pack the npm packages for an existing GitHub release, without publishing anything.
#
#   scripts/npm-stage-from-release.sh <version> <outdir>
#
# Downloads the release archives and adrlint_<version>_checksums.txt from polylint/adrlint, stages
# them into a scratch copy of npm-package/ (stage-binaries.sh verifies every archive against the
# checksums before unpacking), and runs `npm pack` for the eight platform packages and the umbrella
# into <outdir>. The working tree is not modified.
#
# This exists for the first publish of a new platform package: npm trusted publishing can only be
# configured on a package that already exists, so each @polylint/adrlint-<platform> is published
# once by hand from these tarballs, after which publish.yaml takes over.
#
# <version> must match npm-package/package.json, so the packed manifests are the ones in the repo.
set -euo pipefail

usage() {
  echo "usage: $0 <version> <outdir>" >&2
  exit 2
}

[[ $# -eq 2 ]] || usage
VERSION="${1#v}"
OUTDIR="$2"

[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] || {
  echo "error: not a release version: $1" >&2
  exit 1
}

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPOSITORY="polylint/adrlint"
BASE_URL="https://github.com/$REPOSITORY/releases/download/v$VERSION"

manifest_version="$(node -p "require('$REPO_ROOT/npm-package/package.json').version")"
[[ "$manifest_version" == "$VERSION" ]] || {
  echo "error: npm-package/package.json is $manifest_version, not $VERSION; check out the v$VERSION tag" >&2
  exit 1
}

mkdir -p "$OUTDIR"
OUTDIR="$(cd "$OUTDIR" && pwd)"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/adrlint-npm-stage.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

ARCHIVES=(
  adrlint-aarch64-apple-darwin.tar.gz
  adrlint-x86_64-apple-darwin.tar.gz
  adrlint-aarch64-unknown-linux-gnu.tar.gz
  adrlint-aarch64-unknown-linux-musl.tar.gz
  adrlint-x86_64-unknown-linux-gnu.tar.gz
  adrlint-x86_64-unknown-linux-musl.tar.gz
  adrlint-aarch64-pc-windows-msvc.zip
  adrlint-x86_64-pc-windows-msvc.zip
)

mkdir -p "$WORK/artifacts"
echo "Downloading adrlint v$VERSION release assets from $REPOSITORY"
for asset in "adrlint_${VERSION}_checksums.txt" "${ARCHIVES[@]}"; do
  curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 \
    --output "$WORK/artifacts/$asset" "$BASE_URL/$asset"
  echo "  got $asset"
done

# A scratch copy, so staged binaries and LICENSE copies never land in the working tree.
mkdir -p "$WORK/repo"
cp "$REPO_ROOT/LICENSE" "$WORK/repo/LICENSE"
cp -R "$REPO_ROOT/npm-package" "$WORK/repo/npm-package"
rm -rf "$WORK/repo/npm-package/node_modules" "$WORK/repo/npm-package/platforms"/*/bin \
  "$WORK/repo/npm-package/bin/adrlint-bin" "$WORK/repo/npm-package/bin/adrlint.exe"

"$WORK/repo/npm-package/stage-binaries.sh" --artifacts "$WORK/artifacts"

echo "Packing into $OUTDIR"
for dir in "$WORK/repo/npm-package/platforms"/*/ "$WORK/repo/npm-package/"; do
  (cd "$dir" && npm pack --silent --pack-destination "$OUTDIR" >/dev/null)
done

ls -l "$OUTDIR"
echo
echo "Nothing was published. Publish the platform tarballs before the umbrella, for example:"
echo "  npm publish $OUTDIR/polylint-adrlint-darwin-arm64-$VERSION.tgz --access public"
