# Atuin Abacus

Lightweight metrics API for [Atuin](https://atuin.sh) shell history.

## Setup

```bash
cp .env.example .env
# Edit .env with your database config
deno task dev
```

## API

### GET /history

Returns daily command counts. Optional `?start=YYYY-MM-DD&end=YYYY-MM-DD` params.

### GET /time-of-day

Returns hourly command distribution (24 elements). Optional date params.

### GET /health

Database health check.

## Development

```bash
deno task dev          # Start with auto-reload
deno task test         # Run tests
deno task format       # Format code
deno task lint         # Lint code
```
