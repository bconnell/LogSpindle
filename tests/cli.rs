use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_logspindle")
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.jsonl")
}

fn run(args: &[&str]) -> Output {
    Command::new(binary())
        .args(args)
        .output()
        .expect("CLI should start")
}

#[test]
fn help_describes_both_subcommands() {
    let output = run(&["--help"]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("summary"));
    assert!(text.contains("filter"));
}

#[test]
fn summary_reports_counts_and_deterministic_error_order() {
    let file = fixture();
    let output = run(&["summary", file.to_str().unwrap(), "--json"]);
    assert!(output.status.success());

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["total_lines"], 6);
    assert_eq!(value["valid_entries"], 5);
    assert_eq!(value["malformed_entries"], 1);
    assert_eq!(value["matched_entries"], 5);
    assert_eq!(value["levels"]["ERROR"], 2);
    assert_eq!(value["levels"]["FATAL"], 1);
    assert_eq!(value["top_error_codes"][0]["code"], "network_timeout");
    assert_eq!(value["top_error_codes"][0]["count"], 2);
    assert_eq!(value["top_error_codes"][1]["code"], "backend_unavailable");
    assert_eq!(value["top_error_codes"][1]["count"], 1);
    assert!(String::from_utf8_lossy(&output.stderr).contains("skipped 1 malformed"));
}

#[test]
fn filter_applies_severity_and_text() {
    let file = fixture();
    let output = run(&[
        "filter",
        file.to_str().unwrap(),
        "--min-level",
        "error",
        "--contains",
        "timeout",
    ]);
    assert!(output.status.success());

    let lines = String::from_utf8(output.stdout).unwrap();
    assert_eq!(lines.lines().count(), 2);
    for line in lines.lines() {
        let record: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(record["code"], "network_timeout");
    }
}

#[test]
fn timestamp_filter_uses_inclusive_absolute_instants() {
    let file = fixture();
    let output = run(&[
        "summary",
        file.to_str().unwrap(),
        "--json",
        "--since",
        "2026-01-01T01:00:02+01:00",
        "--until",
        "2026-01-01T00:00:03Z",
    ]);
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["matched_entries"], 2);
}

#[test]
fn strict_mode_rejects_bad_line_without_summary_output() {
    let file = fixture();
    let output = run(&["summary", file.to_str().unwrap(), "--strict", "--json"]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("line 6"));
}

#[test]
fn inverted_window_exits_with_error() {
    let file = fixture();
    let output = run(&[
        "summary",
        file.to_str().unwrap(),
        "--since",
        "2026-01-02T00:00:00Z",
        "--until",
        "2026-01-01T00:00:00Z",
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--since"));
}

#[test]
fn invalid_file_exits_with_error() {
    let output = run(&["summary", "this-file-does-not-exist.jsonl"]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn reads_stdin_without_requiring_a_file() {
    let mut child = Command::new(binary())
        .args(["summary", "--json", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    input
        .write_all(b"{\"timestamp\":\"2026-01-01T00:00:00Z\",\"level\":\"INFO\",\"message\":\"hello\"}\n")
        .unwrap();
    drop(input);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["matched_entries"], 1);
}

#[test]
fn summary_limits_top_codes() {
    let file = fixture();
    let output = run(&["summary", file.to_str().unwrap(), "--top", "1", "--json"]);
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["top_error_codes"].as_array().unwrap().len(), 1);
}

#[test]
fn invalid_timestamp_rejected_by_cli() {
    let file = fixture();
    let output = run(&["filter", file.to_str().unwrap(), "--since", "2026-01-01"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("RFC 3339"));
}
