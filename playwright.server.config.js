// The browser tests of the server mode — `docs/specs/0008-server-version.md`.
//
// Separate from `playwright.config.js` because they need a PostgreSQL
// (`CANTARA_BROWSER_DATABASE_URL`, see `tests/server/start-server.mjs`), which
// the stream tests do not, and a second WebAssembly build.
//
// Two programs again: the Cantara server with the web build it serves, and
// the desktop program's test harness, which is what logs in to the server and
// shares a presentation through it the way the desktop does — with the test's
// browser playing the user.

import { defineConfig, devices } from '@playwright/test';

export const SERVER = 'http://127.0.0.1:8095';
export const CONTROL = 'http://127.0.0.1:8431';

export default defineConfig({
  testDir: './tests/server',
  expect: { timeout: 10000 },
  timeout: 60000,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  // One server, one harness, one login at a time.
  workers: 1,
  reporter: process.env.CI ? [['github'], ['list']] : [['list']],
  use: {
    baseURL: SERVER,
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  webServer: [
    {
      command: 'node tests/server/start-server.mjs',
      url: `${SERVER}/healthz`,
      reuseExistingServer: !process.env.CI,
      timeout: 900000,
      stdout: 'pipe',
      stderr: 'pipe',
    },
    {
      command: 'cargo run --features test-harness -- --test-harness',
      url: 'http://127.0.0.1:8430',
      reuseExistingServer: !process.env.CI,
      // A cold build of the desktop program, which in CI runs after the
      // server's and the WebAssembly build — CI builds it in a step of its own
      // first, and this is the margin for when it has not.
      timeout: 900000,
      stdout: 'pipe',
      stderr: 'pipe',
    },
  ],
});
