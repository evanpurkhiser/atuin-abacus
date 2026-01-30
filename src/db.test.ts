import {assert, assertEquals} from '@std/assert';
import {load} from '@std/dotenv';

import {setupTestDatabase} from '../test/setup.ts';

// IMPORTANT: Load test env file BEFORE importing any modules that read env vars
Deno.env.set('SKIP_ENV_LOAD', '1');
await load({envPath: '.env.test', export: true});

// Now import db functions after setting the env var
import {closePool, getCommandsPerDay, getStats, getTimeOfDayStats} from './db.ts';

// Setup test database before all tests
await setupTestDatabase();

Deno.test({
  name: 'getCommandsPerDay - returns correct data from fixtures',
  sanitizeResources: false,
  fn: async () => {
    const result = await getCommandsPerDay({timezone: 'UTC'});

    // Should return an array
    assert(Array.isArray(result));

    // Should have data from both history table (2024-01-01, 2024-01-02)
    // and store table (2026-01-01, 2026-01-02)
    assert(result.length >= 4, `Expected at least 4 days, got ${result.length}`);

    // Check first item has correct shape
    const first = result[0];
    assert('date' in first);
    assert('count' in first);
    assert(typeof first.date === 'string');
    assert(typeof first.count === 'number');

    // Date should be in YYYY-MM-DD format
    assert(/^\d{4}-\d{2}-\d{2}$/.test(first.date));

    // Find specific test dates
    const jan1_2024 = result.find(r => r.date === '2024-01-01');
    const jan2_2024 = result.find(r => r.date === '2024-01-02');
    const jan1_2026 = result.find(r => r.date === '2026-01-01');
    const jan2_2026 = result.find(r => r.date === '2026-01-02');

    // Verify fixture data
    assertEquals(jan1_2024?.count, 5, 'Jan 1 2024 should have 5 commands');
    assertEquals(jan2_2024?.count, 3, 'Jan 2 2024 should have 3 commands');
    assertEquals(jan1_2026?.count, 5, 'Jan 1 2026 should have 5 commands');
    assertEquals(jan2_2026?.count, 3, 'Jan 2 2026 should have 3 commands');
  },
});

Deno.test('getCommandsPerDay - with date range filters correctly', async () => {
  const result = await getCommandsPerDay({
    startDate: '2024-01-01',
    endDate: '2024-01-02',
    timezone: 'UTC',
  });

  // Should only have 2024 data
  assert(Array.isArray(result));
  assertEquals(result.length, 2, 'Should have exactly 2 days in 2024 range');

  // Should not include 2026 data
  const has2026Data = result.some(r => r.date.startsWith('2026'));
  assert(!has2026Data, 'Should not include 2026 data when filtering to 2024');
});

Deno.test('getCommandsPerDay - results are sorted by date', async () => {
  const result = await getCommandsPerDay({timezone: 'UTC'});

  // Check dates are in ascending order
  for (let i = 1; i < result.length; i++) {
    assert(
      result[i].date >= result[i - 1].date,
      `Dates not sorted: ${result[i - 1].date} > ${result[i].date}`
    );
  }
});

Deno.test('getCommandsPerDay - excludes deleted records', async () => {
  const result = await getCommandsPerDay({timezone: 'UTC'});

  // Should not include 2024-01-03 (deleted entry)
  const jan3 = result.find(r => r.date === '2024-01-03');
  assert(!jan3, 'Should not include deleted records');
});

