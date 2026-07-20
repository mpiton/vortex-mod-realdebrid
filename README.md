# vortex-mod-realdebrid

Real-Debrid debrid WASM plugin for [Vortex](https://github.com/mpiton/vortex).

Hands a covered hoster link to Real-Debrid and returns the direct CDN URL
Vortex should download from, never the original hoster page. The API token
lives in the host keyring and is read through `get_credential`; it is never
logged, persisted, or echoed into an error message.

A debrid has no anonymous mode — every export needs the token, so a missing
credential is a typed failure rather than a silent free-tier attempt.

## Features

- Coverage check against a static list of ~35 file hosters (`can_handle`),
  deliberately excluding streaming sites so a debrid never diverts a URL
  that Vortex's own crawler plugins own
- `POST /unrestrict/link` → direct CDN URL, filename, size, and resume
  support inferred from the advertised chunk count
- Multi-file payloads (Real-Debrid answers with a JSON array for some
  hosters) resolve to the first entry
- Account validation through `GET /user`, rejecting free accounts and
  reporting premium expiry to the Accounts view
- ISO-8601 expiry parsed locally — a WASM plugin has no clock, so the
  absolute timestamp cannot be derived from the relative `premium` field
- Real-Debrid's numeric `error_code` mapped to stable machine codes so the
  host's resolution cascade knows *why* a tier declined:

  | `error_code` | Plugin code | Cascade meaning |
  |---|---|---|
  | 8, 9, 12–15, 22 | `ACCOUNT_INVALID_CREDENTIALS` | token is bad, stop using it |
  | 20 | `ACCOUNT_EXPIRED` | premium lapsed |
  | 5, 21, 34 | `ACCOUNT_COOLDOWN` | back off, retry later |
  | 18, 23, 36 | `ACCOUNT_QUOTA_EXCEEDED` | traffic exhausted |
  | 6, 7, 16, 17, 19, 24, 35 | `HOSTER_NO_FILE` | fall through to the next tier |
  | anything else | `PLUGIN_ERROR` | unknown, treated as a hard failure |

- No path can produce a file entry without a direct URL. An empty
  `download` field is a hard failure, not a success with a missing link.

Traffic is not reported: Real-Debrid premium has no scalar byte quota, only
a per-hoster map behind `/traffic`. Only the expiry is surfaced.

## Build

```bash
# Lint
cargo clippy --all-targets -- -D warnings

# WASM artefact (required by the smoke test)
rustup target add wasm32-wasip1   # one-time
cargo build --target wasm32-wasip1 --release
# target/wasm32-wasip1/release/vortex_mod_realdebrid.wasm

# Native, fixture, and mandatory WASM smoke tests
cargo test
```

## Install (development)

```bash
PLUGIN_NAME="vortex-mod-realdebrid"
PLUGIN_DIR="$HOME/.local/share/dev.vortex.app/plugins/$PLUGIN_NAME"

mkdir -p "$PLUGIN_DIR"
cp target/wasm32-wasip1/release/vortex_mod_realdebrid.wasm "$PLUGIN_DIR/plugin.wasm"
cp plugin.toml "$PLUGIN_DIR/plugin.toml"
```

Vortex hot-reloads the plugin via the file watcher.

## Configure

Get an API token from <https://real-debrid.com/apitoken> and store it under
the plugin's own service name:

- **Service**: `vortex-mod-realdebrid`
- **Username**: anything (unused)
- **Password**: your API token

The plugin reads it through the host's `get_credential` host function
(scoped — only `vortex-mod-realdebrid` can read this slot).

## License

GPL-3.0 — see [`LICENSE`](./LICENSE).
