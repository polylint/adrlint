# adrlint

[![npm](https://img.shields.io/npm/v/@polylint/adrlint?style=flat-square&color=f59e0b&label=npm)](https://www.npmjs.com/package/@polylint/adrlint)
[![License: MIT](https://img.shields.io/badge/License-MIT-f59e0b?style=flat-square)](https://github.com/polylint/adrlint/blob/main/LICENSE)

A fast Rust CLI that lints and formats [MADR](https://adr.github.io/madr/) architecture decision
records: structure, numbering, Markdown quality, and immutability of accepted decisions.

## Installation

```bash
npm i -g @polylint/adrlint      # or run once: npx @polylint/adrlint lint
```

The installed command is `adrlint`. This package replaces the unscoped `adrlint` npm package,
which is deprecated.

The binary comes from a per-platform package, `@polylint/adrlint-<platform>`, that npm selects through
`optionalDependencies` by `os`, `cpu` and `libc`, so the install needs no network access beyond the
registry. Platform packages exist for Linux `x64`/`arm64` (glibc and musl), macOS `x64`/`arm64` and
Windows `x64`/`arm64`.

If no platform package is installed (optional dependencies disabled, or an unsupported host), the
`postinstall` step falls back to downloading the release archive and verifying it against the release
checksum manifest. A failed fallback download does not fail the install; `adrlint` then explains how
to get a binary, for example with `cargo install adrlint`.

## Usage

```bash
adrlint init                    # write .adrlint.toml and docs/adr
adrlint lint                    # check every ADR
adrlint new "Use PostgreSQL"    # create the next sequential ADR
adrlint lint --format github    # GitHub annotations for CI
```

## Documentation

Rules, configuration, locking and the full CLI reference:
[https://github.com/polylint/adrlint](https://github.com/polylint/adrlint)

## polylint organization

adrlint is part of [polylint](https://github.com/polylint), high-performance Rust linters and
formatters built for agentic workflows. Sibling tools: [polylint](https://github.com/polylint/polylint)
(one linter and formatter for every language; it can run adrlint as an engine via `poly adr`),
[skif](https://github.com/polylint/skif) (JS/TS dependency linter), and
[uncomment](https://github.com/polylint/uncomment) (comment removal and linting). Install any of them
from the Homebrew tap (`brew tap polylint/tap`) or the Scoop bucket
(`scoop bucket add polylint https://github.com/polylint/scoop-bucket`).

## License

MIT — see [LICENSE](https://github.com/polylint/adrlint/blob/main/LICENSE).
