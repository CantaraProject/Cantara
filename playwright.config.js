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

/// Where the web build is served: Cantara's own pages, rather than the stream
/// the helper serves. A separate server and a separate base address, because
/// they are separate programs — one is the application compiled to
/// WebAssembly, the other is the network side of the desktop one.
///
/// The port and the `/Cantara/` path are `Dioxus.toml`'s, not chosen here.
export const WEB = 'http://127.0.0.1:8080';

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

  // Two servers, because the tests look at two programs. Both are started for
  // any run, which costs a WebAssembly build even when only the stream tests
  // are being run; `reuseExistingServer` makes that a once-per-session cost
  // locally, and in CI a run builds both anyway.
  webServer: [
    {
      // Built with the feature that compiles the harness in. A plain
      // `cargo run` starts Cantara's window instead and the tests would wait
      // for a page that is never served.
      command: 'cargo run --features test-harness -- --test-harness',
      url: STREAM,
      // Locally, an already-running harness is reused — a Rust rebuild between
      // every run of a browser test is a minute nobody spends twice. In CI
      // there is nothing to reuse and a stale process would be a lie.
      reuseExistingServer: !process.env.CI,
      // A cold `cargo build` of this program is not quick.
      timeout: 300000,
      stdout: 'pipe',
      stderr: 'pipe',
    },
    {
      // The web build, with a library compiled into it.
      //
      // Three steps, and each one is here because leaving it out failed in CI.
      //
      // `bundle-library.mjs` puts a library where the build can find it;
      // `CANTARA_BUNDLED_REPOS` is what makes `build.rs` embed it and
      // `Settings::ensure_bundled_repos` skip the welcome wizard. Its own
      // comment explains why the directory is not simply in the repository.
      //
      // Then `dx build`, and a plain static server rather than `dx serve`.
      // `serve-web.mjs` explains why at length: the short of it is that a dev
      // server opens its port before it has built anything and answers 200
      // while it works, which Playwright's readiness check cannot tell apart
      // from the finished application. In CI it declared the server ready at
      // two seconds and ran the whole suite against a page that was still five
      // minutes from existing. Building first, then serving what was built, is
      // the only arrangement here where "the port is open" means "the
      // application is there".
      command:
        'node tests/browser/bundle-library.mjs' +
        ' && CANTARA_BUNDLED_REPOS=local/testsongs dx build --platform web' +
        ' && node tests/browser/serve-web.mjs',
      // The served page, not the bare origin: `Dioxus.toml` sets a base path.
      url: `${WEB}/Cantara/`,
      reuseExistingServer: !process.env.CI,
      // Long enough for a cold WebAssembly build, which is slower again than
      // the native one above and now happens before the port opens at all —
      // so this timeout covers the build rather than only the server start.
      timeout: 900000,
      stdout: 'pipe',
      stderr: 'pipe',
    },
  ],
});
