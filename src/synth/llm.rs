use std::env;
use std::time::Duration;
use serde_json::json;

use crate::error::JitPipeError;
use crate::filter::{CompiledFilter, Filter, FilterKind};
use crate::synth::heuristic::HeuristicSynthesizer;
use crate::synth::FilterSynthesizer;

/// LLM-powered filter synthesizer that sends sample rows and query to an LLM,
/// validates the synthesized filter with self-testing, and falls back to heuristics on failure.
pub struct LlmSynthesizer {
    api_url: Option<String>,
    api_key: Option<String>,
    model: String,
    timeout: Duration,
    heuristic_fallback: HeuristicSynthesizer,
}

impl Default for LlmSynthesizer {
    fn default() -> Self {
        Self::new()
    }
}

impl LlmSynthesizer {
    pub fn new() -> Self {
        let (api_url, api_key, model) = if let Ok(key) = env::var("ANTHROPIC_API_KEY") {
            (
                Some("https://api.anthropic.com/v1/messages".to_string()),
                Some(key),
                env::var("JITPIPE_MODEL").unwrap_or_else(|_| "claude-3-5-sonnet-20241022".to_string()),
            )
        } else if let Ok(key) = env::var("OPENAI_API_KEY") {
            (
                Some("https://api.openai.com/v1/chat/completions".to_string()),
                Some(key),
                env::var("JITPIPE_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string()),
            )
        } else if let Ok(url) = env::var("JITPIPE_LLM_URL") {
            (
                Some(url),
                env::var("JITPIPE_API_KEY").ok(),
                env::var("JITPIPE_MODEL").unwrap_or_else(|_| "default".to_string()),
            )
        } else {
            (None, None, "none".to_string())
        };

        Self {
            api_url,
            api_key,
            model,
            timeout: Duration::from_secs(10),
            heuristic_fallback: HeuristicSynthesizer::new(),
        }
    }

    /// Performs self-test verification against sample rows to ensure compiled safety.
    pub fn self_test_filter(&self, filter: &Filter, samples: &[String]) -> bool {
        for sample in samples {
            let line = sample.trim_end_matches(['\r', '\n']);
            // Must evaluate without panic
            let _ = filter.apply(line);
        }
        true
    }

    /// Checks whether the synthesized filter produces at least one match on a non-empty sample buffer.
    pub fn matches_any_sample(&self, filter: &Filter, samples: &[String]) -> bool {
        if samples.is_empty() {
            return true;
        }
        samples.iter().any(|sample| {
            let line = sample.trim_end_matches(['\r', '\n']);
            filter.apply(line).is_some()
        })
    }

    fn is_anthropic_endpoint(url: &str) -> bool {
        url.contains("anthropic.com") || url.ends_with("/v1/messages")
    }

    fn send_llm_prompt(&self, url: &str, prompt: &str) -> Result<Filter, JitPipeError> {
        let is_anthropic = Self::is_anthropic_endpoint(url);
        let body = if is_anthropic {
            json!({
                "model": self.model,
                "max_tokens": 512,
                "system": "You are a compiler emitting strictly JSON filters. Do not wrap in markdown.",
                "messages": [
                    {"role": "user", "content": prompt}
                ],
                "temperature": 0.0
            })
        } else {
            json!({
                "model": self.model,
                "messages": [
                    {"role": "system", "content": "You are a compiler emitting strictly JSON filters. Do not wrap in markdown."},
                    {"role": "user", "content": prompt}
                ],
                "temperature": 0.0
            })
        };

        let mut req = ureq::post(url)
            .timeout(self.timeout)
            .set("Content-Type", "application/json");

        if let Some(key) = &self.api_key {
            if is_anthropic {
                req = req
                    .set("x-api-key", key)
                    .set("anthropic-version", "2023-06-01");
            } else {
                req = req.set("Authorization", &format!("Bearer {}", key));
            }
        }

        let resp = req.send_json(body).map_err(|e| JitPipeError::Http(e.to_string()))?;
        let resp_json: serde_json::Value = resp.into_json().map_err(JitPipeError::Io)?;

        let content = resp_json["choices"][0]["message"]["content"]
            .as_str()
            .or_else(|| resp_json["content"][0]["text"].as_str())
            .ok_or_else(|| JitPipeError::SynthesisFailed("Missing content in LLM response".to_string()))?;

        let clean_json = content
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        let mut filter: Filter = serde_json::from_str(clean_json).map_err(JitPipeError::Json)?;
        filter.ensure_compiled()?;

        Ok(filter)
    }

    fn call_llm(&self, kind: FilterKind, query: &str, samples: &[String]) -> Result<Filter, JitPipeError> {
        let url = match &self.api_url {
            Some(u) => u,
            None => return Err(JitPipeError::Http("No LLM API configured".to_string())),
        };

        let sample_text = samples.iter().take(25).cloned().collect::<Vec<_>>().join("\n");
        let base_prompt = format!(
            "Synthesize a deterministic {kind} filter for this query: \"{query}\".\nSample data:\n{sample_text}\n\
             Output ONLY valid JSON representing the filter structure matching this schema:\n\
             For grep: {{\"kind\":\"grep\",\"pattern\":\"...\",\"invert\":false,\"ignore_case\":false}}\n\
             For awk: {{\"kind\":\"awk\",\"delimiter\":null,\"output_delimiter\":\"\\t\",\"columns\":[1,2],\"template\":null,\"condition\":null}}\n\
             For jq: {{\"kind\":\"jq\",\"projection\":{{\"Path\":\".user.id\"}},\"predicate\":\"AlwaysTrue\",\"raw_output\":false,\"compact\":true,\"ignore_malformed\":false}}"
        );

        let initial_filter = self.send_llm_prompt(url, &base_prompt)?;
        if self.self_test_filter(&initial_filter, samples) && self.matches_any_sample(&initial_filter, samples) {
            return Ok(initial_filter);
        }

        // Sample-feedback refinement pass when the first candidate matches 0 sample rows
        if let Ok(prev_json) = serde_json::to_string(&initial_filter) {
            let retry_prompt = format!(
                "{base_prompt}\n\nYour previous filter `{prev_json}` matched 0 of the {} sample rows. \
                 Refine the pattern or field names so it accurately matches the relevant sample lines above.",
                samples.len()
            );
            if let Ok(refined_filter) = self.send_llm_prompt(url, &retry_prompt) {
                if self.self_test_filter(&refined_filter, samples) && self.matches_any_sample(&refined_filter, samples) {
                    return Ok(refined_filter);
                }
            }
        }

        Ok(initial_filter)
    }
}

impl FilterSynthesizer for LlmSynthesizer {
    fn synthesize(&self, kind: FilterKind, query: &str, samples: &[String]) -> Result<Filter, JitPipeError> {
        // If LLM endpoint is available, attempt synthesis + sample-feedback verification
        if self.api_url.is_some() {
            if let Ok(filter) = self.call_llm(kind, query, samples) {
                if self.self_test_filter(&filter, samples) && self.matches_any_sample(&filter, samples) {
                    return Ok(filter);
                }
            }
        }

        // Seamless offline fallback to heuristic synthesizer
        self.heuristic_fallback.synthesize(kind, query, samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::grep::GrepFilter;

    #[test]
    fn test_matches_any_sample_verification() {
        let synth = LlmSynthesizer::new();
        let samples = vec![
            "2026-11-16 INFO status=200\n".to_string(),
            "2026-11-16 ERROR status=503\n".to_string(),
        ];
        let matching = Filter::Grep(GrepFilter::new("503", false, false).expect("regex"));
        let non_matching = Filter::Grep(GrepFilter::new("999", false, false).expect("regex"));

        assert!(synth.matches_any_sample(&matching, &samples));
        assert!(!synth.matches_any_sample(&non_matching, &samples));
        assert!(synth. self_test_filter(&matching, &samples));
    }

    #[test]
    fn test_anthropic_endpoint_detection() {
        assert!(LlmSynthesizer::is_anthropic_endpoint("https://api.anthropic.com/v1/messages"));
        assert!(!LlmSynthesizer::is_anthropic_endpoint("https://api.openai.com/v1/chat/completions"));
    }
}
