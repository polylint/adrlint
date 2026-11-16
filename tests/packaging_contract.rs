//! Contract tests for the packaging and release plumbing.
//!
//! Everything here is a pure-text assertion over the checked-in workflows, action and downloaders,
//! so the contract is enforced without network access, a build matrix or a published release.

use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(relative: &str) -> String {
    let path = repo_root().join(relative);
    // ~keep A Windows checkout may use CRLF; the assertions match LF text.
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()))
        .replace("\r\n", "\n")
}

/// Every workflow plus the composite action at the repository root: all of them run `uses:` steps.
fn action_yaml_files() -> Vec<(String, String)> {
    let workflows = repo_root().join(".github/workflows");
    let mut paths: Vec<PathBuf> = fs::read_dir(&workflows)
        .unwrap_or_else(|error| panic!("could not list {}: {error}", workflows.display()))
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| matches!(path.extension().and_then(|ext| ext.to_str()), Some("yml" | "yaml")))
        .collect();
    paths.sort();

    let mut files: Vec<(String, String)> = paths
        .iter()
        .map(|path| {
            (
                path.display().to_string(),
                fs::read_to_string(path)
                    .expect("reading a workflow")
                    .replace("\r\n", "\n"),
            )
        })
        .collect();
    files.push(("action.yml".to_owned(), read("action.yml")));
    files
}

/// The line and indentation of a `key:` whose value is a block scalar, so its body can be found.
fn block_start(line: &str, key: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix(key)?.strip_prefix(':')?.trim();
    if rest.is_empty() || rest.starts_with('|') || rest.starts_with('>') {
        return Some(line.len() - trimmed.len());
    }
    None
}

/// The bodies of every `run:` block (`run: |` scalars and inline `run: command` lines).
fn run_blocks(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut blocks = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start();

        if let Some(indent) = block_start(line, "run") {
            index += 1;
            let mut body = String::new();
            while index < lines.len() {
                let candidate = lines[index];
                if candidate.trim().is_empty() {
                    body.push('\n');
                    index += 1;
                    continue;
                }
                if candidate.len() - candidate.trim_start().len() <= indent {
                    break;
                }
                body.push_str(candidate);
                body.push('\n');
                index += 1;
            }
            blocks.push(body);
            continue;
        }

        if let Some(inline) = trimmed.strip_prefix("run:") {
            blocks.push(inline.to_string());
        }
        index += 1;
    }

    blocks
}

/// Every `uses:` value in a file, including `- uses:` list entries but not mentions in comments.
fn uses_entries(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim_start().strip_prefix("- ").unwrap_or(line.trim_start());
            trimmed
                .strip_prefix("uses:")?
                .split_whitespace()
                .next()
                .map(str::to_owned)
        })
        .collect()
}

/// A bare major tag such as `v7`: the org pins every action to its floating major tag.
fn is_major_tag(value: &str) -> bool {
    value
        .strip_prefix('v')
        .is_some_and(|digits| !digits.is_empty() && digits.chars().all(|character| character.is_ascii_digit()))
}

/// Actions whose upstream publishes no major tag, so a full version tag is the shortest pin.
const FULL_TAG_ACTIONS: &[&str] = &["pypa/gh-action-pypi-publish"];

fn is_full_version_tag(value: &str) -> bool {
    value.strip_prefix('v').is_some_and(|version| {
        let parts: Vec<&str> = version.split('.').collect();
        parts.len() == 3
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
    })
}

/// The block belonging to one top-level job, bounded by the next job key (indented exactly two
/// spaces; deeper indentation belongs to the job's own steps).
fn job_block<'a>(text: &'a str, job: &str) -> &'a str {
    let marker = format!("\n  {job}:");
    let start = text
        .find(&marker)
        .unwrap_or_else(|| panic!("job `{job}` is missing from publish.yaml"));
    let body = start + marker.len();
    let mut offset = body;
    for line in text[body..].split_inclusive('\n') {
        let is_job_key = line.starts_with("  ") && !line.starts_with("   ") && !line.trim().is_empty();
        if is_job_key {
            return &text[start..offset];
        }
        offset += line.len();
    }
    &text[start..]
}

fn line_number(text: &str, needle: &str) -> usize {
    text.find(needle)
        .unwrap_or_else(|| panic!("`{needle}` does not appear in the file"))
}

#[test]
fn no_expression_is_interpolated_inside_a_run_block() {
    for (name, text) in action_yaml_files() {
        for body in run_blocks(&text) {
            assert!(
                !body.contains("${{"),
                "{name}: a `run:` block interpolates an expression; pass the value through `env:` and read \
                 the quoted shell variable instead:\n{body}"
            );
        }
    }
}

