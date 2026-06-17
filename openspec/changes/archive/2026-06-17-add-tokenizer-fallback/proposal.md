---
change_type: implementation
priority: medium
dependencies: []
references:
  - src/llms/openai.rs
  - Cargo.toml (tiktoken-rs v0.6.0)
---

# Add intelligent tokenizer fallback for unknown models

**Change Type**: implementation

## Problem / Context

`tiktoken-rs` v0.6.0 has a hardcoded model-to-tokenizer mapping. When gptcommit is configured with a model name not in that table (e.g., `gpt-5.3-codex-spark`, `gpt-4.1-nano`, or any custom/Ollama model), `get_chat_completion_max_tokens` and `get_completion_max_tokens` return an error.

Commit `509e036` introduced a fallback that catches the error and falls back to `DEFAULT_MAX_TOKENS = 4096`. This prevents a crash, but it causes two problems:

1. **Under-estimation for large models**: Models with 128K+ context windows are capped at 4096, so valid diffs are incorrectly rejected as "too large."
2. **Silent degradation**: The warning is logged, but the user gets a wrong size cap with no obvious indication that their model is being incorrectly sized.

In practice, all OpenAI models from GPT-4 onward use either `cl100k_base` (GPT-4 family) or `o200k_base` (GPT-4o/o1 family). Using `gpt-4o` as a proxy model name for token counting would give accurate token estimates for virtually all modern models.

## Proposed Solution

Implement a two-tier fallback in `src/llms/openai.rs`:

1. **Tier 1 — Proxy model**: When `tiktoken_rs` fails for the configured model, retry with `"gpt-4o"` as the model name. This selects `o200k_base` tokenizer and a 128K context window — appropriate for all GPT-4o/o1/gpt-5 class models.

2. **Tier 2 — Hard default**: If even the proxy model fails (extremely unlikely, but defensive), fall back to `DEFAULT_MAX_TOKENS` (4096) as before.

Also add a helper function `guess_context_size` that returns 128_000 for any model starting with `gpt-` (newer than GPT-4), so that the context size used in `get_chat_completion_max_tokens` is reasonable even when `get_context_size` from tiktoken-rs doesn't know the model.

## Acceptance Criteria

- gptcommit configured with `openai.model = "gpt-5.3-codex-spark"` does NOT crash with `No tokenizer found for model`
- Token counting for unknown `gpt-*` models uses `o200k_base` (via `gpt-4o` proxy), giving accurate estimates for 128K context models
- `get_completion_max_tokens` and `get_chat_completion_max_tokens` both use the new two-tier fallback
- Existing behavior for known models (gpt-4, gpt-3.5-turbo, etc.) is unchanged
- Warning log output clearly states when fallback is in use ("Unknown model 'X', using gpt-4o as tokenizer proxy")

## Out of Scope

- Fixing tiktoken-rs upstream (separate effort)
- Adding tokenizer support to OpenCode itself
- Changing the `tiktoken-rs` dependency version

## Explicit Completion Conditions

1. `src/llms/openai.rs` contains a helper function that takes `&str` model name and returns a fallback model name
2. Both `get_completions` and `get_chat_completions` use the two-tier fallback in their token-counting calls
3. `cargo test` passes with the existing test suite
4. Manual verification: setting `GPTCOMMIT__OPENAI__MODEL=gpt-5.3-codex-spark` and running `gptcommit prepare-commit-msg` produces a warning about fallback but does NOT error
