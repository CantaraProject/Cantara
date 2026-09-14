// Looking a song up, and taking one without touching the mouse.
//
// The search field is how a running order is built under time pressure: the
// person doing it is listening to the service, not looking at the screen, and
// types the next song while the last one is still being sung. Three defects
// came out of that use, and each one has its tests below.
//
// * **Letters were swallowed.** The field was bound with `value:`, which
//   Dioxus rewrites into the element on *every* redraw. A letter typed between
//   a keystroke and the redraw it caused was overwritten by the older query
//   before its own event had been handled. Typing quickly lost letters. This
//   one is out of Playwright's reach and the first block below says why.
// * **The keyboard could not take a hit.** The first hit was not marked and
//   Enter did nothing, so the mouse was the only way to collect a song.
// * **The three ways of taking a hit had drifted apart.** Clicking, pressing
//   Enter and the `Alt` shortcut each spelled it out separately, and only one
//   of them emptied the field — so the next query was typed onto the end of
//   the last one.
//
// These run against the web build, which is the only surface of the three
// Playwright can reach. The field itself is the same component the desktop
// shows; what a browser cannot say anything about is the desktop's WebKitGTK
// window around it. See `docs/specs/0004-testing-playwright.md`.

import { test, expect } from '@playwright/test';
import { WEB } from '../../playwright.config.js';
import {
  LIBRARY,
  openLibrary,
  openSelection,
  search,
  hits,
  markedHit,
  expectMarkedHit,
} from './library.js';

test.use({ baseURL: WEB });

test.describe('the field keeps what is typed into it', () => {
  // What this describe block can and cannot see, because it was measured
  // rather than assumed.
  //
  // The swallowed letters were a desktop defect and they can only be a desktop
  // defect. The old binding rewrote the element's text from Cantara's copy of
  // the query on every redraw, and that only loses a letter if a redraw can
  // land *between* a keystroke and the event reaching Rust. In the desktop's
  // `wry` window that gap is an IPC round trip across a process boundary. In
  // the web build there is no such gap: the event and the redraw it causes are
  // the same turn of one event loop.
  //
  // This was checked by putting the old `value:` binding back and running
  // these tests against it. **They passed.** Two of them were written as
  // regression tests and were deleted again, because a test that cannot fail
  // on the defect it names is worse than no test — it is a claim of coverage
  // that is not there.
  //
  // So what stays is one test of the surface as it behaves in a browser. It
  // would catch a *web* regression of this shape. It would not have caught the
  // defect that was reported, and nothing driven by Playwright could have: see
  // "What can actually be reached" in the spec. The desktop case is covered by
  // a person typing into the window and by nothing else.

  test('a query typed as fast as the keys go arrives whole', async ({ page }) => {
    const field = await openLibrary(page);

    // No delay between the keys, which is what a fast typist is.
    await field.click();
    await field.pressSequentially('amazing grace', { delay: 0 });

    await expect(field).toHaveValue('amazing grace');
    await expect(page.locator('.search-results')).toBeVisible();
  });
});

test.describe('the keyboard walks the hits', () => {
  test('the first hit is marked as soon as there are hits', async ({ page }) => {
    // What makes Enter alone enough: a query that has just been typed is
    // already standing on its best hit.
    await openLibrary(page);
    await search(page, LIBRARY.threeHits);

    await expect(hits(page)).toHaveText(LIBRARY.threeHitTitles);
    await expectMarkedHit(page, 0);
  });

  test('the down arrow moves on and the up arrow back', async ({ page }) => {
    const field = await openLibrary(page);
    await search(page, LIBRARY.threeHits);

    await field.press('ArrowDown');
    await expectMarkedHit(page, 1);

    await field.press('ArrowDown');
    await expectMarkedHit(page, 2);

    await field.press('ArrowUp');
    await expectMarkedHit(page, 1);
  });

  test('tab walks the list instead of leaving the field', async ({ page }) => {
    // Both halves matter. Tab moving the mark is the feature; the focus
    // staying put is what makes the feature usable, because the next letter
    // typed has to reach the query and not whatever Tab would have moved to.
    const field = await openLibrary(page);
    await search(page, LIBRARY.threeHits);

    await field.press('Tab');
    await expectMarkedHit(page, 1);
    await expect(field).toBeFocused();

    await field.press('Shift+Tab');
    await expectMarkedHit(page, 0);
    await expect(field).toBeFocused();
  });

  test('the ends of the list are joined', async ({ page }) => {
    // Holding a key down round-trips rather than sticking against the end,
    // which is what a list this short wants: three hits and the wrong one
    // marked is one key away either way.
    const field = await openLibrary(page);
    await search(page, LIBRARY.threeHits);

    await field.press('ArrowUp');
    await expectMarkedHit(page, LIBRARY.threeHitTitles.length - 1);

    await field.press('ArrowDown');
    await expectMarkedHit(page, 0);
  });

  test('a changed query starts at its own best hit again', async ({ page }) => {
    // A different query is a different list, and the mark cannot be left
    // pointing at a position in the old one.
    const field = await openLibrary(page);
    await search(page, LIBRARY.threeHits);
    await field.press('ArrowDown');
    await expectMarkedHit(page, 1);

    await field.press('Backspace');

    await expectMarkedHit(page, 0);
  });

  test('the arrow keys do not move the caret while the list is open', async ({ page }) => {
    // Otherwise walking the list would quietly reposition where the next
    // letter is inserted, and the query would be typed into its own middle.
    const field = await openLibrary(page);
    await search(page, LIBRARY.threeHits);

    await field.press('ArrowUp');
    await field.press('ArrowDown');

    const caret = await field.evaluate((element) => element.selectionStart);
    expect(caret).toBe(LIBRARY.threeHits.length);
  });
});

