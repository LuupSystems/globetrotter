//! LLM-as-judge review of translation consistency across languages.
//!
//! Given a translation key and its strings in several languages, this crate asks
//! a chat-completion model whether every language tells the user the same thing,
//! and reports the languages that do not ([`KeyInput`] → [`Finding`]). Judging is
//! per key — the model sees all languages of a key in one prompt — so the correct
//! majority gives it context and each key yields at most one round-trip.
//!
//! The endpoint is any OpenAI-compatible server ([`Options::base_url`]): a local
//! ollama/vLLM/llama.cpp instance or a hosted API. Responses are constrained to
//! the [`Verdict`] JSON schema (derived via [`schemars`]), with one recovery
//! attempt per key for servers that reject strict structured output.
//!
//! Verdicts are cached content-addressed on disk (see [`cache`]), so re-runs
//! only pay for keys whose text — or judge configuration — actually changed.
//!
//! The public API is pure data in, pure data out; it has no knowledge of
//! globetrotter's model, diagnostics, or configuration types.

pub mod cache;
pub mod prompt;

use async_openai::{Client, config::OpenAIConfig};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The number of consecutive request failures that makes an endpoint unusable.
///
/// Isolated failures are reported and skipped; a sustained failure stops the
/// run instead of repeating the same endpoint error for every remaining key.
const MAX_CONSECUTIVE_FAILURES: usize = 5;

/// Errors that can occur while judging.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Building a request or reaching the endpoint failed.
    #[error(transparent)]
    Api(#[from] async_openai::error::OpenAIError),

    /// Reading or writing the verdict cache failed.
    #[error("verdict cache error: {0}")]
    Cache(#[from] std::io::Error),

    /// The endpoint failed several requests in a row and is presumed unusable.
    #[error(
        "aborting after {failures} consecutive request failures \
         (endpoint misconfigured or down?); last error: {last}"
    )]
    EndpointUnusable {
        /// Number of consecutive failures observed.
        failures: usize,
        /// The last per-key error, as a human-readable string.
        last: String,
    },

    /// A custom prompt template is missing a required placeholder.
    ///
    /// Without both placeholders every key would render the same prompt — and
    /// therefore share a single cached verdict — so this is rejected up front.
    #[error(
        "prompt template must contain the `{{key}}` and `{{languages}}` \
         placeholders; missing `{missing}`"
    )]
    Template {
        /// The placeholder that was not found.
        missing: &'static str,
    },
}

/// Reasoning effort requested from the model, for models that support it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effort {
    /// Minimal reasoning; fastest, least reliable.
    Low,
    /// Balanced reasoning; the tested sweet spot for this task.
    Medium,
    /// Maximal reasoning; slowest.
    High,
}

impl Effort {
    fn to_api(self) -> async_openai::types::chat::ReasoningEffort {
        use async_openai::types::chat::ReasoningEffort;
        match self {
            Self::Low => ReasoningEffort::Low,
            Self::Medium => ReasoningEffort::Medium,
            Self::High => ReasoningEffort::High,
        }
    }
}

