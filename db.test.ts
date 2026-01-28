import {assert, assertEquals, assertGreater} from '@std/assert';

import {getCommandsPerDay, getTimeOfDayStats, testConnection} from './db.ts';

Deno.test({
  name: 'Database connection',
  sanitizeResources: false,
  fn: async () => {
    const connected = await testConnection();
    assertEquals(connected, true);
  },
});

Deno.test('getCommandsPerDay - returns data in correct format', async () => {
  const result = await getCommandsPerDay();

  // Should return an array
  assert(Array.isArray(result));

  if (result.length > 0) {
    // Check first item has correct shape
    const first = result[0];
    assert('date' in first);
    assert('count' in first);
    assert(typeof first.date === 'string');
    assert(typeof first.count === 'number');

    // Date should be in YYYY-MM-DD format
    assert(/^\d{4}-\d{2}-\d{2}$/.test(first.date));

    // Count should be positive
    assertGreater(first.count, 0);
  }
});

Deno.test('getCommandsPerDay - with date range', async () => {
  const startDate = '2024-01-01';
  const endDate = '2024-12-31';

  const result = await getCommandsPerDay(startDate, endDate);

  // Should return an array
  assert(Array.isArray(result));

  // All dates should be within range if there are results
  for (const item of result) {
    assert(item.date >= startDate);
    assert(item.date <= endDate);
  }
});

Deno.test('getCommandsPerDay - results are sorted by date', async () => {
  const result = await getCommandsPerDay();

  // Check dates are in ascending order
  for (let i = 1; i < result.length; i++) {
    assert(result[i].date >= result[i - 1].date);
  }
});

Deno.test('getTimeOfDayStats - returns 24 hour array', async () => {
  const result = await getTimeOfDayStats();

  // Should have hourly array
  assert('hourly' in result);
  assert(Array.isArray(result.hourly));

  // Should have exactly 24 hours
  assertEquals(result.hourly.length, 24);

  // Each hour should be a number >= 0
  for (const count of result.hourly) {
    assert(typeof count === 'number');
    assert(count >= 0);
  }
});

Deno.test('getTimeOfDayStats - with date range', async () => {
  const startDate = '2024-01-01';
  const endDate = '2024-12-31';

  const result = await getTimeOfDayStats(startDate, endDate);

  // Should still have 24 hours
  assertEquals(result.hourly.length, 24);

  // Each value should be a number >= 0
  for (const count of result.hourly) {
    assert(typeof count === 'number');
    assert(count >= 0);
  }
});
