## ADDED Requirements

### Requirement: Two-tier tokenizer fallback for unknown models

When `tiktoken-rs` cannot resolve the tokenizer for the configured model,
gptcommit SHALL retry token counting with `gpt-4o` as a proxy model name
before falling back to a hardcoded default limit.

#### Scenario: Unknown gpt-5 model falls back to proxy

**Given**: gptcommit is configured with `openai.model = "gpt-5.3-codex-spark"`
**And**: `tiktoken_rs::get_chat_completion_max_tokens("gpt-5.3-codex-spark", ...)` returns `Err`
**When**: gptcommit processes a commit diff
**Then**: it retries with `get_chat_completion_max_tokens("gpt-4o", ...)`
**And**: uses the result (approx 128K context window) as the token limit
**And**: logs a warning that proxy model is in use

#### Scenario: Both model and proxy fail

**Given**: gptcommit is configured with an unknown model
**And**: `tiktoken_rs` fails for both the configured model AND the proxy model `gpt-4o`
**When**: gptcommit processes a commit diff
**Then**: it falls back to `DEFAULT_MAX_TOKENS` (4096)
**And**: logs a distinct warning about the hard fallback

#### Scenario: Known model continues to work normally

**Given**: gptcommit is configured with `openai.model = "gpt-4"`
**When**: gptcommit processes a commit diff
**Then**: `tiktoken_rs` resolves the tokenizer directly (no fallback needed)
**And**: no fallback warning is logged

#### Scenario: Non-chat completion path uses same fallback

**Given**: gptcommit is configured with an unknown model routed to the legacy completions API
**When**: `get_completion_max_tokens` fails for the configured model
**Then**: it retries with `gpt-4o` as proxy before falling back to `DEFAULT_MAX_TOKENS`
