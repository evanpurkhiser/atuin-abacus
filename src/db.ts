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

export interface TotalCommands {
  total: number;
}

/**
 * Get the count of commands run per day within a date range
 */
export async function getCommandsPerDay(opts: Period): Promise<DailyCommandCount[]> {
  const {startDate, endDate, timezone} = opts;
  const client = await pool.connect();
  try {
    // Union query to combine both old history table and new store table
    let query = `
      WITH combined AS (
        -- Old history table
        SELECT timestamp AT TIME ZONE $1 as ts
        FROM history
        WHERE deleted_at IS NULL

        UNION ALL

        -- New store table (timestamps are in nanoseconds, convert to seconds)
        SELECT to_timestamp(timestamp / 1000000000.0) AT TIME ZONE $1 as ts
        FROM store
        WHERE tag = 'history'
      )
      SELECT
        date(ts) as date,
        COUNT(*) as count
      FROM combined
      WHERE 1=1
    `;
    const params: string[] = [timezone];

    if (startDate) {
      params.push(startDate);
      query += ` AND date(ts) >= $${params.length}`;
    }
    if (endDate) {
      params.push(endDate);
      query += ` AND date(ts) <= $${params.length}`;
    }

    query += ` GROUP BY date(ts) ORDER BY date`;

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
 * Get the total count of all commands
 */
export async function getTotalCommands(opts: Period): Promise<TotalCommands> {
  const {startDate, endDate, timezone} = opts;
  const client = await pool.connect();
  try {
    // Union query to combine both old history table and new store table
    let query = `
      WITH combined AS (
        -- Old history table
        SELECT timestamp AT TIME ZONE $1 as ts
        FROM history
        WHERE deleted_at IS NULL

        UNION ALL

        -- New store table (timestamps are in nanoseconds, convert to seconds)
        SELECT to_timestamp(timestamp / 1000000000.0) AT TIME ZONE $1 as ts
        FROM store
        WHERE tag = 'history'
      )
      SELECT COUNT(*) as total
      FROM combined
      WHERE 1=1
    `;
    const params: string[] = [timezone];

    if (startDate) {
      params.push(startDate);
      query += ` AND date(ts) >= $${params.length}`;
    }
    if (endDate) {
      params.push(endDate);
      query += ` AND date(ts) <= $${params.length}`;
    }

    const result = await client.queryObject<{total: number}>(query, params);

    return {
      total: Number(result.rows[0].total),
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
      dateFilter += ` AND date(ts) >= $${params.length}`;
    }
    if (endDate) {
      params.push(endDate);
      dateFilter += ` AND date(ts) <= $${params.length}`;
    }

    // Query to get average commands per hour across all days, combining both tables
    const query = `
      WITH combined AS (
        -- Old history table
        SELECT timestamp AT TIME ZONE $1 as ts
        FROM history
        WHERE deleted_at IS NULL

        UNION ALL

        -- New store table (timestamps are in nanoseconds, convert to seconds)
        SELECT to_timestamp(timestamp / 1000000000.0) AT TIME ZONE $1 as ts
        FROM store
        WHERE tag = 'history'
      ),
      day_count AS (
        SELECT COUNT(DISTINCT date(ts)) as total_days
        FROM combined
        WHERE 1=1 ${dateFilter}
      ),
      hourly_counts AS (
        SELECT
          EXTRACT(HOUR FROM ts)::integer as hour,
          COUNT(*) as count
        FROM combined
        WHERE 1=1 ${dateFilter}
        GROUP BY EXTRACT(HOUR FROM ts)
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
