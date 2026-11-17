use std::io::{self, BufRead};

pub const DEFAULT_SAMPLE_SIZE: usize = 25;

/// Reservoir and prefix sampler that captures initial rows from a stream
/// while preserving the reader's state for subsequent streaming.
pub struct StreamSampler<R: BufRead> {
    reader: R,
    max_samples: usize,
    samples: Vec<String>,
    eof_reached: bool,
}

impl<R: BufRead> StreamSampler<R> {
    /// Creates a new sampler with default 25-row capacity.
    pub fn new(reader: R) -> Self {
        Self::with_sample_size(reader, DEFAULT_SAMPLE_SIZE)
    }

    /// Creates a new sampler with custom sample size.
    pub fn with_sample_size(reader: R, max_samples: usize) -> Self {
        Self {
            reader,
            max_samples,
            samples: Vec::with_capacity(max_samples),
            eof_reached: false,
        }
    }

    /// Reads up to max_samples lines from the reader.
    /// Terminates cleanly on EOF without blocking.
    /// Returns the number of sample lines collected.
    pub fn collect_samples(&mut self) -> io::Result<usize> {
        let mut line_buf = String::with_capacity(1024);
        while self.samples.len() < self.max_samples {
            line_buf.clear();
            let bytes_read = self.reader.read_line(&mut line_buf)?;
            if bytes_read == 0 {
                self.eof_reached = true;
                break;
            }
            if self.samples.is_empty() && line_buf.starts_with('\u{feff}') {
                let stripped = line_buf.trim_start_matches('\u{feff}').to_string();
                self.samples.push(stripped);
            } else {
                self.samples.push(line_buf.clone());
            }
        }
        Ok(self.samples.len())
    }

    /// Collects initial samples and, when running with the default sample window and
    /// none of the first rows match query keywords, scans ahead up to `max_extra_lines`
    /// (buffering every read line so `into_parts()` remains lossless) to surface rare events
    /// for filter synthesis.
    pub fn collect_samples_with_lookahead(
        &mut self,
        query: &str,
        max_extra_lines: usize,
    ) -> io::Result<Vec<String>> {
        self.collect_samples()?;

        let mut synth_view: Vec<String> = self.samples.clone();
        if self.eof_reached || self.max_samples != DEFAULT_SAMPLE_SIZE || max_extra_lines == 0 {
            return Ok(synth_view);
        }

        let tokens: Vec<String> = query
            .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
            .map(|t| t.trim().to_lowercase())
            .filter(|t| {
                t.len() >= 3
                    && !matches!(
                        t.as_str(),
                        "the" | "and" | "for" | "with" | "from" | "where" | "lines" | "rows" | "logs" | "errors"
                    )
            })
            .collect();

        let matches_query = |line: &str| -> bool {
            if tokens.is_empty() {
                return false;
            }
            let lower = line.to_lowercase();
            tokens.iter().any(|tok| lower.contains(tok.as_str()))
        };

        let baseline_cols = self
            .samples
            .first()
            .map(|l| l.split_whitespace().count())
            .unwrap_or(0);

        let initial_has_query_match = !tokens.is_empty() && self.samples.iter().any(|l| matches_query(l));
        if initial_has_query_match {
            return Ok(synth_view);
        }

        let mut line_buf = String::with_capacity(1024);
        let mut anomalies_found = 0usize;
        let mut scanned = 0usize;

        while scanned < max_extra_lines && anomalies_found < 3 {
            line_buf.clear();
            let bytes_read = self.reader.read_line(&mut line_buf)?;
            if bytes_read == 0 {
                self.eof_reached = true;
                break;
            }
            scanned += 1;
            let row = line_buf.clone();
            let is_match = matches_query(&row);
            let is_distinct_shape = tokens.is_empty() && row.split_whitespace().count() != baseline_cols;
            if is_match || is_distinct_shape {
                synth_view.push(row.clone());
                anomalies_found += 1;
            }
            self.samples.push(row);
        }

        Ok(synth_view)
    }

    /// Returns a slice of the collected sample rows.
    pub fn samples(&self) -> &[String] {
        &self.samples
    }

    /// True if the stream reached EOF during sampling.
    pub fn is_eof(&self) -> bool {
        self.eof_reached
    }

    /// Returns the number of collected sample rows.
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// True if zero sample rows were collected.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Uncorks the sampler, returning the buffered lines, the remaining reader,
    /// and whether EOF was reached during sampling.
    pub fn into_parts(self) -> (Vec<String>, R, bool) {
        (self.samples, self.reader, self.eof_reached)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_sampler_empty_input() {
        let input = "";
        let cursor = Cursor::new(input);
        let mut sampler = StreamSampler::new(cursor);
        let count = sampler.collect_samples().expect("collect");
        assert_eq!(count, 0);
        assert!(sampler.is_eof());
        assert!(sampler.is_empty());

        let (buffered, _, eof) = sampler.into_parts();
        assert_eq!(buffered.len(), 0);
        assert!(eof);
    }

    #[test]
    fn test_sampler_fewer_than_25_rows() {
        let input = "line1\nline2\nline3\n";
        let cursor = Cursor::new(input);
        let mut sampler = StreamSampler::new(cursor);
        let count = sampler.collect_samples().expect("collect");
        assert_eq!(count, 3);
        assert!(sampler.is_eof());
        assert_eq!(sampler.samples()[0], "line1\n");
        assert_eq!(sampler.samples()[1], "line2\n");
        assert_eq!(sampler.samples()[2], "line3\n");

        let (buffered, _, eof) = sampler.into_parts();
        assert_eq!(buffered.len(), 3);
        assert!(eof);
    }

    #[test]
    fn test_sampler_more_than_25_rows() {
        let mut input = String::new();
        for i in 0..50 {
            input.push_str(&format!("line {}\n", i));
        }

        let cursor = Cursor::new(input);
        let mut sampler = StreamSampler::new(cursor);
        let count = sampler.collect_samples().expect("collect");
        assert_eq!(count, 25);
        assert!(!sampler.is_eof());

        let (buffered, mut remaining, eof) = sampler.into_parts();
        assert_eq!(buffered.len(), 25);
        assert!(!eof);

        // Check that remaining stream starts precisely at line 25
        let mut next_line = String::new();
        remaining.read_line(&mut next_line).expect("read");
        assert_eq!(next_line, "line 25\n");
    }

    #[test]
    fn test_lookahead_captures_late_rare_event_losslessly() {
        let mut input = String::new();
        for i in 0..3000 {
            if i == 750 || i == 755 || i == 760 {
                input.push_str("2026-11-16 ERROR gateway_timeout status=503\n");
            } else {
                input.push_str(&format!("2026-11-16 INFO ok row={i} status=200\n"));
            }
        }
        let mut sampler = StreamSampler::new(Cursor::new(input));
        let synth_view = sampler
            .collect_samples_with_lookahead("gateway_timeout", 2000)
            .expect("lookahead");

        assert!(synth_view.iter().any(|l| l.contains("gateway_timeout")));
        let (buffered, mut remaining, eof) = sampler.into_parts();
        assert!(!eof);
        // Line 0..760 inclusive = 761 lines buffered, next line is row=761
        assert_eq!(buffered.len(), 761);
        let mut next = String::new();
        remaining.read_line(&mut next).expect("read");
        assert!(next.contains("row=761"));
    }
}
