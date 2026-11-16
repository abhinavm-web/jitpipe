use std::borrow::Cow;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::filter::CompiledFilter;
use crate::filter::FilterKind;

/// Predicate condition applied to JSON values.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum JqPredicate {
    AlwaysTrue,
    Equals { path: String, value: Value },
    NotEquals { path: String, value: Value },
    GreaterThan { path: String, threshold: f64 },
    LessThan { path: String, threshold: f64 },
    GreaterThanOrEqual { path: String, threshold: f64 },
    LessThanOrEqual { path: String, threshold: f64 },
    Contains { path: String, substr: String },
    Exists { path: String },
}

/// Projection transformation applied to JSON values.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum JqProjection {
    /// Return the value unmodified.
    Identity,
    /// Extract a dot-path (e.g. `user.id` or `items[0]`).
    Path(String),
    /// Project specified object fields into a new JSON object.
    Fields(Vec<String>),
    /// Flatten/unroll an array field into multiple output elements.
    ArrayUnroll(String),
}

/// Semantic Jq filter for JSON line extraction, projection, and predicate filtering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JqFilter {
    pub projection: JqProjection,
    pub predicate: JqPredicate,
    pub raw_output: bool,
    pub compact: bool,
    pub ignore_malformed: bool,
}

impl Default for JqFilter {
    fn default() -> Self {
        Self {
            projection: JqProjection::Identity,
            predicate: JqPredicate::AlwaysTrue,
            raw_output: false,
            compact: true,
            ignore_malformed: false,
        }
    }
}

impl JqFilter {
    /// Creates a filter extracting a dot path.
    pub fn new_path(path: impl Into<String>, raw_output: bool) -> Self {
        Self {
            projection: JqProjection::Path(path.into()),
            predicate: JqPredicate::AlwaysTrue,
            raw_output,
            compact: true,
            ignore_malformed: false,
        }
    }

    /// Creates a filter extracting an object projection.
    pub fn new_fields(fields: Vec<String>) -> Self {
        Self {
            projection: JqProjection::Fields(fields),
            predicate: JqPredicate::AlwaysTrue,
            raw_output: false,
            compact: true,
            ignore_malformed: false,
        }
    }

    /// Set predicate condition.
    pub fn with_predicate(mut self, predicate: JqPredicate) -> Self {
        self.predicate = predicate;
        self
    }

    /// Set whether malformed lines are ignored.
    pub fn with_ignore_malformed(mut self, ignore: bool) -> Self {
        self.ignore_malformed = ignore;
        self
    }

    /// Traverses a JSON value along a dot-path string.
    /// Handles dot notation like `.user.id`, `user.id`, and array indices `[0]`.
    pub fn traverse_path<'a>(root: &'a Value, path: &str) -> &'a Value {
        let clean = path.trim_start_matches('.');
        if clean.is_empty() {
            return root;
        }

        let mut current = root;
        for segment in clean.split('.') {
            if segment.is_empty() {
                continue;
            }

            // Check if segment has array indexing e.g. items[0]
            if let Some(bracket_idx) = segment.find('[') {
                let key = &segment[..bracket_idx];
                if !key.is_empty() {
                    current = match current {
                        Value::Object(map) => map.get(key).unwrap_or(&Value::Null),
                        _ => &Value::Null,
                    };
                }

                let remaining = &segment[bracket_idx..];
                for part in remaining.split('[') {
                    if let Some(close_idx) = part.find(']') {
                        let idx_str = &part[..close_idx];
                        if let Ok(idx) = idx_str.parse::<usize>() {
                            current = match current {
                                Value::Array(arr) => arr.get(idx).unwrap_or(&Value::Null),
                                _ => &Value::Null,
                            };
                        }
                    }
                }
            } else {
                current = match current {
                    Value::Object(map) => map.get(segment).unwrap_or(&Value::Null),
                    _ => &Value::Null,
                };
            }

            if current.is_null() {
                break;
            }
        }

        current
    }

    /// Evaluates the predicate condition against a parsed JSON value.
    pub fn evaluate_predicate(&self, root: &Value) -> bool {
        match &self.predicate {
            JqPredicate::AlwaysTrue => true,
            JqPredicate::Equals { path, value } => {
                Self::traverse_path(root, path) == value
            }
            JqPredicate::NotEquals { path, value } => {
                Self::traverse_path(root, path) != value
            }
            JqPredicate::GreaterThan { path, threshold } => {
                if let Some(n) = Self::traverse_path(root, path).as_f64() {
                    n > *threshold
                } else {
                    false
                }
            }
            JqPredicate::LessThan { path, threshold } => {
                if let Some(n) = Self::traverse_path(root, path).as_f64() {
                    n < *threshold
                } else {
                    false
                }
            }
            JqPredicate::GreaterThanOrEqual { path, threshold } => {
                if let Some(n) = Self::traverse_path(root, path).as_f64() {
                    n >= *threshold
                } else {
                    false
                }
            }
            JqPredicate::LessThanOrEqual { path, threshold } => {
                if let Some(n) = Self::traverse_path(root, path).as_f64() {
                    n <= *threshold
                } else {
                    false
                }
            }
            JqPredicate::Contains { path, substr } => {
                let target = Self::traverse_path(root, path);
                match target {
                    Value::String(s) => s.contains(substr),
                    Value::Array(arr) => arr.iter().any(|v| match v {
                        Value::String(s) => s.contains(substr),
                        _ => false,
                    }),
                    _ => false,
                }
            }
            JqPredicate::Exists { path } => {
                !Self::traverse_path(root, path).is_null()
            }
        }
    }

    /// Formats an individual JSON value according to raw_output rules.
    pub fn format_value(&self, value: &Value) -> String {
        if self.raw_output {
            match value {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                Value::Null => String::new(),
                Value::Array(_) | Value::Object(_) => {
                    serde_json::to_string(value).unwrap_or_default()
                }
            }
        } else {
            serde_json::to_string(value).unwrap_or_default()
        }
    }
}

