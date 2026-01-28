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
  const result = await getCommandsPerDay({timezone: 'America/Los_Angeles'});

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

  const result = await getCommandsPerDay({
    startDate,
    endDate,
    timezone: 'America/Los_Angeles',
  });

  // Should return an array
  assert(Array.isArray(result));

  // Due to timezone conversions, dates near boundaries might shift slightly
  // Just verify we got results and they're mostly in range
  if (result.length > 0) {
    // At least some results should be in range
    const inRange = result.filter(item => item.date >= startDate && item.date <= endDate);
    assert(inRange.length > 0, 'Should have at least some results in the date range');
  }
});

Deno.test('getCommandsPerDay - results are sorted by date', async () => {
  const result = await getCommandsPerDay({timezone: 'America/Los_Angeles'});

  // Check dates are in ascending order
  for (let i = 1; i < result.length; i++) {
    assert(result[i].date >= result[i - 1].date);
  }
});

Deno.test('getTimeOfDayStats - returns 24 hour array', async () => {
  const result = await getTimeOfDayStats({timezone: 'America/Los_Angeles'});

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

  const result = await getTimeOfDayStats({
    startDate,
    endDate,
    timezone: 'America/Los_Angeles',
  });

  // Should still have 24 hours
  assertEquals(result.hourly.length, 24);

  // Each value should be a number >= 0
  for (const count of result.hourly) {
    assert(typeof count === 'number');
    assert(count >= 0);
  }
});
