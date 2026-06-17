## Implementation Tasks

- [x] Add `FALLBACK_MODEL: &str = "gpt-4o"` constant in `src/llms/openai.rs` (verification: unit — `src/llms/openai.rs` tests reference `FALLBACK_MODEL`; runnable evidence: `cargo test token_limit_falls_back_to_default_when_proxy_fails`)

- [x] Update `get_completions()` in `src/llms/openai.rs` to use two-tier fallback: retry with `FALLBACK_MODEL` on first error, fall to `DEFAULT_MAX_TOKENS` on second error, with distinct warning messages per tier (verification: unit — `src/llms/openai.rs::tests::unknown_gpt_completion_model_uses_fallback_proxy`; runnable evidence: `cargo test unknown_gpt_completion_model_uses_fallback_proxy`)

- [x] Update `get_chat_completions()` in `src/llms/openai.rs` to use same two-tier fallback pattern (verification: unit — `src/llms/openai.rs::tests::unknown_gpt_chat_model_uses_fallback_proxy`; runnable evidence: `cargo test unknown_gpt_chat_model_uses_fallback_proxy`)

- [x] Verify `cargo test` passes with no regressions (verification: integration — `cargo test`)

- [x] Manual smoke test: set `GPTCOMMIT__OPENAI__MODEL=gpt-5.3-codex-spark` and verify no crash, correct warning logged (verification: manual — repository command evidence: `GPTCOMMIT__OPENAI__MODEL=gpt-5.3-codex-spark cargo run -- prepare-commit-msg`; source path: `src/actions/prepare_commit_msg.rs`)

## Future Work

- Upstream `tiktoken-rs` PR to add `gpt-5` and `gpt-4.1` prefix mappings
- OpenCode internal token counting fix for unknown model names

## Final Validation

Archive validation is the authoritative final gate.
Expected archive gate: `cflx openspec validate add-tokenizer-fallback --archive-gate`
