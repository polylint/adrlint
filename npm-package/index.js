"use strict";

// Resolves the adrlint binary for this host.
//
// Distribution model: this package ships no binary. The eight platform packages
// listed under `optionalDependencies` each ship exactly one, and npm installs only
// the one whose `os`/`cpu`/`libc` match the host. That is the primary path: it
// needs no network at install time, so it works in sandboxes and offline caches,
// and the registry's integrity hash covers the binary.
//
// The fallback is the binary `install.js` downloads beside the launcher when no
// platform package is installed (optional dependencies disabled, or a host with
// no platform package). Resolution order is: platform package, then download.

const fs = require("node:fs");
const path = require("node:path");

// One entry per release target. `libc` only applies to Linux: the glibc and musl
// builds are separate archives, and `detectLibc()` picks the right one even on a
// package manager that ignores the `libc` field and installs both.
const PLATFORM_PACKAGES = [
  { pkg: "@polylint/adrlint-darwin-arm64", platform: "darwin", arch: "arm64", libc: null },
  { pkg: "@polylint/adrlint-darwin-x64", platform: "darwin", arch: "x64", libc: null },
  { pkg: "@polylint/adrlint-linux-arm64-gnu", platform: "linux", arch: "arm64", libc: "glibc" },
  { pkg: "@polylint/adrlint-linux-arm64-musl", platform: "linux", arch: "arm64", libc: "musl" },
  { pkg: "@polylint/adrlint-linux-x64-gnu", platform: "linux", arch: "x64", libc: "glibc" },
  { pkg: "@polylint/adrlint-linux-x64-musl", platform: "linux", arch: "x64", libc: "musl" },
  { pkg: "@polylint/adrlint-win32-arm64", platform: "win32", arch: "arm64", libc: null },
  { pkg: "@polylint/adrlint-win32-x64", platform: "win32", arch: "x64", libc: null },
];

const BINARY_NAME = process.platform === "win32" ? "adrlint.exe" : "adrlint";

// bin/adrlint is the launcher script, so the downloaded executable lives beside
// it under a distinct name and extraction never overwrites the launcher.
const DOWNLOADED_BINARY = path.join(__dirname, "bin", process.platform === "win32" ? "adrlint.exe" : "adrlint-bin");

/**
 * Report whether this Linux host runs glibc or musl.
 *
 * `process.report.getReport().header.glibcVersionRuntime` is present only when
 * the running Node was linked against glibc. Doing the check inline keeps this
 * package free of a `detect-libc` dependency.
 */
function detectLibc() {
  if (process.platform !== "linux") return null;
  try {
    const report = typeof process.report?.getReport === "function" ? process.report.getReport() : null;
    if (report && report.header) return report.header.glibcVersionRuntime ? "glibc" : "musl";
  } catch {
    // Fall through to the filesystem check below.
  }
  return fs.existsSync("/etc/alpine-release") ? "musl" : "glibc";
}

/** The platform packages that could serve this host, best match first. */
function candidatePackages() {
  const libc = detectLibc();
  return PLATFORM_PACKAGES.filter(
    (entry) =>
      entry.platform === process.platform &&
      entry.arch === process.arch &&
      (entry.libc === null || entry.libc === libc),
  ).map((entry) => entry.pkg);
}

/** The binary shipped by an installed platform package, or `null`. */
function resolvePlatformBinary() {
  for (const pkg of candidatePackages()) {
    let packageJson;
    try {
      packageJson = require.resolve(`${pkg}/package.json`);
    } catch {
      continue;
    }
    const candidate = path.join(path.dirname(packageJson), "bin", BINARY_NAME);
    if (fs.existsSync(candidate)) return candidate;
  }
  return null;
}

/** The binary `install.js` downloaded as a fallback, or `null`. */
function resolveDownloadedBinary() {
  return fs.existsSync(DOWNLOADED_BINARY) ? DOWNLOADED_BINARY : null;
}

/** The binary to run: the platform package's first, then the downloaded one. */
function resolveBinaryPath() {
  return resolvePlatformBinary() ?? resolveDownloadedBinary();
}

/** Why no binary was found, and how to fix it. */
function missingBinaryMessage() {
  const libc = detectLibc();
  const host = `${process.platform}/${process.arch}${libc ? ` (${libc})` : ""}`;
  const expected = candidatePackages();
  const lines = ["adrlint: the native binary is missing.", ""];

  if (expected.length === 0) {
    lines.push(`No prebuilt adrlint binary is published for ${host}.`);
  } else {
    lines.push(
      `The platform package for ${host} (${expected.join(", ")}) is not installed,`,
      "and the fallback download did not run or did not succeed.",
      "",
      "Reinstall with optional dependencies enabled, and with network access if the",
      "platform package cannot be installed:",
      "  npm i -g @polylint/adrlint",
    );
  }
  lines.push("", "Or build from source: cargo install adrlint");
  return lines.join("\n");
}

module.exports = {
  PLATFORM_PACKAGES,
  DOWNLOADED_BINARY,
  candidatePackages,
  detectLibc,
  missingBinaryMessage,
  resolveBinaryPath,
  resolveDownloadedBinary,
  resolvePlatformBinary,
};
