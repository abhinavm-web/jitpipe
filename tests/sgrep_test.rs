use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::NamedTempFile;

#[test]
fn test_sgrep_basic_matching() {
    let input = "2026-11-16 [INFO] Started\n2026-11-16 [ERROR] Failed to bind port\n2026-11-16 [INFO] Retrying\n";

    let mut cmd = Command::cargo_bin("sgrep").expect("binary");
    cmd.arg("--mock")
        .arg("error")
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("2026-11-16 [ERROR] Failed to bind port"))
        .stdout(predicate::str::contains("[INFO]").not());
}

#[test]
fn test_sgrep_ignore_case() {
    let input = "error one\nERROR TWO\nclean line\n";

    let mut cmd = Command::cargo_bin("sgrep").expect("binary");
    cmd.arg("-i")
        .arg("--mock")
        .arg("error")
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("error one"))
        .stdout(predicate::str::contains("ERROR TWO"));
}

#[test]
fn test_sgrep_invert_match() {
    let input = "error one\nclean line\n";

    let mut cmd = Command::cargo_bin("sgrep").expect("binary");
    cmd.arg("-v")
        .arg("--mock")
        .arg("error")
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("clean line"))
        .stdout(predicate::str::contains("error one").not());
}

#[test]
fn test_sgrep_line_numbers() {
    let input = "alpha\nbeta error\ngamma\n";

    let mut cmd = Command::cargo_bin("sgrep").expect("binary");
    cmd.arg("-n")
        .arg("--mock")
        .arg("error")
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("2:beta error"));
}

#[test]
fn test_sgrep_count_only() {
    let input = "error 1\nerror 2\ninfo 1\nerror 3\n";

    let mut cmd = Command::cargo_bin("sgrep").expect("binary");
    cmd.arg("-c")
        .arg("--mock")
        .arg("error")
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("3\n"));
}

#[test]
fn test_sgrep_max_count() {
    let input = "match 1\nmatch 2\nmatch 3\nmatch 4\n";

    let mut cmd = Command::cargo_bin("sgrep").expect("binary");
    cmd.arg("-m")
        .arg("2")
        .arg("--mock")
        .arg("match")
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("match 1\nmatch 2\n"))
        .stdout(predicate::str::contains("match 3").not());
}

#[test]
fn test_sgrep_quiet_mode() {
    // Match exists -> exit 0, empty stdout
    let mut cmd_match = Command::cargo_bin("sgrep").expect("binary");
    cmd_match
        .arg("-q")
        .arg("--mock")
        .arg("error")
        .write_stdin("an error happened\n")
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    // No match -> exit 1, empty stdout
    let mut cmd_no_match = Command::cargo_bin("sgrep").expect("binary");
    cmd_no_match
        .arg("-q")
        .arg("--mock")
        .arg("error")
        .write_stdin("everything is clean\n")
        .assert()
        .code(1)
        .stdout(predicate::str::is_empty());
}

#[test]
fn test_sgrep_empty_stream() {
    let mut cmd = Command::cargo_bin("sgrep").expect("binary");
    cmd.arg("--mock")
        .arg("error")
        .write_stdin("")
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn test_sgrep_large_stream_sampling_and_streaming() {
    // Stream of 100 lines: 25 sample lines + 75 remainder lines
    let mut input = String::new();
    for i in 1..=100 {
        if i % 10 == 0 {
            input.push_str(&format!("row {} error found\n", i));
        } else {
            input.push_str(&format!("row {} ok\n", i));
        }
    }

    let mut cmd = Command::cargo_bin("sgrep").expect("binary");
    cmd.arg("-c")
        .arg("--mock")
        .arg("error")
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("10\n"));
}

#[test]
fn test_sgrep_save_and_load_filter() {
    let temp_file = NamedTempFile::new().expect("tempfile");
    let cache_path = temp_file.path().to_str().expect("path").to_string();

    let input1 = "row 500 error\nrow 200 ok\n";

    // Step 1: synthesize and save filter
    let mut cmd_save = Command::cargo_bin("sgrep").expect("binary");
    cmd_save
        .arg("--save-filter")
        .arg(&cache_path)
        .arg("--mock")
        .arg("5xx")
        .write_stdin(input1)
        .assert()
        .success()
        .stdout(predicate::str::contains("row 500 error"));

    // Step 2: load filter and stream new data without synthesis
    let input2 = "row 503 gateway timeout\nrow 200 ok\nrow 502 bad gateway\n";
    let mut cmd_load = Command::cargo_bin("sgrep").expect("binary");
    cmd_load
        .arg("--load-filter")
        .arg(&cache_path)
        .write_stdin(input2)
        .assert()
        .success()
        .stdout(predicate::str::contains("503 gateway timeout"))
        .stdout(predicate::str::contains("502 bad gateway"))
        .stdout(predicate::str::contains("200 ok").not());
}

#[test]
fn test_unified_jitpipe_dispatcher() {
    let input = "line with error\nline with info\n";

    let mut cmd = Command::cargo_bin("jitpipe").expect("binary");
    cmd.arg("sgrep")
        .arg("--mock")
        .arg("error")
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("line with error"));
}
