"""
CLI entry point for adrlint.

Resolution order: a platform wheel carries the binary under ``adrlint/bin/`` and runs it directly,
with no network access. Only the ``py3-none-any`` fallback wheel, which pip picks when no platform
wheel matches the host, has no bundled binary; it hands off to :mod:`adrlint.downloader`, which
fetches and verifies the release archive on first run. ``ADRLINT_BINARY`` overrides both.
"""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

#: Directory a platform wheel unpacks the binary into.
_BIN_DIR = Path(__file__).resolve().parent / "bin"

#: Executable name inside that directory.
_BINARY_NAME = "adrlint.exe" if os.name == "nt" else "adrlint"


def bundled_binary() -> Path | None:
    """The binary shipped inside this platform wheel, or ``None`` for the fallback wheel."""
    candidate = _BIN_DIR / _BINARY_NAME
    return candidate if candidate.is_file() else None


def _resolve_binary() -> str:
    override = os.getenv("ADRLINT_BINARY")
    if override:
        return override

    bundled = bundled_binary()
    if bundled is not None:
        return str(bundled)

    from .downloader import ensure_binary

    try:
        return ensure_binary()
    except (OSError, RuntimeError) as exc:
        print(
            f"adrlint: no platform wheel matched this host and the fallback download failed: {exc}\n"
            "Set ADRLINT_BINARY to an installed adrlint, or run `cargo install adrlint`.",
            file=sys.stderr,
        )
        raise SystemExit(1) from exc


def main() -> None:
    """Run the adrlint binary with this process's arguments and exit with its status."""
    binary = _resolve_binary()
    argv = [binary, *sys.argv[1:]]

    if os.name == "nt":
        # Windows has no exec that replaces the process, so spawn and forward the status.
        try:
            completed = subprocess.run(argv, check=False)
        except OSError as exc:
            print(f"adrlint: failed to run {binary}: {exc}", file=sys.stderr)
            raise SystemExit(1) from exc
        raise SystemExit(completed.returncode)

    # execv replaces this process, so exit codes and signals reach the caller exactly as they would
    # from the standalone binary.
    try:
        os.execv(binary, argv)
    except OSError as exc:
        print(f"adrlint: failed to exec {binary}: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc


if __name__ == "__main__":
    main()
