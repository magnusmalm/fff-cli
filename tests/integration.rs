//! Integration tests for fff-cli.
//!
//! These tests create temporary directories with known file structures,
//! run the fff binary, and verify output.

use std::path::Path;
use std::process::Command;

fn fff() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fff"));
    // Suppress auto-index stderr noise in test output.
    cmd.stderr(std::process::Stdio::piped());
    cmd
}

fn create_test_repo(dir: &Path) {
    std::fs::create_dir_all(dir.join("src/controllers")).unwrap();
    std::fs::create_dir_all(dir.join("src/models")).unwrap();
    std::fs::create_dir_all(dir.join("tests")).unwrap();

    std::fs::write(dir.join("src/main.rs"), "fn main() {\n    println!(\"hello\");\n}\n").unwrap();
    std::fs::write(dir.join("src/lib.rs"), "pub mod controllers;\npub mod models;\n").unwrap();
    std::fs::write(
        dir.join("src/controllers/user.rs"),
        "pub struct UserController;\nimpl UserController {\n    pub fn create() {}\n}\n",
    ).unwrap();
    std::fs::write(
        dir.join("src/controllers/auth.rs"),
        "pub fn authenticate(token: &str) -> bool {\n    !token.is_empty()\n}\n",
    ).unwrap();
    std::fs::write(
        dir.join("src/models/user.rs"),
        "pub struct User {\n    pub name: String,\n    pub email: String,\n}\n",
    ).unwrap();
    std::fs::write(dir.join("tests/test_auth.rs"), "use crate::controllers::auth;\n#[test]\nfn test_auth() {}\n").unwrap();
    std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"test\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(dir.join("README.md"), "# Test Project\n").unwrap();

    // Init a git repo so fff can discover root and .gitignore works.
    Command::new("git").args(["init"]).current_dir(dir).output().unwrap();
    Command::new("git").args(["add", "."]).current_dir(dir).output().unwrap();
    Command::new("git")
        .args(["commit", "-m", "init", "--allow-empty"])
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@test")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@test")
        .output()
        .unwrap();
}

// ── Index ──────────────────────────────────────────────────────────────

#[test]
fn test_index_creates_files() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());

    let output = fff()
        .args(["index", "-C", dir.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success(), "index failed: {:?}", output);

    assert!(dir.path().join(".fff/manifest.bin").exists());
    assert!(dir.path().join(".fff/files.bin").exists());
    assert!(dir.path().join(".fff/bigram.bin").exists());
}

#[test]
fn test_index_force_rebuild() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());

    // Build once.
    fff().args(["index", "-C", dir.path().to_str().unwrap()]).output().unwrap();
    let mtime1 = std::fs::metadata(dir.path().join(".fff/manifest.bin"))
        .unwrap().modified().unwrap();

    std::thread::sleep(std::time::Duration::from_millis(50));

    // Force rebuild.
    fff().args(["index", "--force", "-C", dir.path().to_str().unwrap()]).output().unwrap();
    let mtime2 = std::fs::metadata(dir.path().join(".fff/manifest.bin"))
        .unwrap().modified().unwrap();

    assert!(mtime2 > mtime1, "force rebuild should update manifest");
}

// ── Search ─────────────────────────────────────────────────────────────

#[test]
fn test_search_finds_exact_filename() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());

    let output = fff()
        .args(["search", "main.rs", "-C", dir.path().to_str().unwrap(), "-n", "5"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("src/main.rs"), "should find main.rs, got: {stdout}");
    assert!(output.status.success());
}

#[test]
fn test_search_typo_tolerance() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());

    let output = fff()
        .args(["search", "contorllers", "-C", dir.path().to_str().unwrap(), "-n", "5"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("controllers"),
        "should find controllers despite typo, got: {stdout}"
    );
}

#[test]
fn test_search_no_match_exits_1() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());

    let output = fff()
        .args(["search", "ZZZZZZZZZZZZZ", "-C", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1), "no match should exit 1");
}

#[test]
fn test_search_json_output() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());

    let output = fff()
        .args(["search", "main", "--json", "-C", dir.path().to_str().unwrap(), "-n", "1"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(stdout.lines().next().unwrap()).unwrap();
    assert!(parsed["path"].as_str().unwrap().contains("main"));
    assert!(parsed["score"].as_i64().unwrap() > 0);
}

#[test]
fn test_implicit_search() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());

    // No subcommand — bare positional should work as search.
    let output = fff()
        .args(["main.rs", "-C", dir.path().to_str().unwrap(), "-n", "3"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("main.rs"), "implicit search should work, got: {stdout}");
}

// ── Grep ───────────────────────────────────────────────────────────────

#[test]
fn test_grep_literal() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());

    let output = fff()
        .args(["grep", "authenticate", "-C", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("auth.rs"), "should find in auth.rs, got: {stdout}");
    assert!(stdout.contains("authenticate"), "should show the match");
}

#[test]
fn test_grep_regex() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());

    let output = fff()
        .args(["grep", "-e", r"pub\s+struct\s+\w+", "-C", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("UserController"), "should find struct via regex, got: {stdout}");
    assert!(stdout.contains("User"), "should find User struct");
}

#[test]
fn test_grep_no_match_exits_1() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());

    let output = fff()
        .args(["grep", "NONEXISTENT_PATTERN_12345", "-C", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn test_grep_json_output() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());

    let output = fff()
        .args(["grep", "authenticate", "--json", "-C", dir.path().to_str().unwrap(), "-n", "1"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(stdout.lines().next().unwrap()).unwrap();
    assert!(parsed["path"].as_str().unwrap().contains("auth.rs"));
    assert!(parsed["line_number"].as_u64().unwrap() > 0);
}

// ── Filter ─────────────────────────────────────────────────────────────

#[test]
fn test_filter_from_stdin() {
    let output = Command::new(env!("CARGO_BIN_EXE_fff"))
        .args(["--filter", "main", "-n", "5"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            let stdin = child.stdin.as_mut().unwrap();
            writeln!(stdin, "src/main.rs")?;
            writeln!(stdin, "src/lib.rs")?;
            writeln!(stdin, "src/controllers/auth.rs")?;
            writeln!(stdin, "README.md")?;
            child.wait_with_output()
        })
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.lines().next().unwrap().contains("main"), "first result should match 'main'");
}

#[test]
fn test_filter_no_match_exits_1() {
    let output = Command::new(env!("CARGO_BIN_EXE_fff"))
        .args(["--filter", "ZZZZZZZ", "-n", "5"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            let stdin = child.stdin.as_mut().unwrap();
            writeln!(stdin, "foo.txt")?;
            writeln!(stdin, "bar.txt")?;
            child.wait_with_output()
        })
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
}

// ── Index round-trip ───────────────────────────────────────────────────

#[test]
fn test_index_roundtrip_preserves_file_count() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());

    // Index.
    fff().args(["index", "-C", dir.path().to_str().unwrap()]).output().unwrap();

    // Search with high limit to get all files.
    let output = fff()
        .args(["search", "", "--json", "-C", dir.path().to_str().unwrap(), "-n", "100"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let count = stdout.lines().count();
    // We created 8 files in create_test_repo.
    assert!(count >= 7, "should find most files, got {count}");
}
