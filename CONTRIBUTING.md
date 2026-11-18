# Contributing

The library owns the typed model, KDL codec, capture readers, shared support analysis, native renderer, and atomic artifacts. The binary routes arguments and output.

Read [AGENTS.md](AGENTS.md) and [INTENT.md](INTENT.md). The owner-approved schema is in [docs/schema.md](docs/schema.md). Changes to syntax or meaning need owner approval.

## Verification boundary

Never execute generated .macos files anywhere, including CI, VMs, and disposable domains. Never run emitted commands individually or reproduce their effects through an API. Never change actual settings or restart apps/services for tests.

Inspect entry points, build scripts, tasks, hooks, and CI before running tests. Use synthetic fixtures and mocks. Capture tests use offline trees and CaptureReader, never capture_live. Renderer tests inspect text or parse payloads. CLI tests launch Raskin only for offline capture, generation, help, and usage specification output.

No live application result can be inferred from tests.

## Toolchain and checks

mise.toml pins Rust 1.96 and developer tools. Cargo.lock is committed. No hook is installed automatically.

~~~sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
bash scripts/gen-docs.sh --check
~~~

The mise check task runs those checks. hk.pkl defines formatting, clippy, and documentation checks for contributors who choose to install hooks.

## Generated interface documentation

--usage-spec emits a jdx/usage contract from clap metadata. It is separate from preference KDL.

~~~sh
bash scripts/gen-docs.sh
~~~

This generates the usage specification, Markdown reference, manpage, and bash/zsh/fish completions. --check compares artifacts without modifying them. Completions require usage. Tests never source completions.

## Mapping and dependency changes

Add primary command/API evidence and synthetic fixtures. Retain types and scope. Never use a settings mutation as verification. Support checks belong in the shared analysis used by capture and generation. Unsupported mappings need explicit diagnostics. Test new captures through KDL round trips and strict generation; never duplicate generator support rules in capture.

plist is exactly pinned because capture uses its streaming API to reject duplicate keys before materialization. Retain those tests when updating it.

## Local packaging

dist-workspace.toml prepares macOS arm64/x86_64 archives and a shell installer. Binstall metadata names corresponding future archives. No release is published or automatic publication workflow enabled.

Fixture CI targets Linux and macOS and includes an advisory audit. Its results are not a live migration certification.
