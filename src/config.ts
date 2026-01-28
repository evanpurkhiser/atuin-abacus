import {load} from '@std/dotenv';

// Load .env file (silently ignored if it doesn't exist)
await load({export: true});

export interface Config {
  port: number;
  databaseUrl: string;
  cacheTtlSeconds: number;
}

export function getConfig(): Config {
  const databaseUrl =
    Deno.env.get('DATABASE_URL') || 'postgresql://atuin:atuin@localhost:5432/atuin';

  return {
    port: parseInt(Deno.env.get('PORT') || '8000', 10),
    databaseUrl,
    cacheTtlSeconds: parseInt(Deno.env.get('CACHE_TTL_SECONDS') || '300', 10),
  };
}
