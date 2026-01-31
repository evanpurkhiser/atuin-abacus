import {type Context, Hono} from 'hono';
import {cache} from 'hono/cache';
import {cors} from 'hono/cors';
import {HTTPException} from 'hono/http-exception';

import type {DailyCommandCount, Period, Stats, TimeOfDayStats} from './db.ts';
import {generateContributionGraph} from './svg.ts';
import {parsePeriod} from './utils.ts';

/**
 * Get the system's default timezone using Temporal API
 */
function getSystemTimezone(): string {
  return Temporal.Now.timeZoneId();
}

/**
 * Validate a timezone string using Intl API
 */
function isValidTimezone(tz: string): boolean {
  try {
    Intl.DateTimeFormat(undefined, {timeZone: tz});
    return true;
  } catch {
    return false;
  }
}

/**
 * Parse timezone from Prefer header (format: "timezone=America/Los_Angeles")
 * Supports both quoted and unquoted values
 */
function parseTimezoneFromPrefer(preferHeader: string): string | null {
  // Match timezone=value or timezone="value"
  const match = preferHeader.match(/timezone=(?:"([^"]+)"|([^\s,;]+))/i);
  return match ? match[1] || match[2] : null;
}

// Validate date format (YYYY-MM-DD)
function isValidDate(dateStr: string): boolean {
  const regex = /^\d{4}-\d{2}-\d{2}$/;
  if (!regex.test(dateStr)) {
    return false;
  }
  try {
    Temporal.PlainDate.from(dateStr);
    return true;
  } catch {
    return false;
  }
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

export interface DbFunctions {
  getCommandsPerDay: (opts: Period) => Promise<DailyCommandCount[]>;
  getTimeOfDayStats: (opts: Period) => Promise<TimeOfDayStats>;
  getStats: (opts: Period) => Promise<Stats>;
}

interface Variables {
  timezone: string;
}

export function createApp(db: DbFunctions, cacheTtlSeconds = 300) {
  const app = new Hono<{Variables: Variables}>();

  const cacheMiddleware = cache({
    cacheName: 'atuin-abacus',
    cacheControl: `max-age=${cacheTtlSeconds}`,
    wait: true,
  });

  /**
   * Helper to extract Period parameters from context
   * Throws HTTPException if validation fails
   * Supports either start/end dates OR a period parameter (e.g., "1y", "6m", "30d")
   * If period is provided, it takes precedence over start/end
   */
  const getPeriodFromContext = (c: Context<{Variables: Variables}>): Period => {
    const timezone = c.get('timezone');
    const periodParam = c.req.query('period');

    // If period parameter is provided, parse it
    if (periodParam) {
      const parsed = parsePeriod(periodParam, timezone);
      if (!parsed) {
        throw new HTTPException(400, {
          message: `Invalid period format. Expected format: <number><unit> (e.g., "1y", "6m", "30d"), got: ${periodParam}`,
        });
      }
      return {startDate: parsed.startDate, endDate: parsed.endDate, timezone};
    }

    // Otherwise use start/end date parameters
    const startDate = c.req.query('start') || undefined;
    const endDate = c.req.query('end') || undefined;

    const validationError = validateDateParams(startDate, endDate);
    if (validationError) {
      throw new HTTPException(400, {message: validationError});
    }

    return {startDate, endDate, timezone};
  };

  // CORS middleware
  app.use('*', cors());

  // Timezone middleware - parse Prefer header and set timezone in context
  // deno-lint-ignore require-await
  app.use('*', async (c, next) => {
    const preferHeader = c.req.header('prefer');

    // If no Prefer header, use system timezone
    if (!preferHeader) {
      c.set('timezone', getSystemTimezone());
      return next();
    }

    const requestedTimezone = parseTimezoneFromPrefer(preferHeader);

    // If no timezone in Prefer header, use system timezone
    if (!requestedTimezone) {
      c.set('timezone', getSystemTimezone());
      return next();
    }

    // Validate the requested timezone
    if (!isValidTimezone(requestedTimezone)) {
      return c.json({error: `Invalid timezone: ${requestedTimezone}`}, 400);
    }

    c.set('timezone', requestedTimezone);
    return next();
  });

  // History
  app.get('/history', cacheMiddleware, async c => {
    const period = getPeriodFromContext(c);
    const data = await db.getCommandsPerDay(period);
    return c.json(data);
  });

  // Time of day stats
  app.get('/time-of-day', cacheMiddleware, async c => {
    const period = getPeriodFromContext(c);
    const data = await db.getTimeOfDayStats(period);
    return c.json(data);
  });

  // SVG contribution graph
  app.get('/graph.svg', cacheMiddleware, async c => {
    const period = getPeriodFromContext(c);
    const data = await db.getCommandsPerDay(period);

    // Parse color parameters
    const baseColor = c.req.query('color') || c.req.query('baseColor');
    const textColor = c.req.query('textColor');
    const cellBackground = c.req.query('cellBackground');

    // Parse numeric parameters
    const cellSize = c.req.query('cellSize')
      ? parseInt(c.req.query('cellSize')!, 10)
      : undefined;
    const cellGap = c.req.query('cellGap')
      ? parseInt(c.req.query('cellGap')!, 10)
      : undefined;

    // Parse boolean parameters (accept 'true', '1', 'false', '0')
    const parseBoolean = (value: string | undefined): boolean | undefined => {
      if (value === undefined) {
        return undefined;
      }
      return value === 'true' || value === '1';
    };

    const showMonthLabels = parseBoolean(c.req.query('showMonthLabels'));
    const showDayLabels = parseBoolean(c.req.query('showDayLabels'));

    const svg = generateContributionGraph(data, {
      baseColor,
      textColor,
      cellBackground,
      cellSize,
      cellGap,
      showMonthLabels,
      showDayLabels,
    });

    c.header('Content-Type', 'image/svg+xml');
    return c.body(svg);
  });

  // Stats at root
  app.get('/', cacheMiddleware, async c => {
    const period = getPeriodFromContext(c);
    const data = await db.getStats(period);
    return c.json(data);
  });

  // 404 handler
  app.notFound(c => c.json({error: 'Not found'}, 404));

  // Error handler
  app.onError((error, c) => {
    if (error instanceof HTTPException) {
      return c.json({error: error.message}, error.status);
    }
    console.error('Error handling request:', error);
    return c.json({error: 'Internal server error', message: error.message}, 500);
  });

  return app;
}
