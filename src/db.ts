import {Pool} from 'postgres';

import {getConfig} from './config.ts';

const config = getConfig();
const pool = new Pool(config.databaseUrl, 3, true);

export interface DailyCommandCount {
  date: string;
  count: number;
}

export interface TimeOfDayStats {
  hourly: number[];
}

export interface Period {
  startDate?: string;
  endDate?: string;
  timezone: string;
}

export interface Stats {
  total: number;
  lastCommandAt?: string;
}

/**
 * Common CTE that combines both old history table and new store table
 * Returns both UTC timestamps and timezone-converted timestamps
 * - ts_utc: Raw UTC timestamp
 * - ts_tz: Timestamp converted to timezone $1 (for date/hour bucketing)
 */
const COMBINED_CTE = `
  WITH combined AS (
    -- Old history table
    SELECT
      timestamp as ts_utc,
      timestamp AT TIME ZONE $1 as ts_tz
    FROM history
    WHERE deleted_at IS NULL

    UNION ALL

    -- New store table (timestamps are in nanoseconds, convert to seconds)
    SELECT
      to_timestamp(timestamp / 1000000000.0) as ts_utc,
      to_timestamp(timestamp / 1000000000.0) AT TIME ZONE $1 as ts_tz
    FROM store
    WHERE tag = 'history'
  )
`;

/**
 * Get the count of commands run per day within a date range
 */
export async function getCommandsPerDay(opts: Period): Promise<DailyCommandCount[]> {
  const {startDate, endDate, timezone} = opts;
  const client = await pool.connect();
  try {
    let query = `
      ${COMBINED_CTE}
      SELECT
        date(ts_tz) as date,
        COUNT(*) as count
      FROM combined
      WHERE 1=1
    `;
    const params: string[] = [timezone];

    if (startDate) {
      params.push(startDate);
      query += ` AND date(ts_tz) >= $${params.length}`;
    }
    if (endDate) {
      params.push(endDate);
      query += ` AND date(ts_tz) <= $${params.length}`;
    }

    query += ` GROUP BY date(ts_tz) ORDER BY date`;

    const result = await client.queryObject<{date: Date; count: number}>(query, params);

    return result.rows.map(row => ({
      date: row.date.toISOString().split('T')[0],
      count: Number(row.count),
    }));
  } finally {
    client.release();
  }
}

/**
 * Get statistics including total command count and last command timestamp
 */
export async function getStats(opts: Period): Promise<Stats> {
  const {startDate, endDate, timezone} = opts;
  const client = await pool.connect();
  try {
    let query = `
      ${COMBINED_CTE}
      SELECT
        COUNT(*) as total,
        MAX(ts_utc) as last_command_at
      FROM combined
      WHERE 1=1
    `;
    const params: string[] = [timezone];

    if (startDate) {
      params.push(startDate);
      query += ` AND date(ts_tz) >= $${params.length}`;
    }
    if (endDate) {
      params.push(endDate);
      query += ` AND date(ts_tz) <= $${params.length}`;
    }

    const result = await client.queryObject<{
      total: number;
      last_command_at: Date | null;
    }>(query, params);

    // Convert UTC timestamp to timezone-aware ISO 8601 string
    let lastCommandAt: string | undefined;
    if (result.rows[0].last_command_at) {
      const instant = Temporal.Instant.from(result.rows[0].last_command_at.toISOString());
      const zdt = instant.toZonedDateTimeISO(timezone);
      lastCommandAt = zdt.toString({timeZoneName: 'never'});
    }

    return {
      total: Number(result.rows[0].total),
      lastCommandAt,
    };
  } finally {
    client.release();
  }
}

/**
 * Get average commands per hour of day (0-23)
 * Returns an array of 24 numbers representing the average number of commands
 * per day for each hour. For example, if you ran 50 commands at 9am over 10 days,
 * the value at index 9 would be 5.0
 */
export async function getTimeOfDayStats(opts: Period): Promise<TimeOfDayStats> {
  const {startDate, endDate, timezone} = opts;
  const client = await pool.connect();
  try {
    // Build WHERE clause params for date filtering
    const params: string[] = [timezone];
    let dateFilter = '';

    if (startDate) {
      params.push(startDate);
      dateFilter += ` AND date(ts_tz) >= $${params.length}`;
    }
    if (endDate) {
      params.push(endDate);
      dateFilter += ` AND date(ts_tz) <= $${params.length}`;
    }

    // Query to get average commands per hour across all days, combining both tables
    const query = `
      ${COMBINED_CTE},
      day_count AS (
        SELECT COUNT(DISTINCT date(ts_tz)) as total_days
        FROM combined
        WHERE 1=1 ${dateFilter}
      ),
      hourly_counts AS (
        SELECT
          EXTRACT(HOUR FROM ts_tz)::integer as hour,
          COUNT(*) as count
        FROM combined
        WHERE 1=1 ${dateFilter}
        GROUP BY EXTRACT(HOUR FROM ts_tz)
      )
      SELECT
        hourly_counts.hour,
        CASE
          WHEN day_count.total_days > 0
          THEN ROUND((hourly_counts.count::numeric / day_count.total_days), 2)
          ELSE 0
        END as avg_count
      FROM hourly_counts
      CROSS JOIN day_count
      ORDER BY hourly_counts.hour
    `;

    const result = await client.queryObject<{hour: number; avg_count: number}>(
      query,
      params
    );

    // Initialize array with 0s for all 24 hours
    const hourly = new Array(24).fill(0);

    // Fill in the actual averages
    for (const row of result.rows) {
      hourly[row.hour] = Number(row.avg_count);
    }

    return {hourly};
  } finally {
    client.release();
  }
}

/**
 * Close the database pool
 */
export async function closePool(): Promise<void> {
  await pool.end();
}
