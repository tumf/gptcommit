use anyhow::{anyhow, bail, Ok, Result};
use std::fmt;
use std::fmt::Debug;
use std::time::Duration;

use async_trait::async_trait;

use reqwest::{tls, Proxy};
use tiktoken_rs::{async_openai::get_chat_completion_max_tokens, get_completion_max_tokens};

const DEFAULT_MAX_TOKENS: usize = 4096;
const FALLBACK_MODEL: &str = "gpt-4o";

use crate::{settings::OpenAISettings, util::HTTP_USER_AGENT};
use async_openai::{
    config::{OpenAIConfig, OPENAI_API_BASE},
    types::{
        ChatCompletionRequestMessageArgs, CreateChatCompletionRequestArgs,
        CreateCompletionRequestArgs, Role,
    },
    Client,
};

use super::llm_client::LlmClient;
const COMPLETION_TOKEN_LIMIT: usize = 100;

pub(crate) struct OpenAIClient {
    model: String,
    client: Client<OpenAIConfig>,
}

impl Debug for OpenAIClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenAIClient")
            .field("model", &self.model)
            .finish()
    }
}

impl OpenAIClient {
    pub(crate) fn new(settings: OpenAISettings) -> Result<Self, anyhow::Error> {
        let api_base = settings
            .api_base
            .unwrap_or_else(|| OPENAI_API_BASE.to_string());
        let api_key = settings.api_key.unwrap_or_default();

        let openai_config = OpenAIConfig::new()
            .with_api_base(&api_base)
            .with_api_key(&api_key);

        let mut openai_client = Client::<OpenAIConfig>::with_config(openai_config);

        if api_base == OPENAI_API_BASE && api_key.is_empty() {
            bail!("No OpenAI API key found. Please provide a valid API key.");
        }
        // TODO make configurable
        let mut http_client = reqwest::Client::builder()
            .gzip(true)
            .brotli(true)
            .timeout(Duration::from_secs(60))
            .user_agent(HTTP_USER_AGENT);

        if api_base == OPENAI_API_BASE {
            // Optimized HTTP client
            http_client = http_client
                .http2_prior_knowledge()
                .https_only(true)
                .http2_adaptive_window(true)
                .tcp_keepalive(Duration::from_secs(60))
                .http2_keep_alive_interval(Duration::from_secs(60))
                .http2_keep_alive_while_idle(true)
                .min_tls_version(tls::Version::TLS_1_2);
        }
        let model = settings.model.unwrap_or_default();
        if api_base == OPENAI_API_BASE && model.is_empty() {
            bail!("No OpenAI model configured. Please choose a valid model to use.");
        }

        if let Some(proxy) = settings.proxy {
            if !proxy.is_empty() {
                http_client = http_client.proxy(Proxy::all(proxy)?);
            }
        }
        openai_client = openai_client.with_http_client(http_client.build()?);

        if settings.retries.unwrap_or_default() > 0 {
            let backoff = backoff::ExponentialBackoffBuilder::new()
                .with_max_elapsed_time(Some(std::time::Duration::from_secs(60)))
                .build();
            openai_client = openai_client.with_backoff(backoff);
        }
        Ok(Self {
            model,
            client: openai_client,
        })
    }

    pub(crate) fn should_use_chat_completion(model: &str) -> bool {
        let model = model.to_lowercase();
        // Only use the legacy completions API for known old models
        let legacy_models = [
            "text-davinci",
            "text-curie",
            "text-babbage",
            "text-ada",
            "code-",
        ];
        !legacy_models.iter().any(|prefix| model.starts_with(prefix))
    }

