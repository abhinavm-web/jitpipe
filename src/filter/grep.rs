use std::borrow::Cow;
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

use crate::error::JitPipeError;
use crate::filter::CompiledFilter;
use crate::filter::FilterKind;

/// Semantic regular expression filter matching lines against a compiled pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrepFilter {
    pub pattern: String,
    pub invert: bool,
    pub ignore_case: bool,
    #[serde(skip)]
    regex: Option<Regex>,
}

impl GrepFilter {
    /// Compiles a new GrepFilter from the given pattern and flags.
    pub fn new(pattern: impl Into<String>, invert: bool, ignore_case: bool) -> Result<Self, JitPipeError> {
        let pattern_str = pattern.into();
        let compiled = RegexBuilder::new(&pattern_str)
            .case_insensitive(ignore_case)
            .build()
            .map_err(JitPipeError::Regex)?;

        Ok(Self {
            pattern: pattern_str,
            invert,
            ignore_case,
            regex: Some(compiled),
        })
    }

    /// Ensures the inner regular expression is compiled after deserialization.
    pub fn ensure_compiled(&mut self) -> Result<(), JitPipeError> {
        if self.regex.is_none() {
            let compiled = RegexBuilder::new(&self.pattern)
                .case_insensitive(self.ignore_case)
                .build()
                .map_err(JitPipeError::Regex)?;
            self.regex = Some(compiled);
        }
        Ok(())
    }

    /// Access the compiled regex reference if available.
    pub fn regex(&self) -> Option<&Regex> {
        self.regex.as_ref()
    }
}

impl CompiledFilter for GrepFilter {
    fn apply<'a>(&self, line: &'a str) -> Option<Cow<'a, str>> {
        let matched = match &self.regex {
            Some(re) => re.is_match(line),
            None => false,
        };

        if matched ^ self.invert {
            Some(Cow::Borrowed(line))
        } else {
            None
        }
    }

    fn matches(&self, line: &str) -> bool {
        let matched = match &self.regex {
            Some(re) => re.is_match(line),
            None => false,
        };
        matched ^ self.invert
    }

    fn transform(&self, line: &str) -> Option<String> {
        if self.matches(line) {
            Some(line.to_string())
        } else {
            None
        }
    }

    fn kind(&self) -> FilterKind {
        FilterKind::Grep
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grep_filter_matching() {
        let filter = GrepFilter::new(r"error", false, false).expect("compile");
        assert!(filter.matches("2026-11-16 [error] database timeout"));
        assert!(!filter.matches("2026-11-16 [info] health check passed"));

        // Verify zero-allocation Cow::Borrowed
        let input = "critical error occurred";
        let res = filter.apply(input);
        assert!(matches!(res, Some(Cow::Borrowed(_))));
        assert_eq!(res.unwrap(), input);
    }

    #[test]
    fn test_grep_filter_case_insensitive() {
        let filter = GrepFilter::new(r"error", false, true).expect("compile");
        assert!(filter.matches("ERROR: fatal"));
        assert!(filter.matches("Error in connection"));
    }

    #[test]
    fn test_grep_filter_invert() {
        let filter = GrepFilter::new(r"error", true, false).expect("compile");
        assert!(!filter.matches("an error line"));
        assert!(filter.matches("a clean line"));
    }
}