#[test]
fn every_action_reference_is_pinned_to_a_major_tag() {
    for (name, text) in action_yaml_files() {
        for reference in uses_entries(&text) {
            if reference.starts_with("./") {
                continue;
            }
            let (repository, revision) = reference
                .rsplit_once('@')
                .unwrap_or_else(|| panic!("{name}: `uses: {reference}` carries no ref"));
            assert!(
                !repository.is_empty(),
                "{name}: `uses: {reference}` names no repository"
            );
            let full_tag_allowed = FULL_TAG_ACTIONS.contains(&repository) && is_full_version_tag(revision);
            assert!(
                is_major_tag(revision) || full_tag_allowed,
                "{name}: `uses: {reference}` must be pinned to a bare major tag such as `v1`"
            );
        }
    }
}

#[test]
fn the_action_test_pins_the_release_version() {
    let text = read(".github/workflows/action.yml");
    let line = text
        .lines()
        .find(|line| line.trim_start().starts_with("ADRLINT_ACTION_TEST_VERSION:"))
        .expect("the action test workflow declares ADRLINT_ACTION_TEST_VERSION");
    let value = line
        .split_once(':')
        .expect("a key/value pair")
        .1
        .trim()
        .trim_matches('"');
    assert_eq!(
        value, "0.4.0",
        "the action test must default to the release it exercises"
    );
}

#[test]
fn the_release_workflow_validates_the_tag_before_building() {
    let text = read(".github/workflows/publish.yaml");

    // Anchored: an unanchored pattern would accept `v1.2.3; rm -rf /` and friends.
    let pattern = "^v[0-9]+\\.[0-9]+\\.[0-9]+(-[0-9A-Za-z.]+)?$";
    let validation = line_number(&text, pattern);
    let first_build = line_number(&text, "cargo build --release");
    assert!(
        validation < first_build,
        "the tag must be validated before the first build"
    );

    for line in text.lines() {
        let trimmed = line.trim();
        assert!(
            !(trimmed.contains("curl") && trimmed.contains("| sh")),
            "publish.yaml pipes a downloaded script into a shell: {trimmed}"
        );
    }

    // Taps are pushed with a short-lived org GitHub App token, never a long-lived PAT or a token
    // embedded in a clone URL.
    assert!(
        !text.contains("HOMEBREW_TOKEN"),
        "the retired HOMEBREW_TOKEN PAT must not come back"
    );
    assert!(!text.contains("x-access-token"), "no clone URL may embed a credential");
}

#[test]
fn every_build_and_publish_job_requires_the_verify_job() {
    let text = read(".github/workflows/publish.yaml");

    assert!(
        !text.contains("result != 'failure'"),
        "a `!= 'failure'` guard treats a skipped dependency as success"
    );

    let verify = job_block(&text, "verify");
    for command in [
        "cargo fmt --all -- --check",
        "cargo clippy --all-targets --all-features -- -D warnings",
        "cargo test --locked",
        "cargo-deny-action",
    ] {
        assert!(verify.contains(command), "the verify job does not run `{command}`");
    }

    for job in [
        "build-macos",
        "build-windows",
        "build-linux",
        "build-linux-musl",
        "publish_crates",
        "publish_npm",
        "publish_pypi",
        "publish_homebrew",
        "homebrew_bottles",
        "publish_homebrew_bottles",
        "publish_scoop",
    ] {
        let block = job_block(&text, job);
        let needs = block
            .lines()
            .find(|line| line.trim_start().starts_with("needs:"))
            .unwrap_or_else(|| panic!("job `{job}` declares no needs"));
        assert!(
            needs.contains("verify"),
            "job `{job}` does not depend on verify: {needs}"
        );
        assert!(
            block.contains("== 'success'"),
            "job `{job}` must require an explicit success result from its dependencies"
        );
    }
}

#[test]
fn the_npm_downloader_verifies_before_it_extracts() {
    let script = read("npm-package/install.js");

    let checksum = line_number(&script, "Checksum mismatch for ${archiveName}");
    assert!(checksum < line_number(&script, "await tar.extract("));
    assert!(checksum < line_number(&script, "zip.extractEntryTo("));

    assert!(
        script.contains("const MAX_DOWNLOAD_BYTES = 100 * 1024 * 1024;"),
        "the downloader must cap the response body it buffers"
    );
    assert!(
        script.contains("received > MAX_DOWNLOAD_BYTES"),
        "the size cap must be enforced while the body streams in, not only from Content-Length"
    );

    // A cached binary is re-verified against the manifest and its install-time digest before reuse.
    let reuse = line_number(&script, "if (await cachedBinaryIsValid(");
    assert!(reuse < line_number(&script, "const archive = await httpsGet("));
    assert!(script.contains("sha256(fs.readFileSync(binaryPath)) === recordedBinary"));
}

