use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use assert_cmd::Command;
use predicates::prelude::predicate;

#[test]
fn views_markdown_file() {
    let fixture = workspace_fixture("tests/fixtures/sample.md");

    Command::cargo_bin("md")
        .expect("md binary should exist")
        .arg("view")
        .arg(&fixture)
        .arg("--no-pager")
        .assert()
        .success()
        .stdout(predicate::str::contains("Sample Document"));
}

#[test]
fn views_markdown_file_with_direct_invocation() {
    let fixture = workspace_fixture("tests/fixtures/sample.md");

    Command::cargo_bin("md")
        .expect("md binary should exist")
        .arg(&fixture)
        .arg("--no-pager")
        .assert()
        .success()
        .stdout(predicate::str::contains("Sample Document"));
}

#[test]
fn supports_explicit_pager_command() {
    let fixture = workspace_fixture("tests/fixtures/sample.md");

    Command::cargo_bin("md")
        .expect("md binary should exist")
        .arg("view")
        .arg(&fixture)
        .arg("--pager")
        .arg("cat")
        .assert()
        .success()
        .stdout(predicate::str::contains("Sample Document"));
}

#[test]
fn renders_mermaid_from_markdown_file() {
    let fixture = workspace_fixture("tests/fixtures/big.md");

    Command::cargo_bin("md")
        .expect("md binary should exist")
        .arg(&fixture)
        .arg("--no-pager")
        .arg("--plain")
        .assert()
        .success()
        .stdout(predicate::str::contains("Write"))
        .stdout(predicate::str::contains("Preview"))
        .stdout(predicate::str::contains("Commit"))
        .stdout(predicate::str::contains("parser"));
}

#[test]
fn returns_error_for_missing_file() {
    let missing_file = workspace_fixture("tests/fixtures/missing.md");

    Command::cargo_bin("md")
        .expect("md binary should exist")
        .arg("view")
        .arg(&missing_file)
        .arg("--no-pager")
        .assert()
        .failure()
        .stderr(predicate::str::contains("failed to read markdown file"));
}

#[test]
fn matches_plain_output_snapshot() {
    let fixture = workspace_fixture("tests/fixtures/sample.md");
    let expected_snapshot =
        fs::read_to_string(workspace_fixture("tests/fixtures/expected-cli/basic.txt"))
            .expect("expected CLI snapshot should exist");

    let assert = Command::cargo_bin("md")
        .expect("md binary should exist")
        .arg("view")
        .arg(&fixture)
        .arg("--no-color")
        .arg("--no-pager")
        .assert()
        .success();

    let output = String::from_utf8(assert.get_output().stdout.clone())
        .expect("output should be valid utf-8");
    assert_eq!(expected_snapshot, output);
}

#[test]
fn direct_cli_renders_emoji_and_math_in_plain_mode() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after epoch")
        .as_nanos();
    let fixture = std::env::temp_dir().join(format!("mdstar-rich-render-{unique}.md"));
    fs::write(
        &fixture,
        r#"# Rich rendering

Emoji: :wink: 8-)

Math: \(x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}\)
"#,
    )
    .expect("temporary markdown fixture should be writable");

    let assert = Command::cargo_bin("md")
        .expect("md binary should exist")
        .arg(&fixture)
        .arg("--plain")
        .arg("--no-pager")
        .assert()
        .success();

    let output = String::from_utf8(assert.get_output().stdout.clone())
        .expect("output should be valid utf-8");
    let _ = fs::remove_file(&fixture);

    assert!(output.contains('😉'), "got: {output}");
    assert!(output.contains('😎'), "got: {output}");
    assert!(output.contains('±'), "got: {output}");
    assert!(output.contains('√'), "got: {output}");
    assert!(output.contains("b²"), "got: {output}");
    assert!(!output.contains(r"\frac"), "got: {output}");
}

fn workspace_fixture(relative: &str) -> String {
    format!("{}/../../{relative}", env!("CARGO_MANIFEST_DIR"))
}
