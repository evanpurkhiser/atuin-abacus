# Atuin Abacus

[![Build Status](https://github.com/evanpurkhiser/atuin-abacus/actions/workflows/main.yml/badge.svg)](https://github.com/evanpurkhiser/atuin-abacus/actions/workflows/main.yml)

📊 _Lightweight metrics API for [Atuin](https://atuin.sh) shell history_

**Atuin Abacus** is a simple REST API that provides analytics and insights from your Atuin shell
history database. Query your command usage patterns, visualize when you're most active in the
terminal, and track your productivity over time.

## Features

- **Command History Analytics**: Track your shell command usage over time
- **SVG Contribution Graph**: Generate beautiful GitHub-style visualizations of your activity
- **Time-of-Day Distribution**: See when you're most active in the terminal
- **Timezone Support**: Query metrics in any timezone via the `Prefer` header
- **Date Range Filtering**: Analyze specific time periods
- **Read-Only**: Safe to run against your production Atuin database
- **Lightweight**: Built with Deno and Hono for minimal resource usage

## Quick Start

### Using Docker (Recommended)

```bash
docker run -d \
  -p 8000:8000 \
  -e DATABASE_URL="postgresql://user:pass@host:5432/atuin" \
  ghcr.io/evanpurkhiser/atuin-abacus:latest
```

### Local Development

```bash
# Clone the repository
git clone https://github.com/evanpurkhiser/atuin-abacus.git
cd atuin-abacus

# Configure your Atuin database connection
cp .env.example .env
# Edit .env with your DATABASE_URL

# Start the server
deno task dev
```

## API Reference

### Common Parameters

All endpoints support the following parameters for filtering data by time period:

**Query Parameters:**

- `start` (optional): Start date in `YYYY-MM-DD` format
- `end` (optional): End date in `YYYY-MM-DD` format
- `period` (optional): Time period shorthand (e.g., `1y`, `6m`, `30d`). If provided, takes precedence over `start`/`end`

**Timezone Support:**

All endpoints support timezone specification via the `Prefer` header:

```bash
curl -H "Prefer: timezone=America/Los_Angeles" http://localhost:8000/history
```

### `GET /`

Get statistics including total command count and timestamp of the most recent command.

**Example Response:**

```json
{
  "total": 42891,
  "lastCommandAt": "2026-01-30T16:38:11.317Z"
}
```

### `GET /history`

Get daily command counts over time.

**Example Response:**

```json
[
  {"date": "2024-01-15", "count": 234},
  {"date": "2024-01-16", "count": 189},
  {"date": "2024-01-17", "count": 312}
]
```

### `GET /time-of-day`

Get average command distribution across 24 hours.

**Example Response:**

```json
{
  "hourly": [
    0.5, // 00:00 - 01:00
    0.2, // 01:00 - 02:00
    0.1 // 02:00 - 03:00
    // ... 24 values total
  ]
}
```

### `GET /graph.svg`

Generate a GitHub-style contribution graph SVG visualization of your command history.

**Additional Query Parameters:**

- `color` or `baseColor` (optional): Base color for the heat map (default: `#fb7185`)
- `textColor` (optional): Color for labels and text (default: `#57606a`)
- `cellBackground` (optional): Background color for empty cells (default: `#ebedf0`)
- `cellSize` (optional): Size of each cell in pixels (default: `12`)
- `cellGap` (optional): Gap between cells in pixels (default: `3`)
- `showMonthLabels` (optional): Show month labels (`true` or `false`, default: `true`)
- `showDayLabels` (optional): Show day of week labels (`true` or `false`, default: `true`)

**Example:**

```bash
# Generate a graph for the last year with custom colors
curl "http://localhost:8000/graph.svg?period=1y&color=%23ff6b6b" > graph.svg

# Minimal graph without labels
curl "http://localhost:8000/graph.svg?showMonthLabels=false&showDayLabels=false" > graph.svg
```

**Features:**

- Visualizes daily command counts as a color-coded heat map
- Uses logarithmic scaling with percentile-based thresholds for better distribution
- Includes month separator lines that weave through the grid
- Shows color legend and statistics in the footer
- Perfect for embedding in GitHub READMEs or documentation

## Configuration

Set these environment variables in `.env` or pass them to Docker:

- `DATABASE_URL` (required): PostgreSQL connection string for your Atuin database
  - Example: `postgresql://atuin:password@localhost:5432/atuin`
- `PORT` (optional): Port to listen on (default: 8000)

## Development

```bash
deno task dev          # Start with auto-reload
deno task test         # Run tests
deno task format       # Format code
deno task lint         # Lint code
```

## Deployment

The application is automatically built and published as a Docker image to GitHub Container Registry
on every push to main. Use it in production with:

```bash
docker run -d \
  --name atuin-abacus \
  --restart unless-stopped \
  -p 8000:8000 \
  -e DATABASE_URL="postgresql://user:pass@host:5432/atuin" \
  ghcr.io/evanpurkhiser/atuin-abacus:latest
```

## Tech Stack

- **[Deno](https://deno.com/)**: Modern JavaScript/TypeScript runtime
- **[Hono](https://hono.dev/)**: Fast web framework
- **[postgres](https://deno.land/x/postgres)**: PostgreSQL client for Deno
- **Docker**: Containerized deployment