#[test]
fn the_python_downloader_verifies_before_it_extracts() {
    let module = read("pip-package/adrlint/downloader.py");

    let docstring: String = module.lines().take(12).collect::<Vec<_>>().join("\n");
    assert!(
        docstring.contains("ADRLINT_BINARY"),
        "the module docstring must document the ADRLINT_BINARY override"
    );

    let checksum = line_number(&module, "expected = _expected_digest(version, asset_name)");
    assert!(checksum < line_number(&module, "_extract(archive_path, ext, extracted)"));
    assert!(module.contains("mode=0o700"), "the cache directory must be private");
    assert!(
        module.contains("tempfile.mkstemp(") && module.contains("os.replace(staging, destination)"),
        "cache writes must land on a fresh exclusive file and be renamed into place"
    );

    let reuse = line_number(
        &module,
        "if _cache_is_valid(binary_path, record_path, __version__, asset_name):",
    );
    assert!(reuse < line_number(&module, "archive, asset_name, ext, archive_digest = _download("));
    assert!(module.contains("_sha256_file(binary_path) == recorded_binary"));
}

#[test]
fn the_action_installer_extracts_only_the_expected_member() {
    let script = read("scripts/gh-action-install.sh");

    let checksum = line_number(&script, "actual=\"$(sha256 \"$ASSET\")\"");
    let extraction = line_number(&script, "tar -xzf \"$ASSET\" -C \"$extract_dir\" \"$bin_name\"");
    assert!(
        checksum < extraction,
        "the archive checksum gate must precede extraction"
    );

    assert!(
        !script.contains("extractall"),
        "the installer must not unpack every member of the archive"
    );
    assert!(
        !script.contains("-C \"$BIN_DIR\""),
        "the archive must be unpacked into a private directory, not straight into BIN_DIR"
    );
    assert!(
        script.contains("find \"$extract_dir\" -type f -name \"$bin_name\""),
        "the installer must locate the expected member after extraction"
    );
}

/// Every file that maps a platform onto a release archive: the workflows, the composite action at
/// the repository root, and the three installers that download the archive.
fn platform_maps() -> Vec<(String, String)> {
    let mut files = action_yaml_files();
    for relative in [
        "npm-package/install.js",
        "npm-package/index.js",
        "npm-package/stage-binaries.sh",
        "pip-package/adrlint/downloader.py",
        "pip-package/build_wheels.py",
        "scripts/gh-action-resolve.sh",
        "scripts/gh-action-install.sh",
        "scripts/gh-action-run.sh",
    ] {
        files.push((relative.to_owned(), read(relative)));
    }
    files
}

/// The Windows archive is built natively with MSVC because rumdl's `tikv-jemallocator` dependency
/// is dropped on msvc and `tikv-jemalloc-sys` cannot link for `x86_64-pc-windows-gnu`. A leftover
/// `windows-gnu` reference would publish a triple no installer requests, or request one no build
/// publishes.
#[test]
fn no_release_surface_still_names_the_windows_gnu_target() {
    for (name, text) in platform_maps() {
        for needle in ["windows-gnu", "windows_gnu"] {
            assert!(
                !text.contains(needle),
                "{name} still references `{needle}`; the Windows archive is `x86_64-pc-windows-msvc`"
            );
        }
    }
}

