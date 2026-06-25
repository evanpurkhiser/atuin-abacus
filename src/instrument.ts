import * as Sentry from '@sentry/deno';
import {load} from '@std/dotenv';

// Load .env before init so SENTRY_DSN is available. This module is imported
// first in main.ts, before config.ts performs its own load.
if (!Deno.env.get('SKIP_ENV_LOAD')) {
  await load({export: true});
}

// With no DSN configured (tests, local runs without a .env) the SDK becomes a
// no-op, so there's nothing to guard here.
Sentry.init({
  dsn: Deno.env.get('SENTRY_DSN'),
  environment: Deno.env.get('SENTRY_ENVIRONMENT') ?? 'production',
  release: Deno.env.get('SENTRY_RELEASE'),
  sendDefaultPii: true,
});
