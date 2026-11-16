use std::io::{self, Cursor, Write};
use std::process::{Command, Stdio};
use jitpipe::engine::{is_broken_pipe, StreamOptions, StreamingEngine};
use jitpipe::filter::GrepFilter;

#[test]
fn test_is_broken_pipe_detection() {
    let err_standard = io::Error::new(io::ErrorKind::BrokenPipe, "pipe broken");
    assert!(is_broken_pipe(&err_standard));

    #[cfg(windows)]
    {
        // Windows error code 109: ERROR_BROKEN_PIPE
        let err_109 = io::Error::from_raw_os_error(109);
        assert!(is_broken_pipe(&err_109));

        // Windows error code 232: ERROR_NO_DATA
        let err_232 = io::Error::from_raw_os_error(232);
        assert!(is_broken_pipe(&err_232));
    }
}

struct EarlyClosingWriter {
    capacity_bytes: usize,
    written_bytes: usize,
}

impl Write for EarlyClosingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.written_bytes + buf.len() > self.capacity_bytes {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "downstream closed"));
        }
        self.written_bytes += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.written_bytes >= self.capacity_bytes {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "downstream closed"));
        }
        Ok(())
    }
}

#[test]
fn test_streaming_engine_handles_broken_pipe_gracefully() {
    let mut input = String::new();
    for i in 0..10_000 {
        input.push_str(&format!("log line {} with matching error\n", i));
    }

    let filter = GrepFilter::new("error", false, false).expect("filter");
    let writer = EarlyClosingWriter {
        capacity_bytes: 120, // allows ~3 lines then closes
        written_bytes: 0,
    };

    let cursor = Cursor::new(input);
    let options = StreamOptions::default();

    let summary = StreamingEngine::stream(Vec::new(), cursor, false, &filter, writer, &options)
        .expect("should not return Err on broken pipe");

    assert!(summary.broken_pipe);
    assert!(summary.lines_matched > 0);
}

#[test]
fn test_cli_broken_pipe_subprocess_exit_zero() {
    // Locate the built sgrep binary
    let sgrep_bin = assert_cmd::cargo::cargo_bin("sgrep");

    // Spawn Python consumer that reads 3 lines then immediately exits
    let mut python_consumer = Command::new("python")
        .arg("-c")
        .arg("import sys; [sys.stdin.readline() for _ in range(3)]; sys.exit(0)")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn python consumer");

    let python_in = python_consumer.stdin.take().expect("python stdin");

    // Spawn producer with 100,000 lines
    let mut producer_input = String::new();
    for i in 0..50_000 {
        producer_input.push_str(&format!("line {} error occurrence\n", i));
    }

    let mut producer = Command::new(&sgrep_bin)
        .arg("--mock")
        .arg("error")
        .stdin(Stdio::piped())
        .stdout(python_in)
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn sgrep");

    if let Some(mut sgrep_in) = producer.stdin.take() {
        let _ = sgrep_in.write_all(producer_input.as_bytes());
        let _ = sgrep_in.flush();
    }

    let sgrep_status = producer.wait().expect("wait sgrep");
    let python_status = python_consumer.wait().expect("wait python");

    assert!(python_status.success());
    // Crucial requirement: must exit with code 0 without panicking
    assert!(sgrep_status.success(), "sgrep exited with {:?}", sgrep_status);
}
