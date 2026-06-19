use anyhow::{anyhow, bail, Result};
use std::fmt;
use std::fmt::Debug;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;

use reqwest::{tls, Proxy};
use serde::Deserialize;
use tiktoken_rs::o200k_base;

const DEFAULT_CONTEXT_SIZE: usize = 128_000;

use crate::{settings::OpenAISettings, util::HTTP_USER_AGENT};
use async_openai::{
    config::{OpenAIConfig, OPENAI_API_BASE},
    types::{ChatCompletionRequestMessageArgs, CreateCompletionRequestArgs, Role},
    Client,
};

use super::llm_client::LlmClient;
const COMPLETION_TOKEN_LIMIT: usize = 100;

pub(crate) struct OpenAIClient {
    model: String,
    api_base: String,
    api_key: String,
    http_client: reqwest::Client,
    context_size: AtomicUsize,
    client: Client<OpenAIConfig>,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatCompletionChoice>,
}

#[derive(Deserialize)]
struct ChatCompletionChoice {
    message: ChatCompletionMessage,
}

#[derive(Deserialize)]
struct ChatCompletionMessage {
    content: Option<String>,
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
        let mut http_client = reqwest::Client::builder()
            .gzip(true)
            .brotli(true)
            .timeout(Duration::from_secs(60))
            .user_agent(HTTP_USER_AGENT);

        if api_base == OPENAI_API_BASE {
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
        let http_client = http_client.build()?;
        openai_client = openai_client.with_http_client(http_client.clone());

        if settings.retries.unwrap_or_default() > 0 {
            let backoff = backoff::ExponentialBackoffBuilder::new()
                .with_max_elapsed_time(Some(std::time::Duration::from_secs(60)))
                .build();
            openai_client = openai_client.with_backoff(backoff);
        }
        Ok(Self {
            model,
            api_base,
            api_key,
            http_client,
            context_size: AtomicUsize::new(0),
            client: openai_client,
        })
    }

    pub(crate) fn should_use_chat_completion(model: &str) -> bool {
        let model = model.to_lowercase();
        let legacy_models = [
            "text-davinci",
            "text-curie",
            "text-babbage",
            "text-ada",
            "code-",
        ];
        !legacy_models.iter().any(|prefix| model.starts_with(prefix))
    }

    async fn fetch_context_size(&self) -> usize {
        let cached = self.context_size.load(Ordering::Relaxed);
        if cached > 0 {
            return cached;
        }

        let url = format!(
            "{}/models/{}",
            self.api_base.trim_end_matches('/'),
            self.model
        );

        let size = match self
            .http_client
            .get(&url)
            .bearer_auth(&self.api_key)
            .send()
            .await
        {
            Ok(resp) => match resp.json::<serde_json::Value>().await {
                Ok(json) => json
                    .get("max_model_len")
                    .or_else(|| json.get("max_tokens"))
                    .or_else(|| json.get("context_window"))
                    .or_else(|| json.get("context_length"))
                    .and_then(|v: &serde_json::Value| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(DEFAULT_CONTEXT_SIZE),
                Err(e) => {
                    debug!("Failed to parse /v1/models response: {}", e);
                    DEFAULT_CONTEXT_SIZE
                }
            },
            Err(e) => {
                debug!(
                    "Failed to fetch /v1/models/{}, using default context size {}: {}",
                    self.model, DEFAULT_CONTEXT_SIZE, e
                );
                DEFAULT_CONTEXT_SIZE
            }
        };

        self.context_size.store(size, Ordering::Relaxed);
        size
    }

    fn count_tokens(text: &str) -> usize {
        o200k_base()
            .map(|bpe| bpe.encode_with_special_tokens(text).len())
            .unwrap_or_else(|_| text.len() / 4)
    }

    fn chat_max_tokens(context_size: usize, prompt: &str) -> usize {
        let prompt_tokens = Self::count_tokens(prompt);
        let overhead = 10;
        context_size.saturating_sub(prompt_tokens + overhead)
    }

    fn completion_max_tokens(context_size: usize, prompt: &str) -> usize {
        let prompt_tokens = Self::count_tokens(prompt);
        context_size.saturating_sub(prompt_tokens)
    }

    pub(crate) async fn get_completions(&self, prompt: &str) -> Result<String> {
        let context_size = self.fetch_context_size().await;
        let prompt_token_limit = Self::completion_max_tokens(context_size, prompt);

        if prompt_token_limit < COMPLETION_TOKEN_LIMIT {
            let error_msg =
            "Skipping... The diff is too large for the current model. Consider using a model with a larger context window.".to_string();
            warn!("{}", error_msg);
            bail!(error_msg)
        }
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

        let response = self.client.completions().create(request).await?;

        let completion = response
            .choices
            .first()
            .ok_or(anyhow!("No completion results returned from OpenAI."))
            .map(|c| c.text.clone());

        completion
    }

    fn normalize_chat_completion_response(body: &str) -> &str {
        let first_line = body.split('\n').next().unwrap_or(body);
        first_line
            .strip_suffix("data: [DONE]")
            .unwrap_or(first_line)
            .trim_end_matches('\n')
            .trim_end_matches('\r')
    }

    fn parse_chat_completion_response(body: &str) -> Result<String> {
        let body = Self::normalize_chat_completion_response(body);
        let response: ChatCompletionResponse = serde_json::from_str(body)?;

        response
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.message.content)
            .ok_or(anyhow!("No completion results returned from OpenAI."))
    }

