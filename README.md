# Atuin Abacus

[![Build Status](https://github.com/evanpurkhiser/atuin-abacus/actions/workflows/main.yml/badge.svg)](https://github.com/evanpurkhiser/atuin-abacus/actions/workflows/main.yml)

Atuin Abacus is a metrics API for shell history
and captured coding-agent conversations.
It builds local projections of encrypted shell-history
and coding-agent records downloaded from an Atuin sync server.

## Shell commands

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://apis.evanpurkhiser.com/atuin-abacus/shell-graph.svg?period=1y&cellBackground=%23161b22&textColor=%23c9d1d9">
  <source media="(prefers-color-scheme: light)" srcset="https://apis.evanpurkhiser.com/atuin-abacus/shell-graph.svg?period=1y">
  <img alt="Shell History Activity Graph" src="https://apis.evanpurkhiser.com/atuin-abacus/shell-graph.svg?period=1y">
</picture>

## Agent prompts

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://apis.evanpurkhiser.com/atuin-abacus/agent-graph.svg?period=1y&color=%233b82f6&cellBackground=%23161b22&textColor=%23c9d1d9">
  <source media="(prefers-color-scheme: light)" srcset="https://apis.evanpurkhiser.com/atuin-abacus/agent-graph.svg?period=1y&color=%233b82f6">
  <img alt="Agent Prompt Activity Graph" src="https://apis.evanpurkhiser.com/atuin-abacus/agent-graph.svg?period=1y&color=%233b82f6">
</picture>

## Synchronization

Abacus acts as a download-only Atuin client:

1. It synchronizes encrypted history and `ai-session` records into `records.db`.
2. It uses the account encryption key to project them into `history.db`
   and `threads.db`.
3. It snapshots command timestamps and classifies visible human prompts
   and assistant responses into an in-memory event snapshot used by the API.

The record store and projection live under `ABACUS_DATA_DIR`
and must be on a persistent filesystem.
An initial sync can be several gigabytes;
subsequent syncs use Atuin's record diff and projection watermarks.

Abacus logs in to the sync server on first startup
and persists the resulting session and encryption key under `ABACUS_DATA_DIR`.
It does not write captured records or upload local state.

## API

Every time-series endpoint accepts:

- `start` and `end` in `YYYY-MM-DD` format.
- `period`, such as `1y`, `6m`, or `30d`, which takes precedence over dates.
- `rollup`, a fixed duration such as `30s`, `5m`, `2h`, or `7d` where supported.
- `Prefer: timezone=America/New_York` to select the bucketing timezone.

### `GET /shell-history`

Returns command counts.
Buckets default to one day.

```json
[
  {"date": "2026-11-11", "count": 234},
  {"date": "2026-11-12", "count": 189}
]
```

### `GET /agent-history`

Returns human prompts and visible assistant responses.
Copied fork history is deduplicated by root session,
split assistant messages are counted once per turn,
and subagent-only conversations and tool results are excluded.

```json
[
  {"date": "2026-11-11", "prompts": 82, "responses": 117},
  {"date": "2026-11-12", "prompts": 64, "responses": 91}
]
```

### Other endpoints

- `GET /` returns the total command count and most recent command timestamp.
- `GET /time-of-day` returns average command counts for each hour.
- `GET /shell-graph.svg` returns the shell-command contribution graph.
- `GET /agent-graph.svg` returns a contribution graph driven by human prompts.

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `HOST` | `127.0.0.1` | HTTP listen address |
| `PORT` | `8000` | HTTP listen port |
| `CACHE_TTL_SECONDS` | `300` | Response cache-control lifetime |
| `ATUIN_SYNC_INTERVAL_SECONDS` | `300` | Delay between completed sync attempts |
| `ABACUS_DATA_DIR` | `.local/atuin-abacus` | Persistent record and projection storage |
| `ATUIN_SYNC_ADDRESS` | required | Atuin sync server URL |
| `ATUIN_USERNAME` | required | Atuin account username |
| `ATUIN_PASSWORD` | required | Atuin account password |
| `ATUIN_ENCRYPTION_KEY` | required | Atuin account encryption key |
| `SENTRY_DSN` | unset | Sentry error-reporting DSN |
| `SENTRY_ENVIRONMENT` | `production` | Sentry environment |
| `SENTRY_RELEASE` | unset | Sentry release identifier |

## Development

Rust 1.96 or newer is required.

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
```

Run one synchronization without starting HTTP:

```bash
ABACUS_DATA_DIR=.local/atuin-abacus \
cargo run -- sync-once
```

Run the service:

```bash
cp .env.example .env
cargo run
```

## Container

The image runs as UID 10001
and stores synchronized state in `/var/lib/atuin-abacus`.

```bash
docker run --rm \
  -p 8888:8888 \
  -e ATUIN_SYNC_ADDRESS=https://atuin.example.com \
  -e ATUIN_USERNAME=abacus \
  -e ATUIN_PASSWORD=... \
  -e ATUIN_ENCRYPTION_KEY=... \
  -v atuin-abacus-data:/var/lib/atuin-abacus \
  ghcr.io/evanpurkhiser/atuin-abacus:latest
```
