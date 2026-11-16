use std::borrow::Cow;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::error::JitPipeError;
use crate::filter::CompiledFilter;
use crate::filter::FilterKind;

/// Condition predicate for conditional Awk row filtering.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AwkCondition {
    /// Matches if column `col` (1-based) equals `value`.
    FieldEquals { col: usize, value: String },
    /// Matches if column `col` (1-based) as float is greater than `threshold`.
    FieldGreaterThan { col: usize, threshold: f64 },
    /// Matches if column `col` (1-based) as float is less than `threshold`.
    FieldLessThan { col: usize, threshold: f64 },
    /// Matches if entire line matches regex.
    LineRegex { pattern: String },
}

/// Semantic Awk filter providing column splitting, condition evaluation, and formatted projection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AwkFilter {
    /// Field delimiter (None means standard POSIX whitespace splitting).
    pub delimiter: Option<String>,
    /// Output delimiter string between emitted columns (defaults to tab).
    pub output_delimiter: String,
    /// 1-based column indices to extract when template is not used.
    pub columns: Vec<usize>,
    /// Optional format template containing `$0`, `$1`, `$NF`, etc.
    pub template: Option<String>,
    /// Optional row filtering condition.
    pub condition: Option<AwkCondition>,
    #[serde(skip)]
    compiled_regex: Option<Regex>,
}

impl AwkFilter {
    /// Creates a new AwkFilter for column projection with a given delimiter.
    pub fn new_columns(delimiter: Option<String>, output_delimiter: Option<String>, columns: Vec<usize>) -> Self {
        Self {
            delimiter,
            output_delimiter: output_delimiter.unwrap_or_else(|| "\t".to_string()),
            columns,
            template: None,
            condition: None,
            compiled_regex: None,
        }
    }

    /// Creates a new AwkFilter with a format template (e.g. `"$1\t$3"`).
    pub fn new_template(delimiter: Option<String>, template: impl Into<String>) -> Self {
        Self {
            delimiter,
            output_delimiter: "\t".to_string(),
            columns: Vec::new(),
            template: Some(template.into()),
            condition: None,
            compiled_regex: None,
        }
    }

    /// Sets a row filter condition.
    pub fn with_condition(mut self, condition: AwkCondition) -> Result<Self, JitPipeError> {
        if let AwkCondition::LineRegex { pattern } = &condition {
            self.compiled_regex = Some(Regex::new(pattern).map_err(JitPipeError::Regex)?);
        }
        self.condition = Some(condition);
        Ok(self)
    }

    /// Ensures compiled regular expressions are present after deserialization.
    pub fn ensure_compiled(&mut self) -> Result<(), JitPipeError> {
        if let Some(AwkCondition::LineRegex { pattern }) = &self.condition {
            if self.compiled_regex.is_none() {
                self.compiled_regex = Some(Regex::new(pattern).map_err(JitPipeError::Regex)?);
            }
        }
        Ok(())
    }

