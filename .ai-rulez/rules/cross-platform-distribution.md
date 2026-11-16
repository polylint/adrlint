---
type: Decision
title: Cross Platform Distribution
x-ai-rulez:
  kind: rule
  id: cross-platform-distribution
  metadata:
    priority: high
---

## Cross-Platform Distribution

adrlint is part of the [polylint](https://github.com/polylint) organization and ships through the
same channels as its sibling Rust CLIs; all of them must stay consistent and functional.

### Channels

- **crates.io** (`adrlint`): `cargo publish`. Source of truth for the Rust binary.
- **npm** (`@polylint/adrlint`, bin `adrlint`; the unscoped `adrlint` is deprecated): umbrella in
  `npm-package/` with exact-version `optionalDependencies` on eight platform packages
  `@polylint/adrlint-<platform>` (`npm-package/platforms/<platform>/`: `darwin-arm64`, `darwin-x64`,
  `linux-x64-gnu`, `linux-x64-musl`, `linux-arm64-gnu`, `linux-arm64-musl`, `win32-x64`,
  `win32-arm64`), selected by `os`/`cpu`/`libc`. Each ships one binary in `bin/`, staged from the
  release archives by `npm-package/stage-binaries.sh`.
- **PyPI** (`adrlint`): `pip-package/build_wheels.py` builds one wheel per target with the binary
  embedded (`manylinux_2_<glibc>`, `musllinux_1_2`, `macosx_11_0_arm64`, `macosx_10_12_x86_64`,
  `win_amd64`, `win_arm64`) plus a `py3-none-any` fallback wheel with no binary. No sdist.
- **Homebrew**: `Formula/adrlint.rb` in `polylint/homebrew-tap` (`brew install polylint/tap/adrlint`),
  rendered from `scripts/publish/adrlint.rb.tmpl` by `xberg-io/actions/publish-homebrew-source-formulas`
  (config `scripts/publish/homebrew.json`); bottles for `arm64_tahoe`, `x86_64_linux` and
  `arm64_linux` are built and merged into the formula afterwards.
- **Scoop**: `bucket/adrlint.json` in `polylint/scoop-bucket`, rendered from
  `scripts/publish/adrlint.json.tmpl` by `xberg-io/actions/publish-scoop-manifests` (config
  `scripts/publish/scoop.json`). Scoop's own `$version` placeholders are written `$$version`.
- Tap and bucket pushes use a token minted from the org GitHub App (`BOT_APP_ID` /
  `BOT_APP_PRIVATE_KEY`) scoped to that one repository, committing as `polylint-release-helper[bot]`.
- **GitHub Releases**: `.github/workflows/publish.yaml` builds natively per platform and uploads
  archives plus a checksums file.
- **GitHub Action**: root `action.yml`, a composite action that installs and runs `adrlint lint`.
  Install logic lives in `scripts/gh-action-*.sh`; annotations come from `problem-matcher.json`.

### Release workflow

1. Update the version in all four manifests (see version-synchronization).
2. Commit and tag `vX.Y.Z`.
3. Push the tag; the workflow creates a draft release, builds every target, uploads the archives and
   `adrlint_<version>_checksums.txt`, then promotes the draft.
4. npm, PyPI, Homebrew and Scoop publish from the finalized release assets. npm publishes every
   platform package before the umbrella, so the umbrella never pins a version that does not exist.
5. A stable release moves the floating major tag (`v0`) to the new commit.

### Targets

`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-unknown-linux-musl`,
`aarch64-unknown-linux-musl`, `x86_64-apple-darwin`, `aarch64-apple-darwin`,
`x86_64-pc-windows-msvc`, `aarch64-pc-windows-msvc`.

The Windows archives are built natively on `windows-latest` and `windows-11-arm`, not
cross-compiled: rumdl depends on `tikv-jemallocator` on every target except msvc and wasm, and
`tikv-jemalloc-sys` cannot link for the mingw triple. The musl archives are built with `cross`,
whose container carries the musl toolchain jemalloc needs.

### Guidelines

- Binary resolution order is the platform package first, the download second. The npm launcher runs
  the binary from the matching `@polylint/adrlint-<platform>`; only when none is installed does
  `npm-package/install.js` download the archive, and a failed download never fails `npm install`. A
  platform wheel runs its bundled `adrlint/bin/adrlint`; only the `py3-none-any` fallback wheel,
  which alone depends on certifi, calls `adrlint/downloader.py`. The platform path needs no network.
- Platform packages and wheels are staged from the published archives after verifying them against
  `adrlint_<version>_checksums.txt`; never from a separate build.
- A new npm platform package must be published once by hand before trusted publishing can be
  configured for it: `scripts/npm-stage-from-release.sh <version> <outdir>` packs the tarballs from
  an existing release without publishing.
- Test `npm-package/install.js` and `pip-package/adrlint/downloader.py` whenever either changes; both
  verify SHA-256 against the release checksums before extracting. `tests/wrapper_resolution.rs`
  covers the resolution order and `tests/packaging_contract.rs` the platform inventory.
- Do not add a runtime download for a grammar, a formatter or a rule pack. rumdl is linked in, and the
  binary is self-contained.
- Release candidates use `vX.Y.Z-rc.N` tags; PyPI uses `X.Y.ZrcN`.
- The npm and pip wrappers pin their own version; they never ask the registry for "latest".
- Archive names are `adrlint-<triple>.tar.gz` and `adrlint-<triple>.zip`; the checksum manifest is
  `adrlint_<version>_checksums.txt`.
