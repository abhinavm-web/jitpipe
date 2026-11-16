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
            self.samples.push(line_buf.clone());
        }
        Ok(self.samples.len())
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
}