    /// Splits an input line into fields according to delimiter configuration.
    pub fn split_fields<'a>(&self, line: &'a str) -> Vec<&'a str> {
        match &self.delimiter {
            Some(delim) if !delim.is_empty() => {
                line.split(delim.as_str()).collect()
            }
            _ => {
                // POSIX awk default whitespace splitting: trim leading/trailing whitespace, split on runs of whitespace.
                line.split_whitespace().collect()
            }
        }
    }

    /// Evaluates the condition predicate against the line and split fields.
    fn evaluate_condition(&self, line: &str, fields: &[&str]) -> bool {
        let condition = match &self.condition {
            Some(c) => c,
            None => return true,
        };

        match condition {
            AwkCondition::FieldEquals { col, value } => {
                if *col == 0 {
                    line == value
                } else if *col <= fields.len() {
                    fields[*col - 1] == value
                } else {
                    false
                }
            }
            AwkCondition::FieldGreaterThan { col, threshold } => {
                let text = if *col == 0 {
                    Some(line)
                } else if *col <= fields.len() {
                    Some(fields[*col - 1])
                } else {
                    None
                };

                if let Some(s) = text {
                    if let Ok(num) = s.parse::<f64>() {
                        num > *threshold
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            AwkCondition::FieldLessThan { col, threshold } => {
                let text = if *col == 0 {
                    Some(line)
                } else if *col <= fields.len() {
                    Some(fields[*col - 1])
                } else {
                    None
                };

                if let Some(s) = text {
                    if let Ok(num) = s.parse::<f64>() {
                        num < *threshold
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            AwkCondition::LineRegex { .. } => {
                match &self.compiled_regex {
                    Some(re) => re.is_match(line),
                    None => true,
                }
            }
        }
    }

    /// Resolves a field token like `$0`, `$1`, `$NF`, `$(NF-1)` against fields.
    /// Returns empty string `""` on out-of-bounds indices without panicking.
    fn resolve_field<'a>(&self, token: &str, line: &'a str, fields: &[&'a str]) -> &'a str {
        if token == "$0" {
            return line;
        }
        if token == "$NF" {
            return fields.last().copied().unwrap_or("");
        }
        if token == "$(NF-1)" {
            return if fields.len() >= 2 {
                fields[fields.len() - 2]
            } else {
                ""
            };
        }
        if let Some(idx_str) = token.strip_prefix('$') {
            if let Ok(idx) = idx_str.parse::<usize>() {
                if idx == 0 {
                    return line;
                } else if idx <= fields.len() {
                    return fields[idx - 1];
                } else {
                    return "";
                }
            }
        }
        ""
    }

    /// Interpolates a template string replacing tokens with field values.
    fn format_template(&self, template: &str, line: &str, fields: &[&str]) -> String {
        let mut result = String::with_capacity(line.len());
        let mut chars = template.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '$' {
                let mut token = String::from("$");
                if let Some(&next_ch) = chars.peek() {
                    if next_ch.is_ascii_digit() {
                        while let Some(&d) = chars.peek() {
                            if d.is_ascii_digit() {
                                token.push(d);
                                chars.next();
                            } else {
                                break;
                            }
                        }
                        result.push_str(self.resolve_field(&token, line, fields));
                        continue;
                    } else if next_ch == 'N' {
                        chars.next();
                        token.push('N');
                        if let Some(&'F') = chars.peek() {
                            chars.next();
                            token.push('F');
                        }
                        result.push_str(self.resolve_field(&token, line, fields));
                        continue;
                    } else if next_ch == '(' {
                        chars.next();
                        let mut inner = String::from("$(");
                        while let Some(&c) = chars.peek() {
                            inner.push(c);
                            chars.next();
                            if c == ')' {
                                break;
                            }
                        }
                        result.push_str(self.resolve_field(&inner, line, fields));
                        continue;
                    }
                }
                result.push(ch);
            } else if ch == '\\' {
                match chars.next() {
                    Some('t') => result.push('\t'),
                    Some('n') => result.push('\n'),
                    Some('\\') => result.push('\\'),
                    Some(c) => {
                        result.push('\\');
                        result.push(c);
                    }
                    None => result.push('\\'),
                }
            } else {
                result.push(ch);
            }
        }

        result
    }
}

impl CompiledFilter for AwkFilter {
    fn apply<'a>(&self, line: &'a str) -> Option<Cow<'a, str>> {
        let fields = self.split_fields(line);
        if !self.evaluate_condition(line, &fields) {
            return None;
        }

        if let Some(template) = &self.template {
            let formatted = self.format_template(template, line, &fields);
            Some(Cow::Owned(formatted))
        } else if !self.columns.is_empty() {
            let mut parts = Vec::with_capacity(self.columns.len());
            for &col in &self.columns {
                if col == 0 {
                    parts.push(line);
                } else if col <= fields.len() {
                    parts.push(fields[col - 1]);
                } else {
                    parts.push("");
                }
            }
            Some(Cow::Owned(parts.join(&self.output_delimiter)))
        } else {
            Some(Cow::Borrowed(line))
        }
    }

    fn matches(&self, line: &str) -> bool {
        let fields = self.split_fields(line);
        self.evaluate_condition(line, &fields)
    }

    fn transform(&self, line: &str) -> Option<String> {
        self.apply(line).map(|c| c.into_owned())
    }

    fn kind(&self) -> FilterKind {
        FilterKind::Awk
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_awk_whitespace_columns() {
        let filter = AwkFilter::new_columns(None, Some("\t".to_string()), vec![1, 3]);
        let line = "alpha   beta   gamma   delta";
        let out = filter.apply(line).expect("apply");
        assert_eq!(out, "alpha\tgamma");
    }

    #[test]
    fn test_awk_csv_delimiter() {
        let filter = AwkFilter::new_columns(Some(",".to_string()), Some("|".to_string()), vec![1, 2]);
        let line = "user_123,success,500ms";
        let out = filter.apply(line).expect("apply");
        assert_eq!(out, "user_123|success");
    }

    #[test]
    fn test_awk_out_of_bounds_safety() {
        let filter = AwkFilter::new_columns(None, Some("\t".to_string()), vec![1, 99]);
        let line = "one two";
        let out = filter.apply(line).expect("apply");
        assert_eq!(out, "one\t");
    }

    #[test]
    fn test_awk_template_with_nf() {
        let filter = AwkFilter::new_template(None, "First: $1, Last: $NF");
        let line = "apple banana cherry";
        let out = filter.apply(line).expect("apply");
        assert_eq!(out, "First: apple, Last: cherry");
    }

    #[test]
    fn test_awk_condition_filtering() {
        let filter = AwkFilter::new_columns(None, Some("\t".to_string()), vec![1, 2])
            .with_condition(AwkCondition::FieldGreaterThan { col: 2, threshold: 100.0 })
            .expect("condition");

        assert!(filter.apply("node1 250").is_some());
        assert!(filter.apply("node2 50").is_none());
    }
}
