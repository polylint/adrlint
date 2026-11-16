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
- **npm** (`@polylint/adrlint`, bin `adrlint`; the unscoped `adrlint` is deprecated): wrapper in
  `npm-package/`; `install.js` downloads and verifies the release
  archive matching the platform triple.
- **PyPI** (`adrlint`): wrapper in `pip-package/`; `adrlint/downloader.py` performs the same
  download-and-cache strategy.
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
4. npm, PyPI, Homebrew and Scoop publish from the finalized release assets.
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

- Test `npm-package/install.js` and `pip-package/adrlint/downloader.py` whenever either changes; both
  verify SHA-256 against the release checksums before extracting.
- Do not add a runtime download for a grammar, a formatter or a rule pack. rumdl is linked in, and the
  binary is self-contained.
- Release candidates use `vX.Y.Z-rc.N` tags; PyPI uses `X.Y.ZrcN`.
- The npm and pip wrappers pin their own version; they never ask the registry for "latest".
- Archive names are `adrlint-<triple>.tar.gz` and `adrlint-<triple>.zip`; the checksum manifest is
  `adrlint_<version>_checksums.txt`.
