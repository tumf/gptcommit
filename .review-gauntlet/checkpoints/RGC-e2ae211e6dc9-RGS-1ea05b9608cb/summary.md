# Review Gauntlet Checkpoint

- Checkpoint state: complete
- Usable as review base: True
- Review base commit: e2ae211e6dc9f2dd724ee3fccb64e3de9bf7b52d
- Session ID: RGS-1ea05b9608cb
- Created at: 2026-06-19T09:29:37.343486Z (generation timestamp)

## Coverage

| State | Count |
| --- | ---: |
| reviewed | 91 |

## Findings

| ID | State | Path | Rule | Content |
| --- | --- | --- | --- | --- |
| RGF-0001 | confirmed | .devcontainer/devcontainer.json | data-validation | `cargo install just` fetches and builds the latest `just` at container creation time, making devcontainer setup non-reproducible and dependent on current crates.io state. Pin the tool version so rebuilds use the same dependency graph. |
| RGF-0002 | confirmed | .devcontainer/devcontainer.json | ci-reproducibility | The devcontainer base image is referenced by a mutable tag rather than an immutable digest, so CI or developer environments can change when the upstream tag is rebuilt. Pin the image to a digest, or use a project-controlled Dockerfile, to make the container reproducible. |
| RGF-0003 | confirmed | .devcontainer/devcontainer.json | ci-reproducibility | The post-create step installs the latest available `just` each time, which makes the development container non-reproducible and can introduce unexpected breakage. Pin the installed crate version. |
| RGF-0004 | confirmed | .env.example | cli-contract | The example still documents OPENAI_API_KEY even though the CLI help marks it as deprecated and asks users to prefer GPTCOMMIT__OPENAI__API_KEY. Keeping the deprecated variable in the primary env example can lead new installations to depend on the legacy compatibility path instead of the stable CLI-specific contract. |
| RGF-0005 | confirmed | .env.example | docs-accuracy | The example points users to the old beta.openai.com API-key page, while the project README uses platform.openai.com/account/api-keys for the same setup flow. This stale setup URL can send users to outdated documentation when configuring the required key. |

## Triage Events

| Event | Finding | From | To | Reason |
| ---: | --- | --- | --- | --- |
| 1 | RGF-0001 | open | confirmed | Line 20 runs cargo install just without --version or --locked, so devcontainer creation resolves the latest crate/dependencies at runtime. |
| 2 | RGF-0002 | open | confirmed | Line 6 uses mcr.microsoft.com/devcontainers/rust:0-1-bullseye without an @sha256 digest, so the base image can change when the mutable tag is rebuilt. |
| 3 | RGF-0003 | open | confirmed | Line 20 installs just without a pinned version, making the devcontainer post-create environment dependent on the current crates.io latest release. |
| 4 | RGF-0004 | open | confirmed | Confirmed: .env.example line 2 documents OPENAI_API_KEY, while src/help.rs tells users OPENAI_API_KEY is deprecated and to prefer GPTCOMMIT__OPENAI__API_KEY or config. |
| 5 | RGF-0005 | open | confirmed | Confirmed: .env.example line 1 uses the old beta.openai.com API key URL, while README.md line 168 uses platform.openai.com/account/api-keys. |

## Blockers

None
