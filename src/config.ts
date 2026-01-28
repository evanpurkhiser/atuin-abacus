import {load} from '@std/dotenv';

// Load .env file (silently ignored if it doesn't exist)
await load({export: true});

export interface Config {
  port: number;
  databaseUrl: string;
  cacheTtlSeconds: number;
}

export function getConfig(): Config {
  const dbHost = Deno.env.get('DB_HOST') || 'localhost';
  const dbPort = Deno.env.get('DB_PORT') || '5432';
  const dbUser = Deno.env.get('DB_USER') || 'atuin';
  const dbPassword = Deno.env.get('DB_PASSWORD') || 'atuin';
  const dbName = Deno.env.get('DB_NAME') || 'atuin';

  return {
    port: parseInt(Deno.env.get('PORT') || '8000', 10),
    databaseUrl: `postgresql://${dbUser}:${dbPassword}@${dbHost}:${dbPort}/${dbName}`,
    cacheTtlSeconds: parseInt(Deno.env.get('CACHE_TTL_SECONDS') || '300', 10),
  };
}
