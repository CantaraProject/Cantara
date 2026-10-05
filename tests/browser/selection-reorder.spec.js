// Putting the running order and the sidebar's source icons in order.
//
// Both were dragged by implementations of their own before
// `docs/specs/0006-reorder-designs-and-slide-settings.md` gave every list in
// Cantara the same one (`components::reorder`). The designs and the slide
// settings are tested in `settings-reorder.spec.js`; this is the other two
// users of the same code, so that a change made for one list cannot quietly
// break the others.

import { test, expect } from '@playwright/test';
import { WEB } from '../../playwright.config.js';
import { LIBRARY, openSelection, search } from './library.js';

test.use({ baseURL: WEB });

const ROWS = '#selected-items-list > .selected-item';
const ICONS = '#selection-sidebar > .selection-sidebar-item';

/// Opens the selection view with three elements in the running order, in a
/// known order: the PDF, then the third and the first hit for "amazing".
async function openRunningOrder(page) {
  const field = await openSelection(page);

  await search(page, LIBRARY.oneHit);
  await field.press('Enter');
  await search(page, LIBRARY.threeHits);
  await field.press('ArrowDown');
  await field.press('ArrowDown');
  await field.press('Enter');
  await search(page, LIBRARY.threeHits);
  await field.press('Enter');

  await expect(page.locator(`${ROWS} .selected-item-name`)).toHaveText([
    LIBRARY.oneHitTitle,
    LIBRARY.threeHitTitles[2],
    LIBRARY.threeHitTitles[0],
  ]);
}

/// The names in the running order, top to bottom.
async function runningOrder(page) {
  return page.locator(`${ROWS} .selected-item-name`).allTextContents();
}

/// Whether every item of a list is back in its place — nothing still carried,
/// nothing still moved off to where the pointer let go.
///
/// The order alone does not say so. A dropped item used to land in the right
/// place in the list and stay *drawn* where it had been let go, because its
/// transform was never taken off again — see `ReorderDrag::item_style`.
async function expectAtRest(items) {
  await expect
    .poll(() =>
      items.evaluateAll((all) =>
        all.every(
          (item) =>
            getComputedStyle(item).transform === 'none' &&
            !item.classList.contains('reorder-dragging'),
        ),
      ),
    )
    .toBe(true);
}

/// The middle of an element, on screen.
async function middleOf(locator) {
  const box = await locator.boundingBox();
  return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
}

/// Drags with the mouse, in steps, the way a hand does.
async function mouseDrag(page, from, to) {
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(from.x + 2, from.y + 2, { steps: 2 });
  await page.mouse.move(to.x, to.y, { steps: 12 });
  await page.mouse.up();
}

/// Drags with a finger, through the DevTools protocol — see
/// `settings-reorder.spec.js` for why not Playwright's own touchscreen.
async function touchDrag(page, from, to, steps = 12) {
  const client = await page.context().newCDPSession(page);
  const point = (x, y) => [{ x: Math.round(x), y: Math.round(y), id: 1 }];

  await client.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: point(from.x, from.y) });
  for (let step = 1; step <= steps; step += 1) {
    const x = from.x + ((to.x - from.x) * step) / steps;
    const y = from.y + ((to.y - from.y) * step) / steps;
    await client.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: point(x, y) });
    await page.waitForTimeout(16);
  }
  await client.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await client.detach();
}

