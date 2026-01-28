import {assert, assertEquals, assertExists} from '@std/assert';

const BASE_URL = 'http://localhost:8000';

Deno.test('Health endpoint returns healthy status', async () => {
  const response = await fetch(`${BASE_URL}/health`);
  assertEquals(response.status, 200);

  const data = await response.json();
  assertEquals(data.status, 'healthy');
  assertEquals(data.database, 'connected');
});

Deno.test('Commands per day endpoint returns valid data', async () => {
  const response = await fetch(`${BASE_URL}/api/commands-per-day`);
  assertEquals(response.status, 200);
  assertEquals(response.headers.get('content-type'), 'application/json');

  const data = await response.json();
  assert(Array.isArray(data));

  if (data.length > 0) {
    const first = data[0];
    assertExists(first.date);
    assertExists(first.count);
    assert(typeof first.date === 'string');
    assert(typeof first.count === 'number');
    assert(/^\d{4}-\d{2}-\d{2}$/.test(first.date));
  }
});

Deno.test('Commands per day endpoint with date range', async () => {
  const start = '2024-01-01';
  const end = '2024-12-31';
  const response = await fetch(
    `${BASE_URL}/api/commands-per-day?start=${start}&end=${end}`
  );

  assertEquals(response.status, 200);

  const data = await response.json();
  assert(Array.isArray(data));

  // All dates should be within range
  for (const item of data) {
    assert(item.date >= start);
    assert(item.date <= end);
  }
});

Deno.test('Time of day endpoint returns 24 hour array', async () => {
  const response = await fetch(`${BASE_URL}/api/time-of-day`);
  assertEquals(response.status, 200);

  const data = await response.json();
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
  const response = await fetch(`${BASE_URL}/api/time-of-day?start=${start}&end=${end}`);

  assertEquals(response.status, 200);

  const data = await response.json();
  assertEquals(data.hourly.length, 24);
});

Deno.test('Cache-Control headers are present', async () => {
  const url = `${BASE_URL}/api/commands-per-day?start=2024-01-01&end=2024-01-31`;

  const response = await fetch(url);
  await response.json(); // Consume the body

  // Check that Cache-Control header is set
  const cacheControl = response.headers.get('cache-control');
  assertExists(cacheControl);
  assert(cacheControl.includes('max-age'));
});

Deno.test('CORS headers are present', async () => {
  const response = await fetch(`${BASE_URL}/health`);
  await response.json(); // Consume the body

  assertEquals(response.headers.get('access-control-allow-origin'), '*');
});

Deno.test('404 for unknown endpoints', async () => {
  const response = await fetch(`${BASE_URL}/api/unknown`);
  assertEquals(response.status, 404);

  const data = await response.json();
  assertExists(data.error);
});

Deno.test('404 for non-GET requests to API endpoints', async () => {
  const response = await fetch(`${BASE_URL}/api/commands-per-day`, {
    method: 'POST',
  });

  // Hono returns 404 for unhandled method/route combinations
  assertEquals(response.status, 404);

  const data = await response.json();
  assertExists(data.error);
});

Deno.test('OPTIONS request succeeds for CORS preflight', async () => {
  const response = await fetch(`${BASE_URL}/api/commands-per-day`, {
    method: 'OPTIONS',
  });

  assertEquals(response.status, 204);
});
