# Persistence formats

The default storage directory is `%LOCALAPPDATA%\FlowLauncherRust`.
`FilePersistence::from_dir` can select another directory for tests or portable
installations.

## Application index: `index.bin.zst`

The index is a bincode serialization of `Vec<AppEntry>` compressed with zstd.
Updates are written to a uniquely named `index.bin.zst.<uuid>.tmp` file, synced,
and renamed into place only after the complete compressed payload has been written.

## Launch history: `history.bin`

History is an append-only sequence of fixed-width 24-byte records. Each record
contains an unsigned little-endian Unix timestamp in seconds followed by the
16 raw bytes of the application UUID. At 10,000 records, the current file is
renamed to `history.1.bin` and a new `history.bin` is started. Only the current
and immediately preceding segment are retained.

## Configuration: `config.toml`

Configuration is UTF-8 TOML so users can edit it directly. Missing settings
use `AppConfig::default()`. Updates are written to a unique `config.toml.<uuid>.tmp`
file and renamed into place after the full TOML document has been written and synced.
