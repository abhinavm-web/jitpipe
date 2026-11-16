use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::NamedTempFile;

#[test]
fn test_sawk_column_projection() {
    let input = "192.168.1.1 POST /api/login 200 45ms\n10.0.0.1 GET /health 500 120ms\n";

    let mut cmd = Command::cargo_bin("sawk").expect("binary");
    cmd.arg("--mock")
        .arg("1 4")
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("192.168.1.1\t200"))
        .stdout(predicate::str::contains("10.0.0.1\t500"));
}

#[test]
fn test_sawk_csv_field_separator() {
    let input = "alice,admin,active\nbob,user,pending\n";

    let mut cmd = Command::cargo_bin("sawk").expect("binary");
    cmd.arg("-F,")
        .arg("-O|")
        .arg("--mock")
        .arg("1 3")
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("alice|active"))
        .stdout(predicate::str::contains("bob|pending"));
}

#[test]
fn test_sawk_template_with_nf() {
    let input = "alpha beta gamma delta\none two three four\n";

    let mut cmd = Command::cargo_bin("sawk").expect("binary");
    cmd.arg("--mock")
        .arg("$1 -> $NF")
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("alpha -> delta"))
        .stdout(predicate::str::contains("one -> four"));
}

#[test]
fn test_sawk_out_of_bounds_safety() {
    let input = "col1 col2\nrow1 row2\n";

    let mut cmd = Command::cargo_bin("sawk").expect("binary");
    cmd.arg("--mock")
        .arg("$1\t$99")
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("col1\t\n"))
        .stdout(predicate::str::contains("row1\t\n"));
}

#[test]
fn test_sawk_condition_filtering() {
    let input = "node1 /api 120\nnode2 /api 850\nnode3 /api 340\nnode4 /api 920\n";

    let mut cmd = Command::cargo_bin("sawk").expect("binary");
    cmd.arg("--mock")
        .arg("1 3 > 500")
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("node2\t850"))
        .stdout(predicate::str::contains("node4\t920"))
        .stdout(predicate::str::contains("node1").not())
        .stdout(predicate::str::contains("node3").not());
}

#[test]
fn test_sawk_save_and_load_filter() {
    let temp_file = NamedTempFile::new().expect("tempfile");
    let cache_path = temp_file.path().to_str().expect("path").to_string();

    let input1 = "user1 admin\nuser2 guest\n";

    let mut cmd_save = Command::cargo_bin("sawk").expect("binary");
    cmd_save
        .arg("--save-filter")
        .arg(&cache_path)
        .arg("--mock")
        .arg("$1:$2")
        .write_stdin(input1)
        .assert()
        .success()
        .stdout(predicate::str::contains("user1:admin"));

    let input2 = "alice developer\nbob tester\n";
    let mut cmd_load = Command::cargo_bin("sawk").expect("binary");
    cmd_load
        .arg("--load-filter")
        .arg(&cache_path)
        .write_stdin(input2)
        .assert()
        .success()
        .stdout(predicate::str::contains("alice:developer"))
        .stdout(predicate::str::contains("bob:tester"));
}

#[test]
fn test_sawk_large_streaming() {
    let mut input = String::new();
    for i in 1..=100 {
        input.push_str(&format!("prefix_{} data_{}\n", i, i));
    }

    let mut cmd = Command::cargo_bin("sawk").expect("binary");
    cmd.arg("--mock")
        .arg("2")
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("data_1\n"))
        .stdout(predicate::str::contains("data_100\n"));
}
