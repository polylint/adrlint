#!/usr/bin/env python3
"""Build the wheels for the ``adrlint`` distribution.

    python build_wheels.py --artifacts <dir> [--out dist] [--only <triple>|fallback]

``--artifacts`` points at the release archives exactly as published (``adrlint-<triple>.tar.gz`` /
``.zip``) plus ``adrlint_<version>_checksums.txt``. Every archive is verified against that manifest
before it is unpacked, so each wheel embeds the bytes the release checksums describe.

Produces one platform wheel per release target and one ``py3-none-any`` fallback wheel. pip ranks a
matching platform wheel above ``any``, so the fallback, which downloads the binary at first run, is
only chosen on a host no platform wheel covers.

Requires ``build`` and ``hatchling`` on the interpreter running this script.
"""

from __future__ import annotations

import argparse
import hashlib
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from dataclasses import dataclass
from pathlib import Path

PACKAGE_DIR = Path(__file__).resolve().parent

# The earliest macOS each binary declares (LC_BUILD_VERSION / LC_VERSION_MIN_MACOSX minos): Rust's
# defaults for the two Apple targets. A lower tag would let pip install a binary that cannot load.
MACOS_ARM64_MIN = "11_0"
MACOS_X86_64_MIN = "10_12"

# The musl archives are statically linked; musllinux_1_2 is the floor pip matches them against.
MUSLLINUX_VERSION = "1_2"

# Used only if no GLIBC_2.x symbol version can be read out of the binary.
MANYLINUX_FALLBACK_MINOR = 17

GLIBC_SYMBOL = re.compile(rb"GLIBC_2\.(\d+)")

FALLBACK_DEPENDENCY = "certifi"


@dataclass(frozen=True)
class Target:
    """One release triple and the wheel platform tag it maps onto."""

    triple: str

    @property
    def is_windows(self) -> bool:
        return "windows" in self.triple

    @property
    def archive_name(self) -> str:
        return f"adrlint-{self.triple}.{'zip' if self.is_windows else 'tar.gz'}"

    @property
    def binary_name(self) -> str:
        return "adrlint.exe" if self.is_windows else "adrlint"


TARGETS = (
    Target("aarch64-apple-darwin"),
    Target("x86_64-apple-darwin"),
    Target("aarch64-unknown-linux-gnu"),
    Target("x86_64-unknown-linux-gnu"),
    Target("aarch64-unknown-linux-musl"),
    Target("x86_64-unknown-linux-musl"),
    Target("aarch64-pc-windows-msvc"),
    Target("x86_64-pc-windows-msvc"),
)


def project_version() -> str:
    """The ``[project]`` version in pyproject.toml, kept in lock-step with the other manifests."""
    text = (PACKAGE_DIR / "pyproject.toml").read_text(encoding="utf-8")
    match = re.search(r'^version = "([^"]+)"$', text, re.MULTILINE)
    if match is None:
        message = "pyproject.toml has no [project] version"
        raise SystemExit(message)
    return match.group(1)


def release_version(python_version: str) -> str:
    """The release tag version for a PyPI version: ``1.2.3rc1`` is released as ``1.2.3-rc.1``."""
    if "rc" in python_version:
        core, suffix = python_version.split("rc", 1)
        return f"{core}-rc.{suffix}"
    return python_version


def required_glibc_minor(binary: Path) -> int:
    """Highest ``GLIBC_2.x`` symbol version the binary references.

    Read from the binary rather than assumed from the build image: a tag claiming an older glibc
    than the binary needs produces a wheel that installs and then fails at exec.
    """
    versions = [int(match) for match in GLIBC_SYMBOL.findall(binary.read_bytes())]
    return max(versions) if versions else MANYLINUX_FALLBACK_MINOR


def platform_tag(target: Target, binary: Path) -> str:
    """The PEP 425 platform tag for a target's wheel."""
    machine = "aarch64" if target.triple.startswith("aarch64") else "x86_64"
    if target.triple == "aarch64-apple-darwin":
        return f"macosx_{MACOS_ARM64_MIN}_arm64"
    if target.triple == "x86_64-apple-darwin":
        return f"macosx_{MACOS_X86_64_MIN}_x86_64"
    if target.triple == "aarch64-pc-windows-msvc":
        return "win_arm64"
    if target.triple == "x86_64-pc-windows-msvc":
        return "win_amd64"
    if target.triple.endswith("-linux-musl"):
        return f"musllinux_{MUSLLINUX_VERSION}_{machine}"
    return f"manylinux_2_{required_glibc_minor(binary)}_{machine}"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def expected_digests(manifest: Path) -> dict[str, str]:
    """``archive name -> sha256`` from a ``sha256sum``-format checksum manifest."""
    digests: dict[str, str] = {}
    for line in manifest.read_text(encoding="utf-8").splitlines():
        parts = line.split()
        if len(parts) == 2:  # noqa: PLR2004 - "<digest> <name>"
            digests[parts[1].lstrip("*").removeprefix("./")] = parts[0]
    return digests


def extract_binary(archive: Path, target: Target, destination: Path) -> Path:
    """Unpack the one expected member of a verified release archive, and nothing else."""
    destination.mkdir(parents=True, exist_ok=True)
    output = destination / target.binary_name

    if target.is_windows:
        with zipfile.ZipFile(archive) as zf:
            try:
                source = zf.open(target.binary_name)
            except KeyError:
                message = f"{archive} contains no {target.binary_name}"
                raise SystemExit(message) from None
            with source, output.open("wb") as sink:
                shutil.copyfileobj(source, sink)
    else:
        with tarfile.open(archive, "r:gz") as tf:
            try:
                member = tf.getmember(target.binary_name)
            except KeyError:
                member = None
            source = tf.extractfile(member) if member is not None and member.isfile() else None
            if source is None:
                message = f"{archive} contains no regular file {target.binary_name}"
                raise SystemExit(message)
            with source, output.open("wb") as sink:
                shutil.copyfileobj(source, sink)

    output.chmod(0o755)
    return output