/// Options controlling a [`Judge`].
#[derive(Debug, Clone)]
pub struct Options {
    /// Base URL of the OpenAI-compatible endpoint (e.g. a local
    /// `http://localhost:11434/v1` for ollama).
    pub base_url: String,
    /// Model name as known to the endpoint.
    pub model: String,
    /// Environment variable read for the API key. Local servers ignore the key,
    /// so an unset variable simply sends an empty one.
    pub api_key_env: String,
    /// Maximum number of in-flight requests.
    pub concurrency: usize,
    /// Sampling temperature. `0.0` keeps verdicts reproducible.
    pub temperature: f32,
    /// Reasoning effort, for models that support it; `None` sends no effort
    /// field at all.
    pub effort: Option<Effort>,
    /// Prompt template overriding [`prompt::DEFAULT_TEMPLATE`]. Must contain the
    /// `{key}` and `{languages}` placeholders (see [`prompt::render`]).
    pub template: Option<String>,
    /// Findings with a confidence below this are counted as suppressed instead
    /// of being reported. `0.0` (the default) reports everything.
    ///
    /// Applied when a verdict is emitted — after the cache — so changing the
    /// threshold re-filters cached verdicts without re-running inference.
    pub min_confidence: f64,
    /// Verdict cache location; `None` uses the OS user cache directory.
    pub cache_dir: Option<std::path::PathBuf>,
    /// Maximum number of cached verdicts kept on disk; `0` disables caching.
    pub cache_capacity: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434/v1".to_string(),
            model: "gemma4:12b".to_string(),
            api_key_env: "OPENAI_API_KEY".to_string(),
            concurrency: 8,
            temperature: 0.0,
            effort: Some(Effort::Medium),
            template: None,
            min_confidence: 0.0,
            cache_dir: None,
            cache_capacity: cache::DEFAULT_CAPACITY,
        }
    }
}

/// One language's string for a key.
#[derive(Debug, Clone, Copy)]
pub struct LanguageText<'a> {
    /// The language code (e.g. `en`, `de`).
    pub language: &'a str,
    /// The translated text.
    pub text: &'a str,
}

/// A translation key together with its per-language strings to judge.
#[derive(Debug, Clone)]
pub struct KeyInput<'a> {
    /// The dotted key path. Included in the prompt: key names often reveal
    /// intent (e.g. `phone-placeholder`) that disambiguates locale adaptations.
    pub key: &'a str,
    /// The key's strings, one per language.
    pub languages: Vec<LanguageText<'a>>,
}

/// The model's verdict for one key. This struct *is* the response contract:
/// its derived JSON schema is sent as the strict `response_format`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Verdict {
    /// Whether every language tells the user the same thing.
    pub consistent: bool,
    /// The languages that do not, with a short reason each. Empty when
    /// `consistent` is `true`.
    pub issues: Vec<Issue>,
}

/// One language flagged by the model.
///
/// The schema extension keeps `confidence` required for strict structured
/// output even though serde accepts a missing value on the recovery path.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("required" = ["language", "problem", "confidence"]))]
pub struct Issue {
    /// The language code the model flagged.
    pub language: String,
    /// The model's short explanation of what differs.
    pub problem: String,
    /// The model's self-reported certainty that the difference is real, from
    /// 0.0 to 1.0. Verbalized confidence is only loosely calibrated, so treat
    /// it as an ordinal ranking of findings rather than a probability.
    ///
    /// The recovery parser defaults a missing value to full confidence so the
    /// finding is retained rather than silently filtered out.
    #[serde(default = "full_confidence")]
    pub confidence: f64,
}

/// The parse-time default for [`Issue::confidence`]; full confidence, so
/// findings without one are never filtered out.
fn full_confidence() -> f64 {
    1.0
}

/// A flagged language of a key, ready to be reported.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    /// The dotted key path.
    pub key: String,
    /// The flagged language code.
    pub language: String,
    /// The model's explanation of what differs.
    pub problem: String,
    /// The model's self-reported certainty, clamped to 0.0–1.0. See
    /// [`Issue::confidence`] for how (little) to trust it.
    pub confidence: f64,
}

/// Counters describing a completed [`Judge::judge`] run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    /// Keys judged (including cache hits).
    pub judged: usize,
    /// Keys answered from the verdict cache without a request.
    pub cached: usize,
    /// Keys skipped because both the request and its recovery attempt failed.
    pub failed: usize,
    /// Findings emitted.
    pub flagged: usize,
    /// Findings dropped because their confidence was below
    /// [`Options::min_confidence`].
    pub suppressed: usize,
}

