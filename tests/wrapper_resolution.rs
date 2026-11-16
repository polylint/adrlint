//! Behaviour tests for the npm and PyPI wrappers' binary resolution.
//!
//! Each test assembles the wrapper's real files in a temporary directory, stands in a shell script
//! for the native binary, and runs the wrapper with `node` or `python3`. Nothing touches the
//! network: the platform-package path must work offline, and the download fallback is exercised up
//! to the point where it would fetch.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const NPM_PLATFORMS: &[&str] = &[
    "darwin-arm64",
    "darwin-x64",
    "linux-arm64-gnu",
    "linux-arm64-musl",
    "linux-x64-gnu",
    "linux-x64-musl",
    "win32-arm64",
    "win32-x64",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// A stand-in binary that reports which copy ran and the arguments it was given.
fn fake_binary(path: &Path, label: &str) {
    fs::create_dir_all(path.parent().expect("a parent directory")).expect("creating the bin directory");
    fs::write(path, format!("#!/bin/sh\necho \"{label} $*\"\n")).expect("writing the fake binary");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("marking the fake binary executable");
}

fn copy(relative: &str, destination: &Path) {
    fs::create_dir_all(destination.parent().expect("a parent directory")).expect("creating a directory");
    fs::copy(repo_root().join(relative), destination).unwrap_or_else(|error| panic!("copying {relative}: {error}"));
}

/// `node_modules/@polylint/adrlint` holding the umbrella's published files, and nothing installed
/// beside it: no tar, no adm-zip, no platform package.
fn npm_umbrella(root: &Path) -> PathBuf {
    let umbrella = root.join("node_modules/@polylint/adrlint");
    for file in ["package.json", "index.js", "install.js", "bin/adrlint"] {
        copy(&format!("npm-package/{file}"), &umbrella.join(file));
    }
    umbrella
}

/// Install a stand-in for every platform package; the launcher only considers the host's.
fn npm_platform_packages(root: &Path) {
    for platform in NPM_PLATFORMS {
        let dir = root.join(format!("node_modules/@polylint/adrlint-{platform}"));
        fs::create_dir_all(&dir).expect("creating the platform package");
        fs::write(
            dir.join("package.json"),
            format!(r#"{{"name": "@polylint/adrlint-{platform}", "version": "0.0.0"}}"#),
        )
        .expect("writing the platform package manifest");
        fake_binary(&dir.join("bin/adrlint"), "platform");
    }
}

fn run(program: &str, args: &[&str], cwd: &Path, envs: &[(&str, &str)]) -> Output {
    Command::new(program)
        .args(args)
        .current_dir(cwd)
        .envs(envs.iter().copied())
        .env_remove("ADRLINT_BINARY")
        .output()
        .unwrap_or_else(|error| panic!("could not run {program}: {error}"))
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn the_npm_launcher_runs_the_platform_package_binary_first() {
    let temp = tempfile::tempdir().expect("a temporary directory");
    let umbrella = npm_umbrella(temp.path());
    npm_platform_packages(temp.path());
    fake_binary(&umbrella.join("bin/adrlint-bin"), "downloaded");

    let output = run("node", &["bin/adrlint", "--version"], &umbrella, &[]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "platform --version\n");
}

#[test]
fn the_npm_launcher_falls_back_to_the_downloaded_binary() {
    let temp = tempfile::tempdir().expect("a temporary directory");
    let umbrella = npm_umbrella(temp.path());
    fake_binary(&umbrella.join("bin/adrlint-bin"), "downloaded");

    let output = run("node", &["bin/adrlint", "lint"], &umbrella, &[]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "downloaded lint\n");
}

#[test]
fn the_npm_launcher_explains_a_missing_binary() {
    let temp = tempfile::tempdir().expect("a temporary directory");
    let umbrella = npm_umbrella(temp.path());

    let output = run("node", &["bin/adrlint", "--version"], &umbrella, &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).starts_with("adrlint: the native binary is missing."),
        "{}",
        stderr(&output)
    );
}

/// With the platform package installed, the postinstall does nothing: it neither downloads nor
/// loads the archive readers, which are not even installed here.
#[test]
fn the_npm_postinstall_is_a_no_op_when_the_platform_package_resolved() {
    let temp = tempfile::tempdir().expect("a temporary directory");
    let umbrella = npm_umbrella(temp.path());
    npm_platform_packages(temp.path());

    let output = run("node", &["install.js"], &umbrella, &[]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "");
    assert_eq!(stderr(&output), "");
    assert!(!umbrella.join("bin/adrlint-bin").exists());
}

/// A copy of the `adrlint` Python package under `<root>/site`, importable via PYTHONPATH.
fn python_package(root: &Path) -> PathBuf {
    let package = root.join("site/adrlint");
    for file in ["__init__.py", "__main__.py", "cli.py", "downloader.py"] {
        copy(&format!("pip-package/adrlint/{file}"), &package.join(file));
    }
    package
}

/// `python3 -S`: no site-packages, so the only `adrlint` (and no certifi) is the copy under test.
fn run_python(root: &Path, args: &[&str]) -> Output {
    let site = root.join("site");
    let mut argv = vec!["-S"];
    argv.extend_from_slice(args);
    run(
        "python3",
        &argv,
        root,
        &[
            ("PYTHONPATH", site.to_str().expect("a UTF-8 path")),
            ("HOME", root.to_str().expect("a UTF-8 path")),
            ("PYTHONDONTWRITEBYTECODE", "1"),
        ],
    )
}

#[test]
fn the_python_wrapper_runs_the_binary_bundled_in_the_platform_wheel() {
    let temp = tempfile::tempdir().expect("a temporary directory");
    let package = python_package(temp.path());
    fake_binary(&package.join("bin/adrlint"), "bundled");

    let output = run_python(temp.path(), &["-m", "adrlint", "--version"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "bundled --version\n");
}

/// The fallback wheel has no `adrlint/bin/`; the wrapper must hand off to the downloader. The
/// downloader is replaced by a stub so the hand-off is observed without a network request.
#[test]
fn the_python_fallback_wheel_hands_off_to_the_downloader() {
    let temp = tempfile::tempdir().expect("a temporary directory");
    let package = python_package(temp.path());
    let downloaded = temp.path().join("cache/adrlint");
    fake_binary(&downloaded, "downloaded");
    fs::write(
        package.join("downloader.py"),
        format!(
            "def ensure_binary():\n    return {:?}\n",
            downloaded.to_str().expect("a UTF-8 path")
        ),
    )
    .expect("writing the downloader stub");

    let output = run_python(temp.path(), &["-m", "adrlint", "lint"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "downloaded lint\n");
}

/// A platform wheel does not depend on certifi, so the downloader must still import without it.
#[test]
fn the_python_downloader_imports_without_certifi() {
    let temp = tempfile::tempdir().expect("a temporary directory");
    python_package(temp.path());

    let output = run_python(
        temp.path(),
        &[
            "-c",
            "import adrlint.downloader, importlib.util; print(importlib.util.find_spec('certifi'))",
        ],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "None\n");
}
