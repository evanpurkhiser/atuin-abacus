import {assert, assertEquals, assertExists} from '@std/assert';

import type {DbFunctions} from './app.ts';
import {createApp} from './app.ts';

// Mock database functions
const mockDb: DbFunctions = {
  testConnection: () => Promise.resolve(true),
  getCommandsPerDay: () =>
    Promise.resolve([
      {date: '2024-01-01', count: 42},
      {date: '2024-01-02', count: 35},
    ]),
  getTimeOfDayStats: () =>
    Promise.resolve({
      hourly: Array(24).fill(5),
    }),
};

const app = createApp(mockDb, 300);

Deno.test('Health endpoint returns healthy status', async () => {
  const req = new Request('http://localhost/health');
  const res = await app.fetch(req);

  assertEquals(res.status, 200);

  const data = await res.json();
  assertEquals(data.status, 'healthy');
  assertEquals(data.database, 'connected');
});

Deno.test('Commands per day endpoint returns valid data', async () => {
  const req = new Request('http://localhost/commands-per-day');
  const res = await app.fetch(req);

  assertEquals(res.status, 200);
  assert(res.headers.get('content-type')?.includes('application/json'));

  const data = await res.json();
  assert(Array.isArray(data));
  assertEquals(data.length, 2);

  const first = data[0];
  assertExists(first.date);
  assertExists(first.count);
  assert(typeof first.date === 'string');
  assert(typeof first.count === 'number');
  assert(/^\d{4}-\d{2}-\d{2}$/.test(first.date));
});

Deno.test('Commands per day endpoint with date range', async () => {
  const start = '2024-01-01';
  const end = '2024-12-31';
  const req = new Request(
    `http://localhost/commands-per-day?start=${start}&end=${end}`
  );
  const res = await app.fetch(req);

  assertEquals(res.status, 200);

  const data = await res.json();
  assert(Array.isArray(data));
});

Deno.test('Commands per day endpoint with invalid date format', async () => {
  const req = new Request('http://localhost/commands-per-day?start=invalid-date');
  const res = await app.fetch(req);

  assertEquals(res.status, 400);

  const data = await res.json();
  assertExists(data.error);
  assert(data.error.includes('Invalid start date format'));
});

Deno.test('Commands per day endpoint with end before start', async () => {
  const req = new Request(
    'http://localhost/commands-per-day?start=2024-12-31&end=2024-01-01'
  );
  const res = await app.fetch(req);

  assertEquals(res.status, 400);

  const data = await res.json();
  assertExists(data.error);
  assert(data.error.includes('Start date must be before or equal to end date'));
});

Deno.test('Time of day endpoint returns 24 hour array', async () => {
  const req = new Request('http://localhost/time-of-day');
  const res = await app.fetch(req);

  assertEquals(res.status, 200);

  const data = await res.json();
  assertExists(data.hourly);
  assert(Array.isArray(data.hourly));
  assertEquals(data.hourly.length, 24);

  for (const count of data.hourly) {
    assert(typeof count === 'number');
    assert(count >= 0);
  }
});

Deno.test('Time of day endpoint with date range', async () => {
  const start = '2024-01-01';
  const end = '2024-12-31';
  const req = new Request(`http://localhost/time-of-day?start=${start}&end=${end}`);
  const res = await app.fetch(req);

  assertEquals(res.status, 200);

  const data = await res.json();
  assertEquals(data.hourly.length, 24);
});

Deno.test('Cache-Control headers are present', async () => {
  const req = new Request(
    'http://localhost/commands-per-day?start=2024-01-01&end=2024-01-31'
  );
  const res = await app.fetch(req);
  await res.json(); // Consume the body

  // Check that Cache-Control header is set
  const cacheControl = res.headers.get('cache-control');
  assertExists(cacheControl);
  assert(cacheControl.includes('max-age'));
});

Deno.test('CORS headers are present', async () => {
  const req = new Request('http://localhost/health');
  const res = await app.fetch(req);
  await res.json(); // Consume the body

  assertEquals(res.headers.get('access-control-allow-origin'), '*');
});

Deno.test('404 for unknown endpoints', async () => {
  const req = new Request('http://localhost/unknown');
  const res = await app.fetch(req);

  assertEquals(res.status, 404);

  const data = await res.json();
  assertExists(data.error);
});

Deno.test('404 for non-GET requests to API endpoints', async () => {
  const req = new Request('http://localhost/commands-per-day', {
    method: 'POST',
  });
  const res = await app.fetch(req);

  // Hono returns 404 for unhandled method/route combinations
  assertEquals(res.status, 404);

  const data = await res.json();
  assertExists(data.error);
});

Deno.test('OPTIONS request succeeds for CORS preflight', async () => {
  const req = new Request('http://localhost/commands-per-day', {
    method: 'OPTIONS',
  });
  const res = await app.fetch(req);

  assertEquals(res.status, 204);
});

Deno.test('Commands per day with valid timezone in Prefer header', async () => {
  const req = new Request('http://localhost/commands-per-day', {
    headers: {
      prefer: 'timezone=America/Los_Angeles',
    },
  });
  const res = await app.fetch(req);

  assertEquals(res.status, 200);
  const data = await res.json();
  assert(Array.isArray(data));
});

Deno.test('Invalid timezone in Prefer header returns 400', async () => {
  const req = new Request('http://localhost/commands-per-day', {
    headers: {
      prefer: 'timezone=Invalid/Timezone',
    },
  });
  const res = await app.fetch(req);

  assertEquals(res.status, 400);
  const data = await res.json();
  assertExists(data.error);
  assert(data.error.includes('Invalid timezone'));
});

Deno.test('Time of day with timezone in Prefer header', async () => {
  const req = new Request('http://localhost/time-of-day', {
    headers: {
      prefer: 'timezone=Europe/London',
    },
  });
  const res = await app.fetch(req);

  assertEquals(res.status, 200);
  const data = await res.json();
  assertEquals(data.hourly.length, 24);
});