test.describe('the running order', () => {
  test('a row dragged to the top is the first row', async ({ page }) => {
    await openRunningOrder(page);
    const rows = page.locator(ROWS);

    const firstBox = await rows.nth(0).boundingBox();
    await mouseDrag(page, await middleOf(rows.nth(2)), {
      x: firstBox.x + firstBox.width / 2,
      y: firstBox.y + 2,
    });

    await expect
      .poll(() => runningOrder(page))
      .toEqual([LIBRARY.threeHitTitles[0], LIBRARY.oneHitTitle, LIBRARY.threeHitTitles[2]]);
    await expectAtRest(rows);
  });

  test('the row open in the options follows the move', async ({ page }) => {
    await openRunningOrder(page);
    const rows = page.locator(ROWS);
    await rows.nth(0).click();
    await expect(rows.nth(0)).toHaveClass(/selection_item-active/);

    const lastBox = await rows.nth(2).boundingBox();
    await mouseDrag(page, await middleOf(rows.nth(0)), {
      x: lastBox.x + lastBox.width / 2,
      y: lastBox.y + lastBox.height - 2,
    });

    await expect.poll(() => runningOrder(page)).toEqual([
      LIBRARY.threeHitTitles[2],
      LIBRARY.threeHitTitles[0],
      LIBRARY.oneHitTitle,
    ]);
    await expect(rows.nth(2)).toHaveClass(/selection_item-active/);
    await expect(page.locator(`${ROWS}.selection_item-active`)).toHaveCount(1);
  });

  test('the arrow buttons move a row by one', async ({ page }) => {
    await openRunningOrder(page);

    await page.locator(ROWS).nth(0).locator('.selected-item-action').nth(1).click();

    await expect
      .poll(() => runningOrder(page))
      .toEqual([LIBRARY.threeHitTitles[2], LIBRARY.oneHitTitle, LIBRARY.threeHitTitles[0]]);
  });

  test('Alt and an arrow move the focused row', async ({ page }) => {
    await openRunningOrder(page);
    const rows = page.locator(ROWS);

    // The selection view sends every key that reaches it to the search field,
    // so that typing anywhere looks a song up. The Alt of an Alt-and-arrow
    // used to be one of them: it took the focus to the search before the
    // arrow arrived, and the row never moved. See `ReorderDrag::keydown`.
    await rows.nth(2).focus();
    await page.keyboard.press('Alt+ArrowUp');

    await expect
      .poll(() => runningOrder(page))
      .toEqual([LIBRARY.oneHitTitle, LIBRARY.threeHitTitles[0], LIBRARY.threeHitTitles[2]]);
    await expect(rows.nth(1)).toBeFocused();
  });

  test.describe('with a finger', () => {
    test.use({ hasTouch: true, isMobile: true });

    test('a row dragged by its grip moves', async ({ page }) => {
      await openRunningOrder(page);
      const rows = page.locator(ROWS);

      const grip = rows.nth(2).locator('.selected-item-grip');
      await expect(grip, 'the grip is shown where the pointer is a finger').toBeVisible();
      const firstBox = await rows.nth(0).boundingBox();
      await touchDrag(page, await middleOf(grip), { x: firstBox.x + 20, y: firstBox.y + 2 });

      await expect
        .poll(() => runningOrder(page))
        .toEqual([LIBRARY.threeHitTitles[0], LIBRARY.oneHitTitle, LIBRARY.threeHitTitles[2]]);
    });
  });
});

test.describe('the source icons in the sidebar', () => {
  /// The order the settings have stored, as the web build keeps them.
  async function storedOrder(page) {
    return page.evaluate(() => JSON.parse(localStorage.getItem('cantara-settings') ?? '{}').sidebar_order);
  }

  test('an icon dragged below another moves there, and the order is kept', async ({ page }) => {
    await openSelection(page);
    const icons = page.locator(ICONS);
    await expect(icons.first()).toBeVisible();
    await expect(icons).toHaveCount(5);

    const thirdBox = await icons.nth(2).boundingBox();
    await mouseDrag(page, await middleOf(icons.nth(0)), {
      x: thirdBox.x + thirdBox.width / 2,
      y: thirdBox.y + thirdBox.height - 2,
    });

    // The first icon is now the third — moved, not swapped: the second and
    // the third each move up by one.
    // `logic::settings::default_sidebar_order`.
    const moved = ['Pictures', 'Videos', 'Songs', 'Pdfs', 'Markdown'];
    await expect.poll(() => storedOrder(page)).toEqual(moved);
    await expectAtRest(icons);

    await page.reload();
    await expect.poll(() => storedOrder(page)).toEqual(moved);
  });

  test('a click without moving still picks the source', async ({ page }) => {
    await openSelection(page);
    const icons = page.locator(ICONS);

    await icons.nth(1).click();

    await expect(icons.nth(1)).not.toHaveClass(/secondary/);
    await expect(icons.nth(0)).toHaveClass(/secondary/);
  });
});