Deno.test('getTimeOfDayStats - returns 24 hour array', async () => {
  const result = await getTimeOfDayStats({timezone: 'UTC'});

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

Deno.test('getTimeOfDayStats - calculates averages correctly', async () => {
  const result = await getTimeOfDayStats({timezone: 'UTC'});

  // Based on fixtures, we have commands at:
  // 09:00 (4 days), 10:00 (4 days), 11:00 (4 days), 14:00 (2 days), 15:00 (2 days)
  // Spread across 4 unique days

  // Hour 9 should have average of 4 commands / 4 days = 1.0
  assertEquals(result.hourly[9], 1.0, 'Hour 9 should average 1.0 commands/day');

  // Hour 10 should have average of 4 commands / 4 days = 1.0
  assertEquals(result.hourly[10], 1.0, 'Hour 10 should average 1.0 commands/day');

  // Hour 11 should have average of 4 commands / 4 days = 1.0
  assertEquals(result.hourly[11], 1.0, 'Hour 11 should average 1.0 commands/day');

  // Hour 14 should have average of 2 commands / 4 days = 0.5
  assertEquals(result.hourly[14], 0.5, 'Hour 14 should average 0.5 commands/day');

  // Hour 15 should have average of 2 commands / 4 days = 0.5
  assertEquals(result.hourly[15], 0.5, 'Hour 15 should average 0.5 commands/day');

  // Hours with no commands should be 0
  assertEquals(result.hourly[0], 0, 'Hour 0 should be 0');
  assertEquals(result.hourly[23], 0, 'Hour 23 should be 0');
});

Deno.test('getTimeOfDayStats - with date range', async () => {
  const result = await getTimeOfDayStats({
    startDate: '2024-01-01',
    endDate: '2024-01-02',
    timezone: 'UTC',
  });

  // Should still have 24 hours
  assertEquals(result.hourly.length, 24);

  // With only 2024 data (2 days):
  // Hour 9: 2 commands / 2 days = 1.0
  assertEquals(result.hourly[9], 1.0, 'Hour 9 in 2024 range should average 1.0');
});

Deno.test('getStats - returns correct total and lastCommandAt', async () => {
  const result = await getStats({timezone: 'UTC'});

  // Should have total field
  assert('total' in result);
  assert(typeof result.total === 'number');

  // Total should be 16: 8 from history + 8 from store
  assertEquals(result.total, 16, 'Total should be 16 commands');

  // Should have lastCommandAt field
  assert('lastCommandAt' in result);
  assert(result.lastCommandAt);
  assert(typeof result.lastCommandAt === 'string');

  // Should be valid ISO 8601 format
  assert(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/.test(result.lastCommandAt));

  // Based on fixtures, last command should be from 2026-01-02
  assert(result.lastCommandAt.startsWith('2026-01-02'));
});

Deno.test('getStats - with date range', async () => {
  const result = await getStats({
    startDate: '2024-01-01',
    endDate: '2024-01-02',
    timezone: 'UTC',
  });

  // Should only count 2024 data: 5 + 3 = 8
  assertEquals(result.total, 8, 'Total for 2024-01-01 to 2024-01-02 should be 8');

  // lastCommandAt should be from the 2024 range
  assert(result.lastCommandAt);
  assert(
    result.lastCommandAt.startsWith('2024-01-02'),
    'Last command should be from 2024-01-02'
  );
});

Deno.test('getStats - lastCommandAt respects timezone', async () => {
  // Get result in UTC
  const resultUTC = await getStats({timezone: 'UTC'});

  // Get result in Asia/Taipei (UTC+8)
  const resultTaipei = await getStats({timezone: 'Asia/Taipei'});

  // Both should have lastCommandAt
  assert(resultUTC.lastCommandAt);
  assert(resultTaipei.lastCommandAt);

  // Parse the timestamps
  const utcTime = new Date(resultUTC.lastCommandAt);
  const taipeiTime = new Date(resultTaipei.lastCommandAt);

  // The UTC timestamp should be 8 hours earlier than Taipei
  // (because Taipei is stored as if it were UTC, it will appear 8 hours ahead)
  const hourDiff = Math.abs(taipeiTime.getTime() - utcTime.getTime()) / (1000 * 60 * 60);
  assertEquals(hourDiff, 8, 'Taipei time should be 8 hours ahead of UTC');
});

Deno.test({
  name: 'getTimeOfDayStats - timezone affects hour grouping',
  sanitizeResources: false,
  fn: async () => {
    // Test with America/Los_Angeles timezone (UTC-8)
    // Our fixture data has commands at 09:00, 10:00, 11:00, 14:00, 15:00 UTC
    // In LA time, these would be at 01:00, 02:00, 03:00, 06:00, 07:00
    const resultLA = await getTimeOfDayStats({
      timezone: 'America/Los_Angeles',
    });

    assertEquals(resultLA.hourly.length, 24);

    // Hours 1, 2, 3 should have commands (from UTC 9, 10, 11)
    assert(resultLA.hourly[1] > 0, 'Hour 1 in LA should have commands (UTC 09:00)');
    assert(resultLA.hourly[2] > 0, 'Hour 2 in LA should have commands (UTC 10:00)');
    assert(resultLA.hourly[3] > 0, 'Hour 3 in LA should have commands (UTC 11:00)');

    // Hours 6, 7 should have commands (from UTC 14, 15)
    assert(resultLA.hourly[6] > 0, 'Hour 6 in LA should have commands (UTC 14:00)');
    assert(resultLA.hourly[7] > 0, 'Hour 7 in LA should have commands (UTC 15:00)');

    // Hour 9 in LA should be 0 (since UTC 09:00 converts to LA 01:00)
    assertEquals(
      resultLA.hourly[9],
      0,
      'Hour 9 in LA should be 0 (original UTC 09:00 is now hour 1)'
    );
  },
});

// Cleanup: Close database pool to prevent resource leaks
Deno.test({
  name: 'Cleanup database connections',
  sanitizeResources: false,
  fn: async () => {
    await closePool();
  },
});
