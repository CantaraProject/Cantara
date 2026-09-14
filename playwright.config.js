// How the browser tests are run. See `docs/specs/0004-testing-playwright.md`.
//
// The server under test is Cantara itself, started by `logic::harness` — not a
// mock and not a static page. It runs the real network side: the real helper
// process, the real rendering, the real socket. See that module for why it is
// behind a cargo feature rather than a command-line flag.

import { defineConfig, devices } from '@playwright/test';

/// Where the harness serves the stream. Fixed rather than negotiated: the
/// helper picks its own address on a real network, and a test that had to
/// discover the port would have to parse it out of stdout before it could open
/// anything.
export const STREAM = 'http://127.0.0.1:8430';

/// Where the harness takes instructions. A different socket from the stream —
/// see `logic::harness`.
export const CONTROL = 'http://127.0.0.1:8431';

export default defineConfig({
  testDir: './tests/browser',

  // A slide change goes Cantara → helper → socket → page, so an assertion that
  // fails takes as long as it is given. Short enough that a genuine failure is
  // reported while somebody is still watching.
  expect: { timeout: 5000 },
  timeout: 30000,

  // Refusing `.only` in CI, because it is how a suite quietly stops running.
  forbidOnly: !!process.env.CI,

  // One retry in CI and none locally. A test that only passes on the second
  // attempt is a test with a race in it, and locally that should be visible
  // rather than smoothed over; in CI, an infrastructure hiccup should not turn
  // the whole run red.
  retries: process.env.CI ? 1 : 0,

  // Serially. These tests drive one running service through one control port,
  // so two of them at once would be two people pressing "next".
  workers: 1,

  reporter: process.env.CI ? [['github'], ['list']] : [['list']],

  use: {
    baseURL: STREAM,
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
  },

  // Chromium only, and Ubuntu only in CI. Every surface these tests reach is
  // served over HTTP and drawn by the browser Playwright brings with it, so a
  // second operating system would exercise the same code twice. That is the
  // decision recorded in the spec, and this is where it is spent.
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],

  webServer: {
    // Built with the feature that compiles the harness in. A plain `cargo run`
    // starts Cantara's window instead and the tests would wait for a page that
    // is never served.
    command: 'cargo run --features test-harness -- --test-harness',
    url: STREAM,
    // Locally, an already-running harness is reused — a Rust rebuild between
    // every run of a browser test is a minute nobody spends twice. In CI there
    // is nothing to reuse and a stale process would be a lie.
    reuseExistingServer: !process.env.CI,
    // A cold `cargo build` of this program is not quick.
    timeout: 300000,
    stdout: 'pipe',
    stderr: 'pipe',
  },
});