/// Receives progress updates so a caller can drive a progress bar. The no-op
/// `()` implementation is available when progress isn't needed.
pub trait Progress: Send + Sync {
    /// Sets the total number of keys before judging begins.
    fn set_length(&self, total: u64) {
        let _ = total;
    }
    /// Advances completed keys by `delta`.
    fn inc(&self, delta: u64) {
        let _ = delta;
    }
}

impl Progress for () {}

/// A configured judge holding the HTTP client, the response schema, and the
/// verdict cache.
pub struct Judge {
    client: Client<OpenAIConfig>,
    options: Options,
    schema: serde_json::Value,
    cache: Option<cache::Cache>,
}

impl Judge {
    /// Creates a judge from `options`.
    ///
    /// The API key is read from [`Options::api_key_env`] once, here. No request
    /// is made until [`Self::judge`] runs.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Template`] if a custom template lacks the `{key}` or
    /// `{languages}` placeholder, or an error if the cache directory cannot be
    /// created.
    pub fn new(options: Options) -> Result<Self, Error> {
        if let Some(template) = &options.template {
            for placeholder in ["{key}", "{languages}"] {
                if !template.contains(placeholder) {
                    return Err(Error::Template {
                        missing: placeholder,
                    });
                }
            }
        }
        let api_key = std::env::var(&options.api_key_env).unwrap_or_default();
        let config = OpenAIConfig::new()
            .with_api_base(options.base_url.trim_end_matches('/'))
            .with_api_key(api_key);
        let schema = schemars::schema_for!(Verdict).to_value();
        let cache = cache::Cache::open(options.cache_dir.as_deref(), options.cache_capacity)?;
        Ok(Self {
            client: Client::with_config(config),
            options,
            schema,
            cache,
        })
    }

    /// Judges every key, calling `sink` with each [`Finding`] as its verdict
    /// arrives (completion order, not input order).
    ///
    /// Up to [`Options::concurrency`] requests are in flight at a time. A key
    /// whose request *and* recovery attempt fail is warned about and skipped;
    /// the run only aborts when several keys fail consecutively.
    ///
    /// # Errors
    ///
    /// Returns [`Error::EndpointUnusable`] when the endpoint keeps failing, or
    /// an error if the verdict cache cannot be written.
    pub async fn judge(
        &self,
        keys: &[KeyInput<'_>],
        progress: &dyn Progress,
        sink: &mut dyn FnMut(Finding),
    ) -> Result<Stats, Error> {
        use futures::StreamExt;

        progress.set_length(keys.len() as u64);

        let mut stats = Stats::default();
        let mut consecutive_failures = 0usize;
        // Clamped so an out-of-range threshold cannot silently suppress even
        // full-confidence findings.
        let min_confidence = self.options.min_confidence.clamp(0.0, 1.0);

        // Judge keys concurrently while preserving completion-order delivery.
        let mut verdicts = futures::stream::iter(keys.iter().map(|key| async move {
            let outcome = self.judge_key(key).await;
            (key, outcome)
        }))
        .buffer_unordered(self.options.concurrency.max(1));

        // Stream successful findings and stop after repeated endpoint failures.
        while let Some((key, outcome)) = verdicts.next().await {
            progress.inc(1);
            match outcome {
                Ok((verdict, from_cache)) => {
                    consecutive_failures = 0;
                    stats.judged += 1;
                    if from_cache {
                        stats.cached += 1;
                    }
                    if !verdict.consistent {
                        for issue in verdict.issues {
                            // Clamp before comparing: the schema documents the
                            // 0–1 range but cannot enforce it server-side.
                            let confidence = issue.confidence.clamp(0.0, 1.0);
                            if confidence < min_confidence {
                                stats.suppressed += 1;
                                continue;
                            }
                            stats.flagged += 1;
                            sink(Finding {
                                key: key.key.to_string(),
                                language: issue.language,
                                problem: issue.problem,
                                confidence,
                            });
                        }
                    }
                }
                Err(error) => {
                    stats.failed += 1;
                    consecutive_failures += 1;
                    tracing::warn!(key = key.key, %error, "llm judge request failed; skipping key");
                    if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                        return Err(Error::EndpointUnusable {
                            failures: consecutive_failures,
                            last: error.to_string(),
                        });
                    }
                }
            }
        }
        drop(verdicts);

        // Defer eviction until all cache reads and writes have finished.
        if let Some(cache) = &self.cache {
            cache.enforce_capacity()?;
        }
        Ok(stats)
    }

