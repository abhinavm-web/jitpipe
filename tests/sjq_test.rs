use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::NamedTempFile;

#[test]
fn test_sjq_dot_path_raw_output() {
    let input = "{\"user\":{\"id\":\"usr_01\",\"name\":\"Alice\"}}\n{\"user\":{\"id\":\"usr_02\",\"name\":\"Bob\"}}\n";

    let mut cmd = Command::cargo_bin("sjq").expect("binary");
    cmd.arg("-r")
        .arg("--mock")
        .arg(".user.name")
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("Alice\nBob\n"));
}

#[test]
fn test_sjq_field_projection() {
    let input = "{\"id\":1,\"name\":\"Alice\",\"token\":\"secret1\"}\n{\"id\":2,\"name\":\"Bob\",\"token\":\"secret2\"}\n";

    let mut cmd = Command::cargo_bin("sjq").expect("binary");
    cmd.arg("--mock")
        .arg("{id, name}")
        .write_stdin(input)
        .assert()
        .success();

    let output = cmd.output().expect("output");
    let stdout = String::from_utf8(output.stdout).expect("utf8");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2);

    let val1: Value = serde_json::from_str(lines[0]).expect("json");
    assert_eq!(val1["id"], 1);
    assert_eq!(val1["name"], "Alice");
    assert!(val1.get("token").is_none());
}

#[test]
fn test_sjq_predicate_filtering() {
    let input = "{\"service\":\"auth\",\"latency\":120}\n{\"service\":\"db\",\"latency\":850}\n{\"service\":\"cache\",\"latency\":40}\n{\"service\":\"search\",\"latency\":620}\n";

    let mut cmd = Command::cargo_bin("sjq").expect("binary");
    cmd.arg("-r")
        .arg("--mock")
        .arg(".service > 500")
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("db"))
        .stdout(predicate::str::contains("search"))
        .stdout(predicate::str::contains("auth").not())
        .stdout(predicate::str::contains("cache").not());
}

#[test]
fn test_sjq_array_unroll() {
    let input = "{\"items\":[\"apple\",\"banana\",\"cherry\"]}\n{\"items\":[\"date\",\"elderberry\"]}\n";

    let mut cmd = Command::cargo_bin("sjq").expect("binary");
    cmd.arg("-r")
        .arg("--mock")
        .arg(".items[]")
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("apple\nbanana\ncherry\ndate\nelderberry\n"));
}

#[test]
fn test_sjq_ignore_malformed() {
    let input = "{\"val\": 10}\nthis is a stack trace\n{\"val\": 20}\n";

    let mut cmd = Command::cargo_bin("sjq").expect("binary");
    cmd.arg("-r")
        .arg("--ignore-malformed")
        .arg("--mock")
        .arg(".val")
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("10\n20\n"));
}

#[test]
fn test_sjq_save_and_load_filter() {
    let temp_file = NamedTempFile::new().expect("tempfile");
    let cache_path = temp_file.path().to_str().expect("path").to_string();

    let input1 = "{\"host\":\"web-1\",\"status\":200}\n";

    let mut cmd_save = Command::cargo_bin("sjq").expect("binary");
    cmd_save
        .arg("-r")
        .arg("--save-filter")
        .arg(&cache_path)
        .arg("--mock")
        .arg(".host")
        .write_stdin(input1)
        .assert()
        .success()
        .stdout(predicate::str::contains("web-1"));

    let input2 = "{\"host\":\"api-2\",\"status\":500}\n{\"host\":\"db-3\",\"status\":200}\n";
    let mut cmd_load = Command::cargo_bin("sjq").expect("binary");
    cmd_load
        .arg("--load-filter")
        .arg(&cache_path)
        .write_stdin(input2)
        .assert()
        .success()
        .stdout(predicate::str::contains("api-2"))
        .stdout(predicate::str::contains("db-3"));
}

#[test]
fn test_sjq_large_stream_sampler_and_streaming() {
    let mut input = String::new();
    for i in 1..=100 {
        input.push_str(&format!("{{\"index\":{i},\"active\":true}}\n"));
    }

    let mut cmd = Command::cargo_bin("sjq").expect("binary");
    cmd.arg("-r")
        .arg("--mock")
        .arg(".index")
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("1\n"))
        .stdout(predicate::str::contains("100\n"));
}
