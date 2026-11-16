use std::io::{BufRead, Write};

use crate::engine::pipe::is_broken_pipe;
use crate::error::JitPipeError;
use crate::filter::CompiledFilter;

/// Execution configuration options for the streaming engine.
#[derive(Debug, Clone, Default)]
pub struct StreamOptions {
    /// Prefix each matching line with its 1-based line number (`-n`).
    pub line_numbers: bool,
    /// Suppress normal output; print total count of matching lines (`-c`).
    pub count_only: bool,
    /// Stop streaming after reaching max match count (`-m <NUM>`).
    pub max_count: Option<usize>,
    /// Quiet mode: emit nothing to stdout, exit immediately on match (`-q`).
    pub quiet: bool,
}

/// Statistics and termination summary returned by the streaming engine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StreamSummary {
    /// Total lines read from both sample buffer and remainder stream.
    pub lines_read: usize,
    /// Total lines matching the filter criteria.
    pub lines_matched: usize,
    /// Whether streaming ended prematurely due to a broken pipe.
    pub broken_pipe: bool,
}

/// Zero-allocation line streaming engine.
pub struct StreamingEngine;

impl StreamingEngine {
    /// Streams buffered sample rows followed by remaining input rows through the compiled filter.
    /// Reuses a single pre-allocated String buffer for remainder streaming.
    pub fn stream<R: BufRead, W: Write, F: CompiledFilter>(
        buffered_lines: Vec<String>,
        mut reader: R,
        eof_reached: bool,
        filter: &F,
        mut writer: W,
        options: &StreamOptions,
    ) -> Result<StreamSummary, JitPipeError> {
        let mut summary = StreamSummary::default();
        let mut current_line_num: usize = 0;

        // Phase 1: Uncork buffered sample rows in exact sequential order
        for raw_line in buffered_lines {
            summary.lines_read += 1;
            current_line_num += 1;

            let line = raw_line.trim_end_matches(['\r', '\n']);
            if let Some(output) = filter.apply(line) {
                summary.lines_matched += 1;

                if options.quiet {
                    return Ok(summary);
                }

                if !options.count_only {
                    if let Err(err) = Self::write_output(&mut writer, current_line_num, &output, options.line_numbers) {
                        if is_broken_pipe(&err) {
                            summary.broken_pipe = true;
                            return Ok(summary);
                        }
                        return Err(JitPipeError::Io(err));
                    }
                }

                if let Some(max) = options.max_count {
                    if summary.lines_matched >= max {
                        Self::flush_writer(&mut writer, &mut summary)?;
                        return Ok(summary);
                    }
                }
            }
        }

        // Phase 2: Zero-allocation streaming loop for remainder rows
        if !eof_reached {
            let mut line_buf = String::with_capacity(1024);

            loop {
                line_buf.clear();
                let bytes_read = match reader.read_line(&mut line_buf) {
                    Ok(b) => b,
                    Err(err) => {
                        if is_broken_pipe(&err) {
                            summary.broken_pipe = true;
                            return Ok(summary);
                        }
                        return Err(JitPipeError::Io(err));
                    }
                };

                if bytes_read == 0 {
                    break;
                }

                summary.lines_read += 1;
                current_line_num += 1;

                let line = line_buf.trim_end_matches(['\r', '\n']);
                if let Some(output) = filter.apply(line) {
                    summary.lines_matched += 1;

                    if options.quiet {
                        return Ok(summary);
                    }

                    if !options.count_only {
                        if let Err(err) = Self::write_output(&mut writer, current_line_num, &output, options.line_numbers) {
                            if is_broken_pipe(&err) {
                                summary.broken_pipe = true;
                                return Ok(summary);
                            }
                            return Err(JitPipeError::Io(err));
                        }
                    }

                    if let Some(max) = options.max_count {
                        if summary.lines_matched >= max {
                            break;
                        }
                    }
                }
            }
        }

        // Emit match count if -c was specified
        if options.count_only && !options.quiet && !summary.broken_pipe {
            let count_str = format!("{}\n", summary.lines_matched);
            if let Err(err) = writer.write_all(count_str.as_bytes()) {
                if is_broken_pipe(&err) {
                    summary.broken_pipe = true;
                    return Ok(summary);
                }
                return Err(JitPipeError::Io(err));
            }
        }

        Self::flush_writer(&mut writer, &mut summary)?;
        Ok(summary)
    }

