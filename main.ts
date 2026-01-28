import {Hono} from 'hono';
import {cache} from 'hono/cache';
import {cors} from 'hono/cors';

import {getConfig} from './config.ts';
import {closePool, getCommandsPerDay, getTimeOfDayStats, testConnection} from './db.ts';

const config = getConfig();
const app = new Hono();

// Validate date format (YYYY-MM-DD)
function isValidDate(dateStr: string): boolean {
  const regex = /^\d{4}-\d{2}-\d{2}$/;
  if (!regex.test(dateStr)) {
    return false;
  }
  const date = new Date(dateStr);
  return !isNaN(date.getTime());
}

// Validate query date parameters
function validateDateParams(start?: string, end?: string): string | null {
  if (start && !isValidDate(start)) {
    return `Invalid start date format. Expected YYYY-MM-DD, got: ${start}`;
  }
  if (end && !isValidDate(end)) {
    return `Invalid end date format. Expected YYYY-MM-DD, got: ${end}`;
  }
  if (start && end && start > end) {
    return 'Start date must be before or equal to end date';
  }
  return null;
}

// CORS middleware
app.use('*', cors());

// Health check (no caching)
app.get('/health', async c => {
  const isHealthy = await testConnection();
  return c.json(
    {
      status: isHealthy ? 'healthy' : 'unhealthy',
      database: isHealthy ? 'connected' : 'disconnected',
    },
    isHealthy ? 200 : 503
  );
});

// Commands per day (with caching)
app.get(
  '/api/commands-per-day',
  cache({
    cacheName: 'atuin-abacus',
    cacheControl: `max-age=${config.cacheTtlSeconds}`,
    wait: true,
  }),
  async c => {
    const startDate = c.req.query('start') || undefined;
    const endDate = c.req.query('end') || undefined;

    // Validate date parameters
    const validationError = validateDateParams(startDate, endDate);
    if (validationError) {
      return c.json({error: validationError}, 400);
    }

    const data = await getCommandsPerDay(startDate, endDate);
    return c.json(data);
  }
);

// Time of day stats (with caching)
app.get(
  '/api/time-of-day',
  cache({
    cacheName: 'atuin-abacus',
    cacheControl: `max-age=${config.cacheTtlSeconds}`,
    wait: true,
  }),
  async c => {
    const startDate = c.req.query('start') || undefined;
    const endDate = c.req.query('end') || undefined;

    // Validate date parameters
    const validationError = validateDateParams(startDate, endDate);
    if (validationError) {
      return c.json({error: validationError}, 400);
    }

    const data = await getTimeOfDayStats(startDate, endDate);
    return c.json(data);
  }
);

// 404 handler
app.notFound(c => c.json({error: 'Not found'}, 404));

// Error handler
app.onError((error, c) => {
  console.error('Error handling request:', error);
  return c.json({error: 'Internal server error', message: error.message}, 500);
});

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