    pub(crate) async fn get_chat_completions(&self, prompt: &str) -> Result<String> {
        let messages = [ChatCompletionRequestMessageArgs::default()
            .role(Role::User)
            .content(prompt)
            .build()?];
        let context_size = self.fetch_context_size().await;
        let prompt_token_limit = Self::chat_max_tokens(context_size, prompt);

        if prompt_token_limit < COMPLETION_TOKEN_LIMIT {
            let error_msg =
                "skipping... diff is too large for the model. Consider using a model with a larger context window.".to_string();
            warn!("{}", error_msg);
            bail!(error_msg)
        }

        let body = serde_json::json!({
            "model": self.model,
            "messages": messages,
        });

        let response = self
            .http_client
            .post(format!(
                "{}/chat/completions",
                self.api_base.trim_end_matches('/')
            ))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;

        Self::parse_chat_completion_response(&response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_tokens_returns_reasonable_value() {
        let tokens = OpenAIClient::count_tokens("Summarize this small diff.");
        assert!(tokens > 3 && tokens < 20, "got {tokens}");
    }

    #[test]
    fn chat_max_tokens_deducts_overhead() {
        let prompt = "Short prompt.";
        let limit = OpenAIClient::chat_max_tokens(128_000, prompt);
        let prompt_tokens = OpenAIClient::count_tokens(prompt);

        assert_eq!(limit, 128_000 - prompt_tokens - 10);
    }

    #[test]
    fn completion_max_tokens_no_overhead() {
        let prompt = "Short prompt.";
        let limit = OpenAIClient::completion_max_tokens(128_000, prompt);
        let prompt_tokens = OpenAIClient::count_tokens(prompt);

        assert_eq!(limit, 128_000 - prompt_tokens);
    }

    #[test]
    fn token_limit_clamps_at_zero() {
        let prompt = "Small prompt.";
        let context_size = OpenAIClient::count_tokens(prompt);
        let limit = OpenAIClient::chat_max_tokens(context_size, prompt);

        assert_eq!(limit, 0);
    }

    #[test]
    fn parses_event_stream_like_non_streaming_chat_response() {
        let body = r#"{"choices":[{"message":{"content":"ok"}}]}
data: [DONE]
"#;

        let completion = OpenAIClient::parse_chat_completion_response(body).unwrap();
        assert_eq!(completion, "ok");

        let single_line = r#"{"choices":[{"message":{"content":"ok"}}]}data: [DONE]
"#;
        let completion2 = OpenAIClient::parse_chat_completion_response(single_line).unwrap();
        assert_eq!(completion2, "ok");
    }
}

#[async_trait]
impl LlmClient for OpenAIClient {
    async fn completions(&self, prompt: &str) -> Result<String> {
        let completion = if OpenAIClient::should_use_chat_completion(&self.model) {
            self.get_chat_completions(prompt).await?
        } else {
            self.get_completions(prompt).await?
        };
        Ok(completion.trim().to_string())
    }
}