    #[inline(always)]
    fn write_output<W: Write>(
        writer: &mut W,
        line_num: usize,
        content: &str,
        with_line_numbers: bool,
    ) -> std::io::Result<()> {
        if with_line_numbers {
            write!(writer, "{}:", line_num)?;
        }
        writer.write_all(content.as_bytes())?;
        writer.write_all(b"\n")?;
        Ok(())
    }

    #[inline(always)]
    fn flush_writer<W: Write>(writer: &mut W, summary: &mut StreamSummary) -> Result<(), JitPipeError> {
        if !summary.broken_pipe {
            if let Err(err) = writer.flush() {
                if is_broken_pipe(&err) {
                    summary.broken_pipe = true;
                } else {
                    return Err(JitPipeError::Io(err));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::grep::GrepFilter;
    use std::io::Cursor;

    #[test]
    fn test_engine_uncork_and_stream() {
        let buffered = vec!["line 1: match\n".to_string(), "line 2: skip\n".to_string()];
        let remainder = Cursor::new("line 3: match\nline 4: skip\n");
        let filter = GrepFilter::new("match", false, false).expect("filter");
        let mut output = Vec::new();

        let options = StreamOptions::default();
        let summary = StreamingEngine::stream(buffered, remainder, false, &filter, &mut output, &options).expect("stream");

        assert_eq!(summary.lines_read, 4);
        assert_eq!(summary.lines_matched, 2);
        assert!(!summary.broken_pipe);

        let out_str = String::from_utf8(output).expect("utf8");
        assert_eq!(out_str, "line 1: match\nline 3: match\n");
    }

    #[test]
    fn test_engine_line_numbers() {
        let buffered = vec!["apple\n".to_string()];
        let remainder = Cursor::new("banana\ncherry\n");
        let filter = GrepFilter::new(".*", false, false).expect("filter");
        let mut output = Vec::new();

        let options = StreamOptions {
            line_numbers: true,
            ..Default::default()
        };
        StreamingEngine::stream(buffered, remainder, false, &filter, &mut output, &options).expect("stream");

        let out_str = String::from_utf8(output).expect("utf8");
        assert_eq!(out_str, "1:apple\n2:banana\n3:cherry\n");
    }

    #[test]
    fn test_engine_count_only() {
        let buffered = vec!["match 1\n".to_string(), "skip\n".to_string()];
        let remainder = Cursor::new("match 2\nmatch 3\n");
        let filter = GrepFilter::new("match", false, false).expect("filter");
        let mut output = Vec::new();

        let options = StreamOptions {
            count_only: true,
            ..Default::default()
        };
        StreamingEngine::stream(buffered, remainder, false, &filter, &mut output, &options).expect("stream");

        let out_str = String::from_utf8(output).expect("utf8");
        assert_eq!(out_str, "3\n");
    }

    #[test]
    fn test_engine_max_count() {
        let buffered = vec!["m1\n".to_string(), "m2\n".to_string()];
        let remainder = Cursor::new("m3\nm4\nm5\n");
        let filter = GrepFilter::new("m", false, false).expect("filter");
        let mut output = Vec::new();

        let options = StreamOptions {
            max_count: Some(2),
            ..Default::default()
        };
        let summary = StreamingEngine::stream(buffered, remainder, false, &filter, &mut output, &options).expect("stream");

        assert_eq!(summary.lines_matched, 2);
        let out_str = String::from_utf8(output).expect("utf8");
        assert_eq!(out_str, "m1\nm2\n");
    }

    struct FailingWriter {
        written: usize,
        fail_after: usize,
    }

    impl Write for FailingWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if self.written >= self.fail_after {
                return Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "broken pipe"));
            }
            self.written += buf.len();
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            if self.written >= self.fail_after {
                return Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "broken pipe"));
            }
            Ok(())
        }
    }

    #[test]
    fn test_engine_broken_pipe_handling() {
        let buffered = vec!["row 1\n".to_string(), "row 2\n".to_string()];
        let remainder = Cursor::new("row 3\nrow 4\nrow 5\n");
        let filter = GrepFilter::new("row", false, false).expect("filter");
        let writer = FailingWriter {
            written: 0,
            fail_after: 8, // fails on second row
        };

        let options = StreamOptions::default();
        let summary = StreamingEngine::stream(buffered, remainder, false, &filter, writer, &options).expect("stream");

        assert!(summary.broken_pipe);
    }
}
