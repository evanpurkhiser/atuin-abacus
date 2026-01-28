import {createApp} from './app.ts';
import {getConfig} from './config.ts';
import {
  closePool,
  getCommandsPerDay,
  getTimeOfDayStats,
  getTotalCommands,
  testConnection,
} from './db.ts';

const config = getConfig();
const app = createApp(
  {testConnection, getCommandsPerDay, getTimeOfDayStats, getTotalCommands},
  config.cacheTtlSeconds
);

// Start the server
console.log(`Starting Atuin Metrics API on port ${config.port}...`);
console.log(`Database: ${config.databaseUrl.replace(/:[^:@]+@/, ':***@')}`);
console.log(`Cache TTL: ${config.cacheTtlSeconds}s`);

// Test database connection on startup
const dbHealthy = await testConnection();
if (!dbHealthy) {
  console.error('Failed to connect to database!');
  Deno.exit(1);
}
console.log('Database connection successful!');

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
