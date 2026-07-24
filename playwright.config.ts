import { defineConfig, devices } from '@playwright/test';

/**
 * Playwright configuration for Cantara web E2E tests.
 *
 * The tests run against a pre-built static web bundle served by a local HTTP
 * server.  In CI the server is started by the workflow before this suite runs;
 * locally you can start it with:
 *
 *   npx serve target/dx/cantara/release/web/public -p 8080
 */
export default defineConfig({
  testDir: './tests/playwright',
  timeout: 30_000,
  expect: { timeout: 10_000 },
  fullyParallel: false,
  retries: process.env.CI ? 2 : 0,
  reporter: process.env.CI ? 'github' : 'list',

  use: {
    baseURL: process.env.APP_URL ?? 'http://localhost:8080',
    headless: true,
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
  },

  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
});
