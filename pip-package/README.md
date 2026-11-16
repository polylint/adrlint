# adrlint

[![PyPI](https://img.shields.io/pypi/v/adrlint?style=flat-square&color=f59e0b)](https://pypi.org/project/adrlint/)
[![License: MIT](https://img.shields.io/badge/License-MIT-f59e0b?style=flat-square)](https://github.com/polylint/adrlint/blob/main/LICENSE)

A fast Rust CLI that lints and formats [MADR](https://adr.github.io/madr/) architecture decision
records: structure, numbering, Markdown quality, and immutability of accepted decisions.

## Installation

```bash
pip install adrlint
```

Each supported platform has its own wheel with the binary inside, so `pip` installs it without
fetching anything else: Linux `x86_64`/`aarch64` (`manylinux` for glibc, `musllinux` for musl), macOS
`x86_64`/`arm64` and Windows `x86_64`/`arm64`.

On any other host pip falls back to the pure-Python `py3-none-any` wheel, which downloads the
pre-compiled binary on first run and verifies it against the release checksum manifest.

Set `ADRLINT_BINARY` to use an existing executable instead of the bundled or downloaded one.

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