    /// Judges one key using the cache, a strict-schema request, and one
    /// recovery attempt without structured output. Returns the verdict and
    /// whether it came from the cache.
    async fn judge_key(&self, key: &KeyInput<'_>) -> Result<(Verdict, bool), Error> {
        let rendered = prompt::render(
            self.options
                .template
                .as_deref()
                .unwrap_or(prompt::DEFAULT_TEMPLATE),
            key,
        );

        // The hash covers everything that can change the verdict: the rendered
        // prompt (template + key + all language strings) and the generation
        // configuration. The endpoint URL is included because the same model
        // name can resolve to different weights/quantizations per server.
        // `min_confidence` is deliberately excluded: it filters at emit time,
        // so changing the threshold re-filters cached verdicts for free.
        let cache_key = self.cache.as_ref().map(|cache| {
            cache.key(&[
                &self.options.base_url,
                &self.options.model,
                &format!("{:?}", self.options.effort),
                &format!("{}", self.options.temperature),
                &rendered,
            ])
        });
        if let (Some(cache), Some(cache_key)) = (&self.cache, &cache_key)
            && let Some(verdict) = cache.lookup(cache_key)
        {
            return Ok((verdict, true));
        }

        // Strict structured output first; one recovery attempt without it for
        // servers or models that reject or ignore the schema. The recovery
        // request also drops the reasoning-effort field, since a server old
        // enough to reject `response_format` tends to reject that too.
        let verdict = match self.request(&rendered, true).await {
            Ok(verdict) => verdict,
            Err(first_error) => {
                tracing::debug!(
                    key = key.key,
                    %first_error,
                    "strict structured output failed; retrying without response schema"
                );
                self.request(&rendered, false).await?
            }
        };

        if let (Some(cache), Some(cache_key)) = (&self.cache, &cache_key) {
            cache.store(cache_key, &verdict)?;
        }
        Ok((verdict, false))
    }

    /// Sends one chat completion and parses the verdict. With `strict`, the
    /// derived JSON schema is enforced server-side; without it, the response is
    /// parsed leniently (the prompt itself already demands JSON).
    async fn request(&self, rendered_prompt: &str, strict: bool) -> Result<Verdict, Error> {
        use async_openai::types::chat::{
            ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs, ResponseFormat,
            ResponseFormatJsonSchema,
        };

        let message = ChatCompletionRequestUserMessageArgs::default()
            .content(rendered_prompt)
            .build()?;

        let mut request = CreateChatCompletionRequestArgs::default();
        request
            .model(&self.options.model)
            .temperature(self.options.temperature)
            .messages(vec![message.into()]);
        if strict {
            request.response_format(ResponseFormat::JsonSchema {
                json_schema: ResponseFormatJsonSchema {
                    name: "verdict".to_string(),
                    description: Some("Consistency verdict for one translation key".to_string()),
                    schema: self.schema.clone(),
                    strict: Some(true),
                },
            });
            if let Some(effort) = self.options.effort {
                request.reasoning_effort(effort.to_api());
            }
        }
        let request = request.build()?;

        let response = self.client.chat().create(request).await?;
        let content = response
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
            .unwrap_or_default();
        parse_verdict(content).map_err(|error| {
            async_openai::error::OpenAIError::JSONDeserialize(error, content.to_string()).into()
        })
    }
}

