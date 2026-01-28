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

/**
 * Get the count of commands run per day within a date range
 */
export async function getCommandsPerDay(opts: Period): Promise<DailyCommandCount[]> {
  const {startDate, endDate, timezone} = opts;
  const client = await pool.connect();
  try {
    let query = `
      SELECT
        date(timestamp AT TIME ZONE $1) as date,
        COUNT(*) as count
      FROM history
      WHERE deleted_at IS NULL
    `;
    const params: string[] = [timezone];

    if (startDate) {
      params.push(startDate);
      query += ` AND date(timestamp AT TIME ZONE $1) >= $${params.length}`;
    }
    if (endDate) {
      params.push(endDate);
      query += ` AND date(timestamp AT TIME ZONE $1) <= $${params.length}`;
    }

    query += ` GROUP BY date(timestamp AT TIME ZONE $1) ORDER BY date`;

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
 * Get average commands per hour of day (0-23)
 * Returns an array of 24 numbers representing the average number of commands
 * per day for each hour. For example, if you ran 50 commands at 9am over 10 days,
 * the value at index 9 would be 5.0
 */
export async function getTimeOfDayStats(opts: Period): Promise<TimeOfDayStats> {
  const {startDate, endDate, timezone} = opts;
  const client = await pool.connect();
  try {
    // Build the WHERE clause for date filtering
    let whereClause = 'WHERE deleted_at IS NULL';
    const params: string[] = [timezone];

    if (startDate) {
      params.push(startDate);
      whereClause += ` AND date(timestamp AT TIME ZONE $1) >= $${params.length}`;
    }
    if (endDate) {
      params.push(endDate);
      whereClause += ` AND date(timestamp AT TIME ZONE $1) <= $${params.length}`;
    }

    // Query to get average commands per hour across all days
    const query = `
      WITH day_count AS (
        SELECT COUNT(DISTINCT date(timestamp AT TIME ZONE $1)) as total_days
        FROM history
        ${whereClause}
      ),
      hourly_counts AS (
        SELECT
          EXTRACT(HOUR FROM timestamp AT TIME ZONE $1)::integer as hour,
          COUNT(*) as count
        FROM history
        ${whereClause}
        GROUP BY EXTRACT(HOUR FROM timestamp AT TIME ZONE $1)
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
 * Test database connection
 */
export async function testConnection(): Promise<boolean> {
  try {
    const client = await pool.connect();
    await client.queryObject('SELECT 1');
    client.release();
    return true;
  } catch (error) {
    console.error('Database connection failed:', error);
    return false;
  }
}

/**
 * Close the database pool
 */
export async function closePool(): Promise<void> {
  await pool.end();
}
