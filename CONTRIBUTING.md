# Contributing

## Development standards

- Keep changes focused and follow existing crate boundaries and Rust patterns.
- Use English for source comments, user-facing strings, and documentation.
- Prefer core traits for cross-crate service contracts; keep platform APIs
  behind the `platform` crate.
- Keep blocking work out of Iced `update`/`view` and async executor tasks.
- Propagate errors with useful context; do not silently replace failures with
  success-shaped values.
- Add focused tests for new behavior and preserve cancellation and shutdown
  semantics.

## Checks

Before opening a pull request, run:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Windows packaging changes should also be checked with:

```powershell
cargo build --release --package app
cargo wix --package app --include wix\main.wxs
```

The full release benchmark is intended for a stable Windows runner:
`cargo bench --package app --bench release_hardening`.

## Commits and pull requests

- Use concise imperative commit subjects, optionally prefixed by area, such as
  `platform: add tray shutdown cleanup`.
- Keep unrelated changes in separate commits.
- Pull requests should explain the user-visible change, implementation
  tradeoffs, and validation performed. Include screenshots for visual changes
  when a Windows capture is available.
- Link related issues, call out Windows-only manual checks, and keep CI green.
- Do not commit credentials, signing keys, generated build outputs, or private
  certificates.

Release tags are created by maintainers after merge. The `v0.1.0` release tag
must be created only after the release PR has merged and its Windows package
has been verified.
