import {assert, assertEquals, assertExists} from '@std/assert';

import type {DbFunctions} from './app.ts';
import {createApp} from './app.ts';

// Mock database functions
const mockDb: DbFunctions = {
  getCommandsPerDay: () =>
    Promise.resolve([
      {date: '2024-01-01', count: 42},
      {date: '2024-01-02', count: 35},
    ]),
  getTimeOfDayStats: () =>
    Promise.resolve({
      hourly: Array(24).fill(5),
    }),
  getStats: () =>
    Promise.resolve({
      total: 296378,
      lastCommandAt: '2026-01-30T08:00:00.000Z',
    }),
};

const app = createApp(mockDb, 300);

Deno.test('History endpoint returns valid data', async () => {
  const req = new Request('http://localhost/history');
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

Deno.test('History endpoint with date range', async () => {
  const start = '2024-01-01';
  const end = '2024-12-31';
  const req = new Request(`http://localhost/history?start=${start}&end=${end}`);
  const res = await app.fetch(req);

  assertEquals(res.status, 200);

  const data = await res.json();
  assert(Array.isArray(data));
});

Deno.test('History endpoint with invalid date format', async () => {
  const req = new Request('http://localhost/history?start=invalid-date');
  const res = await app.fetch(req);

  assertEquals(res.status, 400);

  const data = await res.json();
  assertExists(data.error);
  assert(data.error.includes('Invalid start date format'));
});

Deno.test('History endpoint with end before start', async () => {
  const req = new Request('http://localhost/history?start=2024-12-31&end=2024-01-01');
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
  const req = new Request('http://localhost/history?start=2024-01-01&end=2024-01-31');
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
  const req = new Request('http://localhost/history', {
    method: 'POST',
  });
  const res = await app.fetch(req);

  // Hono returns 404 for unhandled method/route combinations
  assertEquals(res.status, 404);

  const data = await res.json();
  assertExists(data.error);
});

Deno.test('OPTIONS request succeeds for CORS preflight', async () => {
  const req = new Request('http://localhost/history', {
    method: 'OPTIONS',
  });
  const res = await app.fetch(req);

  assertEquals(res.status, 204);
});

Deno.test('History with valid timezone in Prefer header', async () => {
  const req = new Request('http://localhost/history', {
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
  const req = new Request('http://localhost/history', {
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

Deno.test('Root endpoint returns stats', async () => {
  const req = new Request('http://localhost/?nocache=test');
  const res = await app.fetch(req);

  assertEquals(res.status, 200);
  assert(res.headers.get('content-type')?.includes('application/json'));

  const data = await res.json();
  assertExists(data.total);
  assert(typeof data.total === 'number');
  assert(data.total >= 0);

  // Check lastCommandAt field - should exist from mock
  assertExists(data.lastCommandAt, 'lastCommandAt should be present in response');
  // Should be valid ISO 8601 format
  assert(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/.test(data.lastCommandAt));
});

Deno.test('Root endpoint with date range', async () => {
  const start = '2024-01-01';
  const end = '2024-12-31';
  const req = new Request(`http://localhost/?start=${start}&end=${end}`);
  const res = await app.fetch(req);

  assertEquals(res.status, 200);

  const data = await res.json();
  assertExists(data.total);
  assert(typeof data.total === 'number');
});

Deno.test('Root endpoint with invalid date format', async () => {
  const req = new Request('http://localhost/?start=invalid-date');
  const res = await app.fetch(req);

  assertEquals(res.status, 400);

  const data = await res.json();
  assertExists(data.error);
  assert(data.error.includes('Invalid start date format'));
});
