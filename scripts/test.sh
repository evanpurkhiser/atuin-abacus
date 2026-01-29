#!/bin/bash
# Test script that ensures DATABASE_URL is set for tests

# Use DATABASE_URL from environment if set (for CI), otherwise use local test database
if [ -z "$DATABASE_URL" ]; then
  export DATABASE_URL="postgresql://atuin_test:atuin_test@127.0.0.1:5433/atuin_test"
fi

export SKIP_ENV_LOAD=1
export TZ=UTC

deno test --unstable-temporal --allow-net --allow-env --allow-read "$@"