/// Parses a verdict from model output, tolerating surrounding prose or a
/// ```` ```json ```` fence by falling back to the first balanced `{…}` block.
fn parse_verdict(content: &str) -> Result<Verdict, serde_json::Error> {
    match serde_json::from_str(content) {
        Ok(verdict) => Ok(verdict),
        Err(error) => match first_json_object(content) {
            Some(block) => serde_json::from_str(block),
            None => Err(error),
        },
    }
}

/// The first balanced top-level `{…}` block in `text`, if any. Braces inside
/// JSON strings are skipped by tracking string/escape state.
fn first_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, c) in text[start..].char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '{' if !in_string => depth += 1,
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..=start + offset]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{Verdict, first_json_object, parse_verdict};

    /// A bare structured response parses directly.
    #[test]
    fn parses_a_bare_verdict() {
        let verdict: Verdict =
            parse_verdict(r#"{"consistent": true, "issues": []}"#).expect("parse");
        assert!(verdict.consistent);
        assert!(verdict.issues.is_empty());
    }

    /// The recovery path receives free-form model output; a fenced or
    /// prose-wrapped JSON object must still parse.
    #[test]
    fn parses_a_fenced_verdict() {
        let content = "Here is my verdict:\n```json\n{\"consistent\": false, \
                       \"issues\": [{\"language\": \"de\", \"problem\": \"says {x}\"}]}\n```";
        let verdict = parse_verdict(content).expect("parse");
        assert!(!verdict.consistent);
        assert_eq!(verdict.issues[0].language, "de");
        // Braces inside JSON strings do not affect balanced-block extraction.
        assert_eq!(verdict.issues[0].problem, "says {x}");
        // Missing confidence defaults to 1.0 so the finding is retained.
        assert!((verdict.issues[0].confidence - 1.0).abs() < f64::EPSILON);
    }

    /// A template without the placeholders would render one identical prompt
    /// for every key (and thus one shared cache entry); creation must fail.
    #[test]
    fn rejects_template_without_placeholders() {
        let options = crate::Options {
            template: Some("judge this: {key}".to_string()),
            ..crate::Options::default()
        };
        let missing = match crate::Judge::new(options) {
            Err(crate::Error::Template { missing }) => missing,
            Err(other) => panic!("expected a template error, got: {other}"),
            Ok(_) => panic!("a template without {{languages}} must be rejected"),
        };
        assert_eq!(missing, "{languages}");
    }

    /// Explicit confidence values survive response parsing.
    #[test]
    fn parses_an_explicit_confidence() {
        let content = r#"{"consistent": false, "issues": [{"language": "fr",
            "problem": "different action", "confidence": 0.4}]}"#;
        let verdict = parse_verdict(content).expect("parse");
        assert!((verdict.issues[0].confidence - 0.4).abs() < f64::EPSILON);
    }

    /// Balanced-object extraction ignores braces inside JSON strings.
    #[test]
    fn finds_balanced_object_with_braces_in_strings() {
        let text = r#"noise {"a": "{not a block}", "b": {"c": 1}} trailing"#;
        assert_eq!(
            first_json_object(text),
            Some(r#"{"a": "{not a block}", "b": {"c": 1}}"#)
        );
    }

    /// The derived schema satisfies strict structured-output requirements.
    #[test]
    fn schema_derives_with_closed_objects() {
        let schema = serde_json::to_value(schemars::schema_for!(Verdict)).expect("schema");
        // Strict structured output requires closed objects; `deny_unknown_fields`
        // must surface as `additionalProperties: false` at the top level.
        assert_eq!(schema["additionalProperties"], serde_json::json!(false));
        // Strict mode also requires every property to be required. `confidence`
        // has a serde parse default, so `#[schemars(required)]` must keep it in
        // the schema's required list or strict requests would be rejected.
        let issue_required = &schema["$defs"]["Issue"]["required"];
        let required: Vec<&str> = issue_required
            .as_array()
            .expect("required array")
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect();
        assert!(required.contains(&"confidence"), "required: {required:?}");
    }
}