    fn fallback_model_for_token_counting(model: &str) -> Option<&'static str> {
        (model != FALLBACK_MODEL).then_some(FALLBACK_MODEL)
    }

    fn guess_context_size(model: &str) -> usize {
        if model.to_lowercase().starts_with("gpt-") {
            128_000
        } else {
            tiktoken_rs::model::get_context_size(model)
        }
    }

    fn token_limit_with_fallback<F>(model: &str, mut token_limit: F) -> usize
    where
        F: FnMut(&str) -> Result<usize>,
    {
        match token_limit(model) {
            std::result::Result::Ok(limit) => limit,
            Err(model_error) => {
                let Some(fallback_model) = Self::fallback_model_for_token_counting(model) else {
                    warn!(
                        "Tokenizer lookup failed for fallback model '{}', using default limit {}: {}",
                        model, DEFAULT_MAX_TOKENS, model_error
                    );
                    return DEFAULT_MAX_TOKENS;
                };

                warn!(
                    "Unknown model '{}', using {} as tokenizer proxy: {}",
                    model, fallback_model, model_error
                );
                match token_limit(fallback_model) {
                    std::result::Result::Ok(fallback_limit) => {
                        Self::adjust_fallback_token_limit(model, fallback_model, fallback_limit)
                    }
                    Err(fallback_error) => {
                        warn!(
                            "Tokenizer proxy '{}' also failed for model '{}', using default limit {}: {}",
                            fallback_model, model, DEFAULT_MAX_TOKENS, fallback_error
                        );
                        DEFAULT_MAX_TOKENS
                    }
                }
            }
        }
    }

    fn adjust_fallback_token_limit(
        original_model: &str,
        fallback_model: &str,
        fallback_limit: usize,
    ) -> usize {
        let fallback_context_size = tiktoken_rs::model::get_context_size(fallback_model);
        let prompt_tokens = fallback_context_size.saturating_sub(fallback_limit);
        Self::guess_context_size(original_model).saturating_sub(prompt_tokens)
    }

    fn get_completion_prompt_token_limit(model: &str, prompt: &str) -> usize {
        Self::token_limit_with_fallback(model, |tokenizer_model| {
            get_completion_max_tokens(tokenizer_model, prompt)
        })
    }

    fn get_chat_prompt_token_limit(
        model: &str,
        messages: &[async_openai::types::ChatCompletionRequestMessage],
    ) -> usize {
        Self::token_limit_with_fallback(model, |tokenizer_model| {
            get_chat_completion_max_tokens(tokenizer_model, messages)
        })
    }

    pub(crate) async fn get_completions(&self, prompt: &str) -> Result<String> {
        let prompt_token_limit = Self::get_completion_prompt_token_limit(&self.model, prompt);

        if prompt_token_limit < COMPLETION_TOKEN_LIMIT {
            let error_msg =
"Skipping... The diff is too large for the current model. Consider using a model with a larger context window.".to_string();
            warn!("{}", error_msg);
            bail!(error_msg)
        }
        // Create request using builder pattern
        let request = CreateCompletionRequestArgs::default()
            .model(&self.model)
            .prompt(prompt)
            .max_tokens(prompt_token_limit as u16)
            .temperature(0.5)
            .top_p(1.)
            .frequency_penalty(0.)
            .presence_penalty(0.)
            .build()?;

        debug!("Sending request to OpenAI:\n{:?}", request);

        let response = self
            .client
            .completions() // Get the API "group" (completions, images, etc.) from the client
            .create(request) // Make the API call in that "group"
            .await?;

        let completion = response
            .choices
            .first()
            .ok_or(anyhow!("No completion results returned from OpenAI."))
            .map(|c| c.text.clone());

        completion
    }

    pub(crate) async fn get_chat_completions(&self, prompt: &str) -> Result<String> {
        let messages = [ChatCompletionRequestMessageArgs::default()
            .role(Role::User)
            .content(prompt)
            .build()?];
        let prompt_token_limit = Self::get_chat_prompt_token_limit(&self.model, &messages);

        if prompt_token_limit < COMPLETION_TOKEN_LIMIT {
            let error_msg =
                "skipping... diff is too large for the model. Consider using a model with a larger context window.".to_string();
            warn!("{}", error_msg);
            bail!(error_msg)
        }

        let request = CreateChatCompletionRequestArgs::default()
            .model(&self.model)
            .messages(messages)
            .build()?;

        let response = self.client.chat().create(request).await?;

        if let Some(choice) = response.choices.into_iter().next() {
            debug!(
                "{}: Role: {}  Content: {}",
                choice.index,
                choice.message.role,
                choice.message.content.clone().unwrap_or_default()
            );

            return choice
                .message
                .content
                .ok_or(anyhow!("No completion results returned from OpenAI."));
        }

        bail!("No completion results returned from OpenAI.")
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    #[test]
    fn known_completion_model_uses_existing_tiktoken_limit() {
        let prompt = "Summarize this small diff.";

        let expected = get_completion_max_tokens("gpt-4", prompt).unwrap();
        let actual = OpenAIClient::get_completion_prompt_token_limit("gpt-4", prompt);

        assert_eq!(actual, expected);
    }

    #[test]
    fn unknown_gpt_completion_model_uses_fallback_proxy() {
        let prompt = "Summarize this small diff.";

        let limit = OpenAIClient::get_completion_prompt_token_limit("gpt-5.3-codex-spark", prompt);

        assert!(
            limit > 100_000,
            "unknown gpt-* models should keep a modern context window, got {limit}"
        );
    }

    #[test]
    fn unknown_gpt_chat_model_uses_fallback_proxy() {
        let messages = [ChatCompletionRequestMessageArgs::default()
            .role(Role::User)
            .content("Summarize this small diff.")
            .build()
            .unwrap()];

        let limit = OpenAIClient::get_chat_prompt_token_limit("gpt-5.3-codex-spark", &messages);

        assert!(
            limit > 100_000,
            "unknown gpt-* chat models should keep a modern context window, got {limit}"
        );
    }

    #[test]
    fn token_limit_falls_back_to_default_when_proxy_fails() {
        let calls = RefCell::new(Vec::new());

        let limit = OpenAIClient::token_limit_with_fallback("gpt-5.3-codex-spark", |model| {
            calls.borrow_mut().push(model.to_string());
            Err(anyhow!("forced tokenizer failure"))
        });

        assert_eq!(limit, DEFAULT_MAX_TOKENS);
        assert_eq!(
            calls.into_inner(),
            vec![
                "gpt-5.3-codex-spark".to_string(),
                FALLBACK_MODEL.to_string()
            ]
        );
    }

    #[test]
    fn fallback_limit_uses_original_gpt_context_size() {
        let fallback_prompt_tokens = 42;
        let fallback_limit =
            tiktoken_rs::model::get_context_size(FALLBACK_MODEL) - fallback_prompt_tokens;

        let adjusted = OpenAIClient::adjust_fallback_token_limit(
            "gpt-5.3-codex-spark",
            FALLBACK_MODEL,
            fallback_limit,
        );

        assert_eq!(adjusted, 128_000 - fallback_prompt_tokens);
    }
}

#[async_trait]
impl LlmClient for OpenAIClient {
    /// Sends a request to OpenAI's API to get a text completion.
    /// It takes a prompt as input, and returns the completion.
    async fn completions(&self, prompt: &str) -> Result<String> {
        let completion = if OpenAIClient::should_use_chat_completion(&self.model) {
            self.get_chat_completions(prompt).await?
        } else {
            self.get_completions(prompt).await?
        };
        Ok(completion.trim().to_string())
    }
}
