# Atalho

Atalho is a keyboard-driven Windows application launcher and command
palette built with Rust and Iced. It keeps a searchable application index,
provides fuzzy-ranked results, supports launching apps and shortcuts, and can
stay in the system tray.

## Features

- Fuzzy search with cancellation and query debouncing.
- Cached application index and launch history.
- Windows shortcut launching, including shortcut elevation behavior.
- Tray menu for showing the launcher, opening settings, and quitting.
- Optional per-user sign-in startup shortcut.
- Search results ranked with historical launch frequency.

## Screenshots

Screenshots are not included yet. Add captures from a Windows 10/11 release
build here before publishing the first public release.

## Install

Download and run the MSI attached to a tagged GitHub release. The installer
targets `%ProgramFiles%\Atalho` and adds a Start Menu shortcut. The
optional **Start Atalho when I sign in** feature can be selected in
the MSI feature-selection dialog or enabled later in launcher Settings.

The MSI is currently unsigned. Review [SIGNING.md](SIGNING.md) for the
organization's release-signing procedure. The application does not add itself
to `PATH`.

## Keyboard shortcuts

| Shortcut | Action |
|---|---|
| Up / Down | Move through results |
| Tab | Select the next result |
| Enter | Launch the selected result |
| Esc | Hide the launcher without quitting |
| Ctrl+1 through Ctrl+9 | Launch the matching visible result |
| Alt+Space | Configured default launcher hotkey |

## Development setup

Requirements: stable Rust (Edition 2024), the Windows MSVC build tools for
Windows builds, and WiX Toolset 3.x plus `cargo-wix` for MSI packaging.

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo run --package app
```

Build an optimized Windows binary and installer:

```powershell
cargo build --release --package app
cargo install cargo-wix --locked
cargo wix --package app --include wix\main.wxs
```

Run the end-to-end startup, search, and memory benchmark on Windows:

```powershell
$env:BENCHMARK_TIMESTAMP = Get-Date -Format "yyyy-MM-dd HH:mm:ss zzz"
$env:BENCHMARKS_OUTPUT = Join-Path $PWD "BENCHMARKS.md"
cargo bench --package app --bench release_hardening
```

See [ARCHITECTURE.md](ARCHITECTURE.md), [CONTRIBUTING.md](CONTRIBUTING.md),
[BENCHMARKS.md](BENCHMARKS.md), and [crates/app/LIFECYCLE.md](crates/app/LIFECYCLE.md)
for implementation and project details.