test.describe('taking a hit, in the selection view', () => {
  test('enter puts the marked hit into the running order and empties the field', async ({
    page,
  }) => {
    const field = await openSelection(page);
    await search(page, LIBRARY.oneHit);
    await expect(hits(page)).toHaveText([LIBRARY.oneHitTitle]);

    await field.press('Enter');

    await expect(page.locator('.selected-item-name')).toHaveText([LIBRARY.oneHitTitle]);
    // The search is over: what was looked for should not be standing in the
    // way of the next one, and the list of hits for nothing is no list.
    await expect(field).toHaveValue('');
    await expect(page.locator('.search-results')).toBeHidden();
  });

  test('enter takes the hit the keyboard was moved to, not the first', async ({ page }) => {
    const field = await openSelection(page);
    await search(page, LIBRARY.threeHits);

    await field.press('ArrowDown');
    await field.press('ArrowDown');
    await expectMarkedHit(page, 2);
    await field.press('Enter');

    await expect(page.locator('.selected-item-name')).toHaveText([LIBRARY.threeHitTitles[2]]);
  });

  test('the alt shortcut empties the field too', async ({ page }) => {
    // This is the one that had drifted. `Alt+3` collected the song and left
    // the query standing, so the next song was typed onto the end of the last
    // one and found nothing. All three ways of taking a hit now go through
    // `ResultPicker::take`.
    const field = await openSelection(page);
    await search(page, LIBRARY.threeHits);

    await field.press('Alt+3');

    await expect(page.locator('.selected-item-name')).toHaveText([LIBRARY.threeHitTitles[2]]);
    await expect(field).toHaveValue('');
    await expect(page.locator('.search-results')).toBeHidden();
  });

  test('a click empties the field too', async ({ page }) => {
    const field = await openSelection(page);
    await search(page, LIBRARY.oneHit);

    await hits(page).first().click();

    await expect(page.locator('.selected-item-name')).toHaveText([LIBRARY.oneHitTitle]);
    await expect(field).toHaveValue('');
  });

  test('escape abandons the search without taking anything', async ({ page }) => {
    // Escape empties the field rather than only putting the list away, and
    // this test is here because the first version of it asserted the opposite
    // and failed. The browser empties an `input type="search"` on Escape by
    // itself, without an event — so "keep the query" was never a state the two
    // could be in agreement about. What it must not do is take a hit.
    const field = await openSelection(page);
    await search(page, LIBRARY.threeHits);

    await field.press('Escape');

    await expect(page.locator('.search-results')).toBeHidden();
    await expect(field).toHaveValue('');
    await expect(page.locator('.selected-item-name')).toHaveCount(0);
  });
});

test.describe('taking a hit, in the detail view', () => {
  test('enter opens the marked hit instead of collecting it', async ({ page }) => {
    // The same field and the same keys, and deliberately not the same result:
    // the detail view is for looking an element up. A search here used to add
    // the song to the presentation instead, which is the defect
    // `ItemClickAction` exists to prevent — and Enter is a third way to make
    // it, so it gets its own test.
    //
    // A song rather than the one-hit PDF: the library list shows one kind of
    // element at a time, so an opened PDF is not in the list the detail view
    // is drawing and there would be nothing on screen to assert against.
    const field = await openLibrary(page);
    await search(page, LIBRARY.threeHits);

    await field.press('Enter');

    await expect(page.locator('.selection_item-active')).toContainText(
      LIBRARY.threeHitTitles[0],
    );
    await expect(field).toHaveValue('');
  });
});

test.describe('where the list sits', () => {
  test('the hits hang directly under the field, with nothing showing between', async ({
    page,
  }) => {
    // The list used to be positioned at a fixed 80px from the top of the page,
    // a guess at how tall the bar holding the field is. Whenever the guess was
    // off, a strip of the page behind showed through between the field and its
    // results. It is now measured from the bar's own bottom edge, and this is
    // the assertion that says so.
    await openLibrary(page);
    await search(page, LIBRARY.threeHits);

    const bar = await page.locator('header.top-bar').boundingBox();
    const list = await page.locator('.search-results').boundingBox();

    expect(list.y - (bar.y + bar.height)).toBeLessThanOrEqual(1);
    expect(list.width).toBeCloseTo(bar.width, 0);
  });
});
