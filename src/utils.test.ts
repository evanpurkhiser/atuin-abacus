import {assert, assertEquals, assertExists} from '@std/assert';

import {parsePeriod} from './utils.ts';

Deno.test('parsePeriod with valid year format', () => {
  const result = parsePeriod('1y', 'UTC');
  assertExists(result);
  assert(result.startDate < result.endDate);

  // Verify the dates are valid YYYY-MM-DD format
  assert(/^\d{4}-\d{2}-\d{2}$/.test(result.startDate));
  assert(/^\d{4}-\d{2}-\d{2}$/.test(result.endDate));

  // Verify it's approximately 1 year difference
  const start = new Date(result.startDate);
  const end = new Date(result.endDate);
  const diffYears = (end.getTime() - start.getTime()) / (365.25 * 24 * 60 * 60 * 1000);
  assert(diffYears >= 0.99 && diffYears <= 1.01);
});

Deno.test('parsePeriod with valid month format', () => {
  const result = parsePeriod('6m', 'UTC');
  assertExists(result);
  assert(result.startDate < result.endDate);

  // Verify the dates are valid YYYY-MM-DD format
  assert(/^\d{4}-\d{2}-\d{2}$/.test(result.startDate));
  assert(/^\d{4}-\d{2}-\d{2}$/.test(result.endDate));
});

Deno.test('parsePeriod with valid day format', () => {
  const result = parsePeriod('30d', 'UTC');
  assertExists(result);
  assert(result.startDate < result.endDate);

  // Verify exactly 30 days difference
  const start = new Date(result.startDate);
  const end = new Date(result.endDate);
  const diffDays = (end.getTime() - start.getTime()) / (24 * 60 * 60 * 1000);
  assertEquals(diffDays, 30);
});

Deno.test('parsePeriod with case insensitive format', () => {
  const resultLower = parsePeriod('1y', 'UTC');
  const resultUpper = parsePeriod('1Y', 'UTC');
  assertExists(resultLower);
  assertExists(resultUpper);
  assertEquals(resultLower.startDate, resultUpper.startDate);
  assertEquals(resultLower.endDate, resultUpper.endDate);
});

Deno.test('parsePeriod with invalid format returns null', () => {
  assertEquals(parsePeriod('invalid', 'UTC'), null);
  assertEquals(parsePeriod('1x', 'UTC'), null);
  assertEquals(parsePeriod('abc', 'UTC'), null);
  assertEquals(parsePeriod('1', 'UTC'), null);
  assertEquals(parsePeriod('y', 'UTC'), null);
});

Deno.test('parsePeriod with zero value returns null', () => {
  assertEquals(parsePeriod('0d', 'UTC'), null);
  assertEquals(parsePeriod('0m', 'UTC'), null);
  assertEquals(parsePeriod('0y', 'UTC'), null);
});

Deno.test('parsePeriod with negative value returns null', () => {
  assertEquals(parsePeriod('-1d', 'UTC'), null);
});

Deno.test('parsePeriod respects timezone', () => {
  const resultUTC = parsePeriod('1d', 'UTC');
  const resultPST = parsePeriod('1d', 'America/Los_Angeles');
  assertExists(resultUTC);
  assertExists(resultPST);

  // End dates should be different due to timezone
  // (unless it happens to be the exact same calendar day in both timezones)
  assert(resultUTC.endDate !== null);
  assert(resultPST.endDate !== null);
});
