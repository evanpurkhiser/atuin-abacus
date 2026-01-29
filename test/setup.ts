import {Pool} from 'postgres';

const TEST_DATABASE_URL =
  Deno.env.get('TEST_DATABASE_URL') ||
  'postgresql://atuin_test:atuin_test@127.0.0.1:5433/atuin_test';

/**
 * Set up the test database with schema and fixtures
 */
export async function setupTestDatabase(): Promise<void> {
  const pool = new Pool(TEST_DATABASE_URL, 1, true);
  const client = await pool.connect();

  try {
    // Read and execute schema
    const schemaPath = new URL('./schema.sql', import.meta.url).pathname;
    const schema = await Deno.readTextFile(schemaPath);
    await client.queryArray(schema);

    // Read and execute fixtures
    const fixturesPath = new URL('./fixtures.sql', import.meta.url).pathname;
    const fixtures = await Deno.readTextFile(fixturesPath);
    await client.queryArray(fixtures);

    console.log('✓ Test database setup complete');
  } finally {
    client.release();
    await pool.end();
  }
}

/**
 * Clean up the test database
 */
export async function cleanupTestDatabase(): Promise<void> {
  const pool = new Pool(TEST_DATABASE_URL, 1, true);
  const client = await pool.connect();

  try {
    await client.queryArray('TRUNCATE TABLE history CASCADE');
    await client.queryArray('TRUNCATE TABLE store CASCADE');
    console.log('✓ Test database cleaned up');
  } finally {
    client.release();
    await pool.end();
  }
}
