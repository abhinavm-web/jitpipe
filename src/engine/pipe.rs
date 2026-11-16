use std::io::{self, BufWriter, StdoutLock, Write};

/// Tuned default buffer capacity (128 KB) for high-throughput pipeline streaming.
pub const DEFAULT_BUFFER_SIZE: usize = 128 * 1024;

/// Creates a tuned 128 KB BufWriter wrapping locked stdout.
pub fn create_stdout_writer() -> BufWriter<StdoutLock<'static>> {
    let stdout = Box::leak(Box::new(io::stdout()));
    BufWriter::with_capacity(DEFAULT_BUFFER_SIZE, stdout.lock())
}

/// Creates a tuned BufWriter for any given writer with custom capacity.
pub fn create_buffered_writer<W: Write>(writer: W, capacity: usize) -> BufWriter<W> {
    BufWriter::with_capacity(capacity, writer)
}

/// Detects whether an I/O error corresponds to a broken pipe across platforms.
/// Intercepts POSIX EPIPE/SIGPIPE as well as Windows error codes 109 (ERROR_BROKEN_PIPE)
/// and 232 (ERROR_NO_DATA).
pub fn is_broken_pipe(err: &io::Error) -> bool {
    if err.kind() == io::ErrorKind::BrokenPipe {
        return true;
    }

    #[cfg(windows)]
    {
        if let Some(code) = err.raw_os_error() {
            if code == 109 || code == 232 {
                return true;
            }
        }
    }

    false
}
