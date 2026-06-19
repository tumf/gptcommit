# review-gauntlet OCR Review Prompt

You are an external code review CLI. Produce verdict JSON matching the contract.
Do not include provider credentials or markdown fences in the verdict JSON.

## Output Contract
Write the final verdict JSON to this file:
/Users/tumf/work/gptcommit/.review-gauntlet/runs/1/cells/RGC-a260a8736c4cd5bd/verdict.json
Before finishing, validate the file with:
review-gauntlet validate-verdict /Users/tumf/work/gptcommit/.review-gauntlet/runs/1/cells/RGC-a260a8736c4cd5bd/verdict.json --expected-path .devcontainer/devcontainer.json
If validation fails, fix the JSON file and run the validator again.
Stdout and stderr are audit/progress channels only; they are preserved but not parsed as verdict input.
Do not rely on stdout or stderr to deliver the verdict when file-json output is active.

## Repository Context
repository_root: /Users/tumf/work/gptcommit
ruleset_digest: 8f6075fa8b3d8ad3606e0f850277b73de14cf786552139cd817a8bd18d34477c

## Review Cell
cell_id: RGC-a260a8736c4cd5bd
file_path: .devcontainer/devcontainer.json
slice_id: config
rule_id: ci-reproducibility
content_digest: 29f70056ccfffc1b60df13cea427aaf9e62928f5af6b4c2b6b0b25a9eec8514e
file_size_bytes: 1160
line_count: 25

Source file contents are not embedded in this prompt. When source inspection is needed, read the target file from repository_root plus file_path.

## Review Scope Guardrails
Only report issues whose JSON path exactly equals the Review Cell file_path above.
You may inspect related files to understand context, but do not emit comments for related files or helper files.
If the only issue you find is in a different file, return an empty comments array.
A verdict comment whose path differs from file_path will be rejected by the adapter.

## Selected Rule
rule_document: json.md
# OCR Rule Attribution

Ported review guidance derived from Alibaba open-code-review at commit c323c6b40c72aa95d7cb801bedcb957b52ff9807 (Apache-2.0). Review for correctness, security, maintainability, and actionable line-level comments.

## json.md

Apply OCR-style checks for this file family.


## Verdict JSON Contract
The verdict object must contain exactly one top-level key: comments.
Each comment object may contain only these keys: path, content, suggestion_code, existing_code, start_line, end_line, thinking.
Do not include rule_id, cell_id, severity, confidence, title, category, metadata, or any other keys.
Every comment.path MUST equal: .devcontainer/devcontainer.json
{
  "comments": [
    {
      "content": "Issue description for this exact review cell path only",
      "end_line": 1,
      "existing_code": "Existing code",
      "path": ".devcontainer/devcontainer.json",
      "start_line": 1,
      "suggestion_code": "Suggested code",
      "thinking": "Optional reasoning"
    }
  ]
}
