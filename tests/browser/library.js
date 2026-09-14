// Driving the web build's own pages — the library, and the search over it.
//
// `docs/specs/0004-testing-playwright.md` listed these pages under "Still
// open": Playwright reached the stream viewer and the console, but nothing the
// person building a running order actually touches. This is the first of them.
//
// The library is not seeded over a control port the way the stream tests seed
// a service. It is compiled in: `CANTARA_BUNDLED_REPOS=local/testsongs` makes
// `build.rs` embed `bundled_repos/local/testsongs` into the WebAssembly, and
// `Settings::ensure_bundled_repos` loads it at startup and skips the wizard.
// So the page the tests open already has a library, with no network, no file
// picker and nothing for a test to set up. See `logic::bundled_repos`, and
// `bundle-library.mjs` for where those files come from.

import { expect } from '@playwright/test';

/// The elements in `bundled_repos/local/testsongs`, by the names they are
/// searched under.
///
/// Written down here rather than in each test, because the tests below are
/// about *how many* hits a query has and *in what order* — which is only
/// meaningful against a library somebody can look at. Two of the songs are
/// deliberately the same title from different files; that is what the library
/// contains, and the search has to cope with it.
export const LIBRARY = {
  /// A query with three hits whose last one differs from the first two, so
  /// that walking the list is visible in the text and not only in a class.
  threeHits: 'amazing',
  threeHitTitles: ['Amazing Grace', 'Amazing Grace', 'Alas, and Did My Savior Bleed'],

  /// A query with exactly one hit — a PDF, so the hit is not a song either.
  oneHit: 'page',
  oneHitTitle: 'MultiPage',
};

/// Opens the web build and waits until it has a library to search.
///
/// The wait is the point. The page loads, the WebAssembly starts, the bundled
/// repository is unpacked and only then is there anything to find; a test that
/// started typing before that would be asserting against an empty library and
/// would fail somewhere far away from the reason.
export async function openLibrary(page) {
  await page.goto('/Cantara/');

  // The web build opens the detail view rather than the selection — it is
  // mostly used to look songs up. See the `wasm32` redirect in `Selection`.
  await expect(page).toHaveURL(/\/detail/);
  await expect(page.locator('#searchinput')).toBeVisible();

  // Named, because this is the assertion that failed in CI and the name is
  // the whole difference between a five-minute diagnosis and a long one. An
  // application with no library looks entirely healthy from the outside: the
  // wizard is skipped, the route is right, the search field is there, and
  // every search finds nothing.
  await expect(
    page.locator('.selection_item').first(),
    'the web build was compiled without a library — see bundle-library.mjs',
  ).toBeVisible();

  return page.locator('#searchinput');
}

/// Opens the web build and crosses to the selection view.
///
/// Through the button a person would use, rather than by asking the router:
/// the redirect above fires on every fresh load, so navigating straight to `/`
/// lands back in the detail view and a test that did it would be testing the
/// redirect instead of the selection.
export async function openSelection(page) {
  await openLibrary(page);
  await page.locator('#view-mode-toggle').click();
  await expect(page).not.toHaveURL(/\/detail/);
  await expect(page.locator('#searchinput')).toBeVisible();

  return page.locator('#searchinput');
}

/// Types a query the way a person does — key by key, into the focused field.
///
/// `fill` is deliberately not used: it sets the value in one assignment and
/// fires one event, which is exactly the case that never broke. The bug these
/// tests exist for lived between one keystroke and the next.
export async function search(page, query, { delay = 40 } = {}) {
  const field = page.locator('#searchinput');
  await field.click();
  await field.pressSequentially(query, { delay });
  await expect(page.locator('.search-results')).toBeVisible();
}

/// The hits on screen, in the order they are listed.
export function hits(page) {
  return page.locator('.search-result-title');
}

/// Which hit the keyboard is on, counted from the top of the list.
///
/// `-1` where nothing is marked, so that "nothing is marked" fails with a
/// number rather than a timeout somewhere else.
export async function markedHit(page) {
  return page.locator('.search-result').evaluateAll((rows) =>
    rows.findIndex((row) => row.classList.contains('search-result-active')),
  );
}

/// Waits until the marked hit is the one expected.
///
/// A keypress goes to Rust and the class comes back, so every assertion about
/// the mark has to be given time; `expect.poll` retries where a bare read
/// would race.
export async function expectMarkedHit(page, index) {
  await expect
    .poll(() => markedHit(page), { message: `the hit at ${index} should be marked` })
    .toBe(index);
}
