import {createApp} from './app.ts';
import {getConfig} from './config.ts';
import {closePool, getCommandsPerDay, getStats, getTimeOfDayStats} from './db.ts';

const config = getConfig();
const app = createApp(
  {getCommandsPerDay, getTimeOfDayStats, getStats},
  config.cacheTtlSeconds
);

// Start the server
console.log(`Starting Atuin Metrics API on port ${config.port}...`);
console.log(`Database: ${config.databaseUrl.replace(/:[^:@]+@/, ':***@')}`);
console.log(`Cache TTL: ${config.cacheTtlSeconds}s`);

// Graceful shutdown handler
const shutdown = async (signal: string) => {
  console.log(`\nReceived ${signal}, shutting down gracefully...`);
  try {
    await closePool();
    console.log('Database connections closed');
    Deno.exit(0);
  } catch (error) {
    console.error('Error during shutdown:', error);
    Deno.exit(1);
  }
};

// Register signal handlers for graceful shutdown
Deno.addSignalListener('SIGINT', () => shutdown('SIGINT'));
Deno.addSignalListener('SIGTERM', () => shutdown('SIGTERM'));

Deno.serve({port: config.port}, app.fetch);