#[test]
fn the_windows_archive_is_built_natively_with_msvc() {
    let text = read(".github/workflows/publish.yaml");
    let block = job_block(&text, "build-windows");

    assert!(
        block.contains("runs-on: ${{ matrix.os }}"),
        "the Windows archives must be built on Windows runners, not cross-compiled"
    );
    assert!(
        block.contains("triple: x86_64-pc-windows-msvc"),
        "build-windows must target x86_64-pc-windows-msvc"
    );
    assert!(
        block.contains("triple: aarch64-pc-windows-msvc"),
        "build-windows must target aarch64-pc-windows-msvc"
    );
    assert!(
        block.contains("os: windows-latest") && block.contains("os: windows-11-arm"),
        "each Windows triple needs its native runner"
    );
    assert!(
        block.contains("targets: ${{ matrix.triple }}"),
        "the Windows job must install the target it builds"
    );
    assert!(
        block.contains("cargo build --release --bin adrlint --target $env:TRIPLE"),
        "build-windows must build the binary for its target"
    );

    // The mingw cross-build is what failed on 0.1.0; none of its toolchain setup may return. The
    // job's own comment explains why msvc is required, so only executable lines are checked.
    let commands: String = block
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    for stale in ["mingw", "x86_64-w64-mingw32", "apt-get"] {
        assert!(
            !commands.contains(stale),
            "build-windows still runs `{stale}`-related setup; the job builds natively with MSVC"
        );
    }

    assert!(
        block.contains("Compress-Archive"),
        "the Windows archive must be packaged with Compress-Archive"
    );
    assert!(
        block.contains(r#""target/$env:TRIPLE/release/adrlint.exe""#),
        "Compress-Archive must package the produced adrlint.exe"
    );
    assert!(
        block.contains(r#""adrlint-$env:TRIPLE.zip""#),
        "the archive must be named adrlint-<triple>.zip"
    );
    assert!(
        block.contains("gh release upload"),
        "build-windows must upload its archive to the release"
    );
}

#[test]
fn every_installer_requests_the_msvc_windows_archive() {
    let npm = read("npm-package/install.js");
    assert!(
        npm.contains(r#""Windows_NT:x64": "x86_64-pc-windows-msvc","#),
        "the npm wrapper must resolve Windows x64 to the msvc archive"
    );
    assert!(
        npm.contains(r#"const archiveName = `adrlint-${triple}.${isWindows ? "zip" : "tar.gz"}`;"#),
        "the npm wrapper must still request a .zip on Windows"
    );
    assert!(
        npm.contains(r#""Linux:x64:musl": "x86_64-unknown-linux-musl","#)
            && npm.contains(r#""Linux:arm64:musl": "aarch64-unknown-linux-musl","#),
        "the npm wrapper must resolve musl Linux to the musl archives"
    );
    assert!(
        npm.contains(r#""Windows_NT:arm64": "aarch64-pc-windows-msvc","#),
        "the npm wrapper must resolve Windows arm64 to the arm64 archive"
    );

    let python = read("pip-package/adrlint/downloader.py");
    assert!(
        python.contains(r#"return "x86_64-pc-windows-msvc""#),
        "the pip wrapper must resolve Windows amd64 to the msvc archive"
    );
    assert!(
        python.contains(r#"return f"x86_64-unknown-linux-{libc}""#)
            && python.contains(r#"return f"aarch64-unknown-linux-{libc}""#),
        "the pip wrapper must select the gnu or musl Linux archive at runtime"
    );
    assert!(
        python.contains(r#"return "aarch64-pc-windows-msvc""#),
        "the pip wrapper must resolve Windows arm64 to the arm64 archive"
    );
    assert!(
        python.contains(r#"ext = "zip" if "windows" in triple else "tar.gz""#),
        "the pip wrapper must still request a .zip on Windows"
    );

    let resolve = read("scripts/gh-action-resolve.sh");
    assert!(
        resolve.contains(r#"X64) triple="x86_64-pc-windows-msvc" ;;"#),
        "the GitHub Action must resolve Windows X64 to the msvc archive"
    );

    let install = read("scripts/gh-action-install.sh");
    assert!(
        install.contains(r#"bin_name="adrlint.exe""#),
        "the GitHub Action must publish adrlint.exe on Windows"
    );
    assert!(
        install.contains("zip)"),
        "the GitHub Action installer must still handle the .zip archive"
    );
}

#[test]
fn the_release_asset_lists_name_the_msvc_archive() {
    let text = read(".github/workflows/publish.yaml");
    let archive = "adrlint-x86_64-pc-windows-msvc.zip";

    for job in ["meta", "checksums", "finalize_release"] {
        assert!(
            job_block(&text, job).contains(archive),
            "job `{job}` does not expect {archive} on the release"
        );
    }

    assert!(
        job_block(&text, "verify-binstall").contains(r#"{"os": "windows-latest", "target": "x86_64-pc-windows-msvc"}"#),
        "the binstall smoke test must install the msvc archive"
    );
}

#[test]
fn the_release_asset_lists_name_every_published_archive() {
    let text = read(".github/workflows/publish.yaml");
    let archives = [
        "adrlint-x86_64-unknown-linux-gnu.tar.gz",
        "adrlint-aarch64-unknown-linux-gnu.tar.gz",
        "adrlint-x86_64-unknown-linux-musl.tar.gz",
        "adrlint-aarch64-unknown-linux-musl.tar.gz",
        "adrlint-aarch64-apple-darwin.tar.gz",
        "adrlint-x86_64-apple-darwin.tar.gz",
        "adrlint-x86_64-pc-windows-msvc.zip",
        "adrlint-aarch64-pc-windows-msvc.zip",
    ];

    for job in ["meta", "checksums", "finalize_release"] {
        let block = job_block(&text, job);
        for archive in archives {
            assert!(
                block.contains(archive),
                "job `{job}` does not expect {archive} on the release"
            );
        }
    }

    let musl = job_block(&text, "build-linux-musl");
    for triple in ["x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl"] {
        assert!(
            musl.contains(&format!("triple: {triple}")),
            "build-linux-musl does not build {triple}"
        );
    }
    assert!(
        musl.contains("cross build --release --bin adrlint"),
        "musl archives need cross's musl toolchain"
    );
}

const RELEASE_BASE: &str = "https://github.com/polylint/adrlint/releases/download";

#[test]
fn the_scoop_manifest_matches_the_published_assets() {
    let template = read("scripts/publish/adrlint.json.tmpl");
    let config = read("scripts/publish/scoop.json");

    // Each hash is bound to the release asset it describes.
    for (key, archive) in [
        ("win_x64_sha", "adrlint-x86_64-pc-windows-msvc.zip"),
        ("win_arm64_sha", "adrlint-aarch64-pc-windows-msvc.zip"),
    ] {
        assert!(
            config.contains(&format!(r#""{key}": "{archive}""#)),
            "scripts/publish/scoop.json must bind {key} to {archive}"
        );
        assert!(
            template.contains(&format!(r#""url": "{RELEASE_BASE}/${{tag}}/{archive}""#)),
            "the Scoop manifest must point at {archive} on the tagged release"
        );
        assert!(
            template.contains(&format!(r#""hash": "${{{key}}}""#)),
            "the Scoop manifest must take the {archive} hash from {key}"
        );
        assert!(
            template.contains(&format!("{RELEASE_BASE}/v$$version/{archive}")),
            "autoupdate must keep Scoop's $version placeholder (escaped as $$version) for {archive}"
        );
    }
    assert!(
        config.contains(r#""name": "adrlint""#)
            && config.contains(r#""template": "scripts/publish/adrlint.json.tmpl""#),
        "scripts/publish/scoop.json must render bucket/adrlint.json from the template"
    );

    for field in [
        r#""version": "${version}","#,
        r#""bin": "adrlint.exe","#,
        r#""license": "MIT","#,
        r#""homepage": "https://github.com/polylint/adrlint","#,
        r#""github": "https://github.com/polylint/adrlint""#,
        r#""arm64": {"#,
    ] {
        assert!(template.contains(field), "the Scoop manifest is missing {field}");
    }
    assert!(
        template.contains(&format!("{RELEASE_BASE}/v$$version/adrlint_$$version_checksums.txt")),
        "autoupdate.hash.url must point at the release checksums file"
    );

    // An unescaped `$version` would be substituted at render time and freeze autoupdate.
    let unescaped = template.replace("$$", "").replace("${", "");
    assert!(
        !unescaped.contains('$'),
        "the Scoop template has an unescaped `$`; write Scoop placeholders as `$$version`"
    );
}

#[test]
fn the_homebrew_formula_matches_the_published_assets() {
    let template = read("scripts/publish/adrlint.rb.tmpl");
    let config = read("scripts/publish/homebrew.json");

    for (key, archive) in [
        ("macos_arm_sha", "adrlint-aarch64-apple-darwin.tar.gz"),
        ("macos_intel_sha", "adrlint-x86_64-apple-darwin.tar.gz"),
        ("linux_arm_sha", "adrlint-aarch64-unknown-linux-gnu.tar.gz"),
        ("linux_intel_sha", "adrlint-x86_64-unknown-linux-gnu.tar.gz"),
    ] {
        assert!(
            config.contains(&format!(r#""{key}": "{archive}""#)),
            "scripts/publish/homebrew.json must bind {key} to {archive}"
        );
        let url = format!(r#"url "{RELEASE_BASE}/v#{{version}}/{archive}""#);
        let sha = format!(r#"sha256 "${{{key}}}""#);
        let url_at = line_number(&template, &url);
        assert!(
            template[url_at..].lines().nth(1).is_some_and(|line| line.trim() == sha),
            "the formula must pair {archive} with {key}"
        );
    }

    for field in [
        "class Adrlint < Formula",
        r#"homepage "https://github.com/polylint/adrlint""#,
        r#"version "${version}""#,
        r#"license "MIT""#,
        r#"bin.install "adrlint""#,
    ] {
        assert!(template.contains(field), "the formula is missing {field}");
    }
}

/// The steps shared by every job that pushes to a polylint tap or bucket.
fn assert_pushes_with_the_org_app(job: &str, repository: &str, written: &str) {
    let text = read(".github/workflows/publish.yaml");
    let block = job_block(&text, job);

    assert!(
        block.contains("actions/create-github-app-token@v3")
            && block.contains("app-id: ${{ secrets.BOT_APP_ID }}")
            && block.contains("private-key: ${{ secrets.BOT_APP_PRIVATE_KEY }}")
            && block.contains("owner: polylint")
            && block.contains(&format!("repositories: {repository}")),
        "{job} must mint an org App token scoped to polylint/{repository}"
    );
    assert!(
        block.contains(&format!("repository: polylint/{repository}")),
        "{job} must check out polylint/{repository}"
    );
    assert!(
        block.contains(r#"git config user.name "polylint-release-helper[bot]""#)
            && block.contains("+polylint-release-helper[bot]@users.noreply.github.com"),
        "{job} must commit as the org release bot"
    );
    assert!(
        block.contains(&format!("git add {written}")),
        "{job} must write {written}"
    );
    assert!(
        block.contains("if git diff --cached --quiet; then"),
        "{job} must only commit when the file changed"
    );
    assert!(
        !block.contains("git push --force") && !block.contains("git push -f"),
        "{job} must never force-push a shared tap"
    );
}

#[test]
fn the_tap_jobs_publish_only_from_a_finalized_release() {
    let text = read(".github/workflows/publish.yaml");

    for job in ["publish_homebrew", "publish_scoop"] {
        let block = job_block(&text, job);
        let needs = block
            .lines()
            .find(|line| line.trim_start().starts_with("needs:"))
            .unwrap_or_else(|| panic!("{job} declares no needs"));
        for dependency in ["meta", "verify", "finalize_release"] {
            assert!(
                needs.contains(dependency),
                "{job} must depend on `{dependency}`: {needs}"
            );
        }
        assert!(
            block.contains("!contains(needs.meta.outputs.version, '-rc')"),
            "a prerelease must not update the tap ({job})"
        );
        assert!(
            block.contains("      contents: read") && !block.contains("contents: write"),
            "{job} only reads this repository; the tap push uses the App token"
        );
    }

    let homebrew = job_block(&text, "publish_homebrew");
    assert!(
        homebrew.contains("xberg-io/actions/publish-homebrew-source-formulas@v1")
            && homebrew.contains("config-file: scripts/publish/homebrew.json"),
        "publish_homebrew must render the formula from scripts/publish/homebrew.json"
    );
    assert_pushes_with_the_org_app("publish_homebrew", "homebrew-tap", "Formula/adrlint.rb");

    let scoop = job_block(&text, "publish_scoop");
    assert!(
        scoop.contains("xberg-io/actions/publish-scoop-manifests@v1")
            && scoop.contains("config-file: scripts/publish/scoop.json"),
        "publish_scoop must render the manifest from scripts/publish/scoop.json"
    );
    assert_pushes_with_the_org_app("publish_scoop", "scoop-bucket", "bucket/adrlint.json");
}

#[test]
fn bottles_are_built_after_the_formula_and_merged_into_the_tap() {
    let text = read(".github/workflows/publish.yaml");

    let bottles = job_block(&text, "homebrew_bottles");
    assert!(
        bottles.contains("needs: [meta, verify, publish_homebrew]"),
        "bottles need the rendered formula in the tap"
    );
    for (os, tag) in [
        ("macos-26", "arm64_tahoe"),
        ("ubuntu-latest", "x86_64_linux"),
        ("ubuntu-24.04-arm", "arm64_linux"),
    ] {
        assert!(
            bottles.contains(&format!("os: {os}, bottle_tag: {tag}")),
            "homebrew_bottles must build the {tag} bottle on {os}"
        );
    }
    assert!(
        bottles.contains("xberg-io/actions/homebrew-build-bottles@v1") && bottles.contains("tap: polylint/tap"),
        "homebrew_bottles must build from polylint/tap"
    );

    let merge = job_block(&text, "publish_homebrew_bottles");
    assert!(
        merge.contains("xberg-io/actions/homebrew-merge-bottles@v1"),
        "publish_homebrew_bottles must merge the bottle DSL"
    );
    assert_pushes_with_the_org_app("publish_homebrew_bottles", "homebrew-tap", "Formula/adrlint.rb");
}

/// The npm platform packages: directory, npm `os`, `cpu`, `libc` (Linux only) and release triple.
const NPM_PLATFORMS: &[(&str, &str, &str, Option<&str>, &str)] = &[
    ("darwin-arm64", "darwin", "arm64", None, "aarch64-apple-darwin"),
    ("darwin-x64", "darwin", "x64", None, "x86_64-apple-darwin"),
    (
        "linux-arm64-gnu",
        "linux",
        "arm64",
        Some("glibc"),
        "aarch64-unknown-linux-gnu",
    ),
    (
        "linux-arm64-musl",
        "linux",
        "arm64",
        Some("musl"),
        "aarch64-unknown-linux-musl",
    ),
    (
        "linux-x64-gnu",
        "linux",
        "x64",
        Some("glibc"),
        "x86_64-unknown-linux-gnu",
    ),
    (
        "linux-x64-musl",
        "linux",
        "x64",
        Some("musl"),
        "x86_64-unknown-linux-musl",
    ),
    ("win32-arm64", "win32", "arm64", None, "aarch64-pc-windows-msvc"),
    ("win32-x64", "win32", "x64", None, "x86_64-pc-windows-msvc"),
];

fn json(relative: &str) -> serde_json::Value {
    serde_json::from_str(&read(relative)).unwrap_or_else(|error| panic!("{relative} is not JSON: {error}"))
}

fn crate_version() -> String {
    read("Cargo.toml")
        .lines()
        .find_map(|line| line.strip_prefix("version = \""))
        .and_then(|rest| rest.strip_suffix('"'))
        .expect("Cargo.toml declares a package version")
        .to_owned()
}

#[test]
fn every_npm_platform_package_is_pinned_by_the_umbrella_at_the_crate_version() {
    let version = crate_version();
    let umbrella = json("npm-package/package.json");
    let pins = umbrella["optionalDependencies"]
        .as_object()
        .expect("the umbrella declares optionalDependencies");
    assert_eq!(pins.len(), NPM_PLATFORMS.len(), "one pin per platform package");

    let on_disk = fs::read_dir(repo_root().join("npm-package/platforms"))
        .expect("npm-package/platforms exists")
        .count();
    assert_eq!(on_disk, NPM_PLATFORMS.len(), "one directory per platform package");

    let index = read("npm-package/index.js");
    let staging = read("npm-package/stage-binaries.sh");
    for &(dir, os, cpu, libc, triple) in NPM_PLATFORMS {
        let name = format!("@polylint/adrlint-{dir}");
        let manifest = json(&format!("npm-package/platforms/{dir}/package.json"));

        assert_eq!(manifest["name"], name.as_str());
        assert_eq!(
            manifest["version"],
            version.as_str(),
            "{name} must carry the crate version"
        );
        assert_eq!(pins[&name], version.as_str(), "the umbrella must pin {name} exactly");
        assert_eq!(manifest["os"], serde_json::json!([os]), "{name} os");
        assert_eq!(manifest["cpu"], serde_json::json!([cpu]), "{name} cpu");
        match libc {
            Some(libc) => assert_eq!(manifest["libc"], serde_json::json!([libc]), "{name} libc"),
            None => assert!(manifest.get("libc").is_none(), "{name} must not restrict libc"),
        }
        assert!(
            manifest["files"]
                .as_array()
                .is_some_and(|files| files.contains(&"bin/".into())),
            "{name} must publish its staged bin/ directory"
        );
        assert!(manifest.get("scripts").is_none(), "{name} must run no install script");

        let libc_js = libc.map_or_else(|| "null".to_owned(), |libc| format!("\"{libc}\""));
        assert!(
            index.contains(&format!(
                r#"{{ pkg: "{name}", platform: "{os}", arch: "{cpu}", libc: {libc_js} }}"#
            )),
            "index.js must resolve {name} on {os}/{cpu}"
        );
        assert!(
            staging.contains(&format!(r#""{dir}:{triple}""#)),
            "stage-binaries.sh must fill {name} from the {triple} archive"
        );
    }
}

#[test]
fn the_npm_launcher_prefers_the_platform_package_over_the_download() {
    let index = read("npm-package/index.js");
    assert!(
        index.contains("return resolvePlatformBinary() ?? resolveDownloadedBinary();"),
        "the platform package binary must win over the fallback download"
    );

    let launcher = read("npm-package/bin/adrlint");
    assert!(launcher.contains("resolveBinaryPath()") && launcher.contains("missingBinaryMessage()"));

    let install = read("npm-package/install.js");
    let platform_check = line_number(&install, "const platformBinary = resolvePlatformBinary();");
    assert!(
        platform_check < line_number(&install, "await installBinary();"),
        "install.js must skip the download when a platform package resolved"
    );
    assert!(
        !install.contains("process.exit(1)"),
        "a failed fallback download must not fail `npm install`"
    );
    // The archive readers load only on the download path.
    assert!(!install.lines().any(|line| line.starts_with("const tar = require(")));
    assert!(!install.lines().any(|line| line.starts_with("const AdmZip = require(")));

    let umbrella = json("npm-package/package.json");
    let files = umbrella["files"].as_array().expect("the umbrella lists its files");
    for file in ["index.js", "install.js", "bin/adrlint"] {
        assert!(files.contains(&file.into()), "the umbrella must publish {file}");
    }
}

#[test]
fn the_npm_job_publishes_the_platform_packages_before_the_umbrella() {
    let text = read(".github/workflows/publish.yaml");
    let block = job_block(&text, "publish_npm");

    let download = line_number(block, "gh release download");
    let stage = line_number(block, "./npm-package/stage-binaries.sh --artifacts artifacts");
    let platforms = line_number(block, "publish_one \"${dir%/}\"");
    let umbrella = line_number(block, "publish_one npm-package");
    assert!(download < stage && stage < platforms && platforms < umbrella);
    assert!(block.contains("npm publish --provenance --access public"));
    assert!(block.contains("id-token: write"));
    assert!(
        !block.contains("environment:"),
        "the npm trusted publisher names no environment"
    );

    let staging = read("npm-package/stage-binaries.sh");
    assert!(
        line_number(&staging, "checksum mismatch for $archive") < line_number(&staging, "tar -xzf"),
        "stage-binaries.sh must verify each archive before unpacking it"
    );
}

#[test]
fn the_pypi_job_builds_every_platform_wheel_and_the_fallback() {
    let text = read(".github/workflows/publish.yaml");
    let block = job_block(&text, "publish_pypi");
    assert!(block.contains("environment: pypi"));
    assert!(block.contains("python pip-package/build_wheels.py --artifacts artifacts --out pip-package/dist"));
    assert!(
        block.contains(r#"if [ "$count" -ne 9 ]; then"#),
        "8 platform wheels plus the fallback"
    );
    assert!(
        !block.contains("python -m build\n"),
        "no sdist: the wheels come from build_wheels.py"
    );

    let builder = read("pip-package/build_wheels.py");
    for &(_, _, _, _, triple) in NPM_PLATFORMS {
        assert!(
            builder.contains(&format!(r#"Target("{triple}")"#)),
            "build_wheels.py must build a wheel for {triple}"
        );
    }
    for tag in [
        r#"return f"macosx_{MACOS_ARM64_MIN}_arm64""#,
        r#"return f"macosx_{MACOS_X86_64_MIN}_x86_64""#,
        r#"return "win_arm64""#,
        r#"return "win_amd64""#,
        r#"return f"musllinux_{MUSLLINUX_VERSION}_{machine}""#,
        r#"return f"manylinux_2_{required_glibc_minor(binary)}_{machine}""#,
    ] {
        assert!(builder.contains(tag), "build_wheels.py must map a target onto {tag}");
    }
    assert!(
        line_number(&builder, "checksum mismatch for") < line_number(&builder, "binary = extract_binary("),
        "build_wheels.py must verify each archive before unpacking it"
    );

    let pyproject = read("pip-package/pyproject.toml");
    assert!(
        pyproject.contains("dependencies = []"),
        "platform wheels need no dependency"
    );
    assert!(pyproject.contains("[tool.hatch.build.targets.sdist]\nexclude = [\"*\"]"));
    let hook = read("pip-package/hatch_build.py");
    assert!(
        hook.contains(r#"FALLBACK_DEPENDENCIES = ["certifi"#),
        "only the fallback wheel downloads, so only it depends on certifi"
    );

    let cli = read("pip-package/adrlint/cli.py");
    assert!(
        line_number(&cli, "bundled = bundled_binary()") < line_number(&cli, "from .downloader import ensure_binary"),
        "the bundled binary must win over the fallback download"
    );
}

/// Guard: the helpers above must actually find the constructs they assert about.
#[test]
fn the_text_parsers_find_the_constructs_they_assert_about() {
    let text = read(".github/workflows/publish.yaml");
    assert!(!run_blocks(&text).is_empty());
    assert!(uses_entries(&text).contains(&"actions/checkout@v7".to_owned()));
    assert!(job_block(&text, "meta").contains("Validate the release tag"));
    assert!(job_block(&text, "publish_scoop").contains("Update Scoop manifest"));
    assert!(
        platform_maps()
            .iter()
            .any(|(name, _)| name == "scripts/gh-action-resolve.sh")
    );
    assert!(Path::new(&repo_root().join("action.yml")).is_file());
}
