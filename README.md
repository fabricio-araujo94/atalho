# Atalho

An extensible, lightning-fast desktop application launcher and command palette built with Rust, following Clean Architecture principles.

## Project Scope

Atalho (Portuguese for *Shortcut*) provides an instant keyboard-driven interface to search, discover, and execute desktop applications and system commands. The architecture enforces strict separation of concerns, dependency inversion, and modularity across dedicated workspace crates.

## Architecture

The workspace is organized following Clean Architecture layers:

```text
               ┌────────────────────────┐
               │          app           │
               └───────────┬────────────┘
                           │
       ┌──────────────┬────┴─────┬──────────────┐
       ▼              ▼          ▼              ▼
     [ ui ]      [ executor ] [ indexer ]  [ persistence ]
   (iced GUI)         │          │              │
       │              ▼          │              │
       │        [ platform ]     │              │
       │     (windows-rs API)    │              │
       │              │          │              │
       └──────────────┼──────────┴──────────────┘
                      ▼
                  [ core ]
          (domain types & traits)
                      ▲
                      │
                 [ matcher ]
              (fuzzy ranking)
```

### Workspace Member Crates

| Crate | Responsibility | Dependencies |
|---|---|---|
| `core` | Domain models (`AppEntry`, `CommandEntry`, `SearchResult`, `Score`), contracts (`Indexer`, `Matcher`, `Executor`, `Persistence`), and domain error types (`AppError`). Zero external domain or platform dependencies. | `thiserror`, `serde`, `async-trait` |
| `indexer` | Discovers and indexes installed applications and executable system commands. | `core` |
| `matcher` | High-performance fuzzy matching and relevance scoring engine. | `core` |
| `executor` | Executes matched applications, scripts, or system commands. | `core`, `platform` |
| `persistence` | Storage layer for search history, preferences, and cache. | `core` |
| `platform` | Operating system abstractions and native Windows APIs (via `windows-rs` gated behind `cfg(windows)`). | `core`, `windows` |
| `ui` | Graphical user interface powered by [Iced](https://iced.rs). | `core`, `iced` |
| `app` | Binary application entry point and composition root wiring all components together. | All crates above |

## Prerequisites

- Rust stable toolchain (Edition 2024, pinned in `rust-toolchain.toml`)
- [Just](https://github.com/casey/just) command runner (optional, standard `cargo` commands can also be used)

## Development

A `justfile` is included with tasks for standard workflows:

```bash
# Build all workspace crates
just build
# or: cargo build --workspace

# Run all workspace unit and integration tests
just test
# or: cargo test --workspace

# Run clippy with strict warning enforcement
just lint
# or: cargo clippy --all-targets --all-features -- -D warnings

# Check code formatting
just fmt-check
# or: cargo fmt --check

# Format code
just fmt
# or: cargo fmt

# Launch the desktop application
just run
# or: cargo run -p app
```

## Quality Standards

- **Strict Clippy Configuration**: `clippy::all`, `clippy::pedantic`, and `clippy::nursery` enabled as warnings; `clippy::unwrap_used` strictly denied.
- **Robust Error Handling**: Library crates use domain `Result` propagation with `AppError` without unwrap or panic.
- **Documentation**: All public types, functions, methods, and traits are thoroughly documented in English.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