impl CompiledFilter for JqFilter {
    fn apply<'a>(&self, line: &'a str) -> Option<Cow<'a, str>> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        let parsed: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => {
                if self.ignore_malformed {
                    return None;
                }
                // Attempt to find embedded JSON if surrounded by log prefixes
                if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
                    if start < end {
                        if let Ok(v) = serde_json::from_str(&trimmed[start..=end]) {
                            v
                        } else {
                            return None;
                        }
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            }
        };

        if !self.evaluate_predicate(&parsed) {
            return None;
        }

        match &self.projection {
            JqProjection::Identity => {
                let out = self.format_value(&parsed);
                Some(Cow::Owned(out))
            }
            JqProjection::Path(path) => {
                let val = Self::traverse_path(&parsed, path);
                if val.is_null() && self.raw_output {
                    None
                } else {
                    let out = self.format_value(val);
                    Some(Cow::Owned(out))
                }
            }
            JqProjection::Fields(fields) => {
                let mut map = serde_json::Map::new();
                for field in fields {
                    let val = Self::traverse_path(&parsed, field);
                    if !val.is_null() {
                        let clean_name = field.trim_start_matches('.');
                        map.insert(clean_name.to_string(), val.clone());
                    }
                }
                let projected = Value::Object(map);
                let out = self.format_value(&projected);
                Some(Cow::Owned(out))
            }
            JqProjection::ArrayUnroll(path) => {
                let val = Self::traverse_path(&parsed, path);
                match val {
                    Value::Array(items) => {
                        let lines: Vec<String> = items.iter().map(|item| self.format_value(item)).collect();
                        Some(Cow::Owned(lines.join("\n")))
                    }
                    _ => {
                        let out = self.format_value(val);
                        Some(Cow::Owned(out))
                    }
                }
            }
        }
    }

    fn matches(&self, line: &str) -> bool {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return false;
        }
        if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
            self.evaluate_predicate(&parsed)
        } else {
            false
        }
    }

    fn transform(&self, line: &str) -> Option<String> {
        self.apply(line).map(|c| c.into_owned())
    }

    fn kind(&self) -> FilterKind {
        FilterKind::Jq
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jq_dot_path_raw_output() {
        let filter = JqFilter::new_path(".user.id", true);
        let line = r#"{"user":{"id":"usr_42","name":"Alice"}}"#;
        let out = filter.apply(line).expect("apply");
        assert_eq!(out, "usr_42");
    }

    #[test]
    fn test_jq_fields_projection() {
        let filter = JqFilter::new_fields(vec!["id".to_string(), "status".to_string()]);
        let line = r#"{"id":100,"status":"active","payload":"ignored"}"#;
        let out = filter.apply(line).expect("apply");
        let parsed: Value = serde_json::from_str(&out).expect("json");
        assert_eq!(parsed["id"], 100);
        assert_eq!(parsed["status"], "active");
        assert!(parsed.get("payload").is_none());
    }

    #[test]
    fn test_jq_predicate_filter() {
        let filter = JqFilter::new_path(".status", true)
            .with_predicate(JqPredicate::GreaterThan {
                path: "latency".to_string(),
                threshold: 500.0,
            });

        let line_match = r#"{"status":"slow","latency":850}"#;
        let line_fail = r#"{"status":"fast","latency":120}"#;

        assert_eq!(filter.apply(line_match), Some(Cow::Owned("slow".to_string())));
        assert_eq!(filter.apply(line_fail), None);
    }

    #[test]
    fn test_jq_ignore_malformed() {
        let filter = JqFilter::new_path(".msg", true).with_ignore_malformed(true);
        assert!(filter.apply("not a json line").is_none());
        assert_eq!(filter.apply(r#"{"msg":"hello"}"#), Some(Cow::Owned("hello".to_string())));
    }
}