def force_executable_bit(wheel: Path, member_prefix: str = "adrlint/bin/") -> None:
    """Rewrite the wheel so the bundled binary unpacks with mode 0755.

    pip honours the mode stored in a wheel entry's external attributes. Setting it here, rather than
    trusting the backend to keep the source file's mode, means a wheel can never install a binary
    that is not executable. RECORD hashes contents, which are untouched.
    """
    rewritten = wheel.with_suffix(".whl.tmp")
    with zipfile.ZipFile(wheel) as source, zipfile.ZipFile(rewritten, "w", zipfile.ZIP_DEFLATED) as sink:
        for info in source.infolist():
            data = source.read(info.filename)
            new_info = zipfile.ZipInfo(info.filename, date_time=info.date_time)
            new_info.compress_type = info.compress_type
            new_info.external_attr = info.external_attr
            if info.filename.startswith(member_prefix):
                # 0o100755: a regular file, rwxr-xr-x.
                new_info.external_attr = (0o100755 << 16) | (info.external_attr & 0xFFFF)
            sink.writestr(new_info, data)
    rewritten.replace(wheel)


def run_build(out_dir: Path, env_overrides: dict[str, str]) -> Path:
    """Invoke the wheel backend once and return the wheel it produced."""
    before = set(out_dir.glob("*.whl"))
    # Clear both hook variables first, so a stale ambient value cannot turn the fallback wheel into a
    # platform one.
    env = {key: value for key, value in os.environ.items() if not key.startswith("ADRLINT_WHEEL_")}
    env.update(env_overrides)

    subprocess.run(  # noqa: S603 - fixed argv
        [sys.executable, "-m", "build", "--wheel", "--no-isolation", "--outdir", str(out_dir)],
        cwd=PACKAGE_DIR,
        check=True,
        env=env,
    )

    produced = set(out_dir.glob("*.whl")) - before
    if len(produced) != 1:
        message = f"expected exactly one new wheel, got {sorted(p.name for p in produced)}"
        raise SystemExit(message)
    return produced.pop()


def build_platform_wheel(target: Target, binary: Path, out_dir: Path) -> Path:
    tag = platform_tag(target, binary)
    wheel = run_build(out_dir, {"ADRLINT_WHEEL_TAG": tag, "ADRLINT_WHEEL_BINARY": str(binary)})
    force_executable_bit(wheel)
    print(f"  ok {target.triple:28} -> {wheel.name}")  # noqa: T201
    return wheel


def build_fallback_wheel(out_dir: Path) -> Path:
    """The ``py3-none-any`` wheel: no binary, the release download at first run instead."""
    wheel = run_build(out_dir, {})
    with zipfile.ZipFile(wheel) as zf:
        if any(name.startswith("adrlint/bin/") for name in zf.namelist()):
            message = f"{wheel.name} must not embed a binary"
            raise SystemExit(message)
        metadata = next(name for name in zf.namelist() if name.endswith(".dist-info/METADATA"))
        if f"Requires-Dist: {FALLBACK_DEPENDENCY}" not in zf.read(metadata).decode("utf-8"):
            message = f"{wheel.name} does not declare its {FALLBACK_DEPENDENCY} dependency"
            raise SystemExit(message)
    print(f"  ok {'fallback (downloads at run)':28} -> {wheel.name}")  # noqa: T201
    return wheel


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--artifacts", type=Path, required=True, help="directory holding the release archives")
    parser.add_argument("--out", type=Path, default=PACKAGE_DIR / "dist", help="output directory")
    parser.add_argument("--only", help="build just this triple, or 'fallback'")
    args = parser.parse_args()

    known = {target.triple for target in TARGETS} | {"fallback"}
    if args.only is not None and args.only not in known:
        message = f"--only must be one of {sorted(known)}"
        raise SystemExit(message)

    out_dir = args.out.resolve()
    out_dir.mkdir(parents=True, exist_ok=True)
    version = project_version()
    artifacts = args.artifacts.resolve()
    print(f"Building adrlint {version} wheels into {out_dir}")  # noqa: T201

    if args.only in (None, *(target.triple for target in TARGETS)):
        manifest = artifacts / f"adrlint_{release_version(version)}_checksums.txt"
        if not manifest.is_file():
            message = f"missing checksum manifest: {manifest}"
            raise SystemExit(message)
        digests = expected_digests(manifest)

        with tempfile.TemporaryDirectory() as staging_root:
            staging = Path(staging_root)
            for target in TARGETS:
                if args.only not in (None, target.triple):
                    continue
                archive = artifacts / target.archive_name
                if not archive.is_file():
                    message = f"missing release archive: {archive}"
                    raise SystemExit(message)
                expected = digests.get(target.archive_name)
                if expected is None:
                    message = f"{manifest.name} lists no checksum for {target.archive_name}"
                    raise SystemExit(message)
                actual = sha256_file(archive)
                if actual != expected:
                    message = f"checksum mismatch for {target.archive_name}: expected {expected}, got {actual}"
                    raise SystemExit(message)
                binary = extract_binary(archive, target, staging / target.triple)
                build_platform_wheel(target, binary, out_dir)

    if args.only in (None, "fallback"):
        build_fallback_wheel(out_dir)

    print(f"\nWheels in {out_dir}:")  # noqa: T201
    for wheel in sorted(out_dir.glob("*.whl")):
        print(f"  {wheel.name}")  # noqa: T201


if __name__ == "__main__":
    main()
