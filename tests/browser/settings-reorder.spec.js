// Putting the presentation designs and the slide settings in order.
//
// `docs/specs/0006-reorder-designs-and-slide-settings.md`. The arithmetic —
// where a drop lands, and that every stored choice follows the item it chose —
// is covered by the unit tests in `logic::reorder` and `logic::settings`, and
// what the lists render by the markup tests beside the components. What is
// left for a browser is everything those cannot see: that a real pointer, a
// real finger and a real keyboard reach that arithmetic at all, that the page
// still scrolls under a finger, and that the new order is still there after a
// reload.
//
// The settings are seeded through the browser's local storage, which is where
// the web build keeps them. That is the real way in and out — no control port,
// and nothing a test could get right that the program gets wrong.

import { test, expect } from '@playwright/test';
import { WEB } from '../../playwright.config.js';

test.use({ baseURL: WEB });

/// Where the web build keeps its settings. `logic::settings::SETTINGS_KEY`.
const SETTINGS_KEY = 'cantara-settings';

const DESIGNS = '#presentation-design-list > .presentation-design-selector-item';
const DIVISIONS = '#slide-settings-list > li';

/// Opens the settings with three designs and three slide divisions, each
/// with a name that says where it started.
///
/// The settings are only in local storage once the program has written them,
/// so the first visit makes it write them — by duplicating the default design,
/// which is what a person would do to get a second one — and the rest is
/// changed in the stored document and read back in by a reload.
async function openSeededSettings(page, change = () => {}) {
  await page.goto('/Cantara/settings');
  await expect(page.locator(DESIGNS).first()).toBeVisible();

  await page.locator('.presentation-design-actions button', { hasText: 'Duplicate' }).click();
  await page.waitForFunction((key) => localStorage.getItem(key) !== null, SETTINGS_KEY);

  const stored = JSON.parse(await page.evaluate((key) => localStorage.getItem(key), SETTINGS_KEY));
  const design = stored.presentation_designs[0];
  stored.presentation_designs = ['A', 'B', 'C'].map((name) => ({ ...design, name }));
  const division = stored.song_slide_settings[0];
  stored.song_slide_settings = ['One', 'Two', 'Three'].map((name) => ({
    ...division,
    name,
    description: '',
  }));
  stored.default_design_index = 0;
  stored.default_slide_settings_index = 0;
  change(stored);

  await page.evaluate(
    ([key, value]) => localStorage.setItem(key, value),
    [SETTINGS_KEY, JSON.stringify(stored)],
  );
  await page.reload();
  await expect(page.locator(DESIGNS)).toHaveCount(3);
  await expect(page.locator(DIVISIONS)).toHaveCount(3);
}

/// Scrolls the settings page so that a section starts at the top of the
/// window. A mouse can only press what is on screen, and the settings are
/// longer than any window.
async function showSection(page, id) {
  await page.locator(id).evaluate((section) => section.scrollIntoView({ block: 'start' }));
}

/// The settings document as it is stored right now.
async function storedSettings(page) {
  return JSON.parse(await page.evaluate((key) => localStorage.getItem(key), SETTINGS_KEY));
}

/// The names of the designs, in the order the page shows them.
///
/// By the text in the page rather than the text on screen: a tile that has
/// scrolled out of view is not drawn (`content-visibility: auto`, see
/// `PresentationViewer`), and the text on screen of a tile that is not drawn
/// is nothing at all.
async function designOrder(page) {
  return page.locator(`${DESIGNS} .presentation-title`).allTextContents();
}

/// The names of the slide divisions, in the order the page shows them.
async function divisionOrder(page) {
  return page.locator(`${DIVISIONS} .slide-settings-row-name`).allTextContents();
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

/// Drags with the mouse from one point to another, in steps, the way a hand
/// does. A single jump would be one `pointermove`, which is not a drag anyone
/// makes and would not exercise the threshold.
async function mouseDrag(page, from, to) {
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(from.x + 2, from.y + 2, { steps: 2 });
  await page.mouse.move(to.x, to.y, { steps: 12 });
  await page.mouse.up();
}

/// Drags with a finger, through the DevTools protocol.
///
/// Playwright's own `touchscreen` only taps. `Input.dispatchTouchEvent` goes
/// through the browser's input pipeline like a real finger, so it produces
/// pointer events of type `touch`, it honours `touch-action`, and a finger
/// that is not claimed scrolls the page. Chromium only — which is the only
/// browser this suite runs (spec 0004).
async function touchDrag(page, from, to, { steps = 12, holdAtEnd = 0 } = {}) {
  const client = await page.context().newCDPSession(page);
  const point = (x, y) => [{ x: Math.round(x), y: Math.round(y), id: 1 }];

  await client.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: point(from.x, from.y) });
  for (let step = 1; step <= steps; step += 1) {
    const x = from.x + ((to.x - from.x) * step) / steps;
    const y = from.y + ((to.y - from.y) * step) / steps;
    await client.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: point(x, y) });
    await page.waitForTimeout(16);
  }
  // Held still at the end — at the edge of the screen, this is where the page
  // scrolls by itself.
  for (let tick = 0; tick < holdAtEnd; tick += 1) {
    await client.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: point(to.x, to.y) });
    await page.waitForTimeout(50);
  }
  await client.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await client.detach();
}

/// The scroll position of the settings page — the `main` element scrolls, not
/// the document.
async function pageScroll(page) {
  return page.locator('main.content').evaluate((main) => main.scrollTop);
}

test.describe('the designs, with a mouse', () => {
  // Wide enough for two tiles to a line beside the jump list and the buttons,
  // and tall enough for the three tiles to be on screen together.
  test.use({ viewport: { width: 1600, height: 1000 } });
  test.beforeEach(async ({ page }) => {
    await openSeededSettings(page);
    await showSection(page, '#settings-presentation');
  });

  test('a design dragged behind the last one is the last one', async ({ page }) => {
    const tiles = page.locator(DESIGNS);

    const from = await middleOf(tiles.nth(0));
    const lastBox = await tiles.nth(2).boundingBox();
    // The right half of the last tile is the gap after it.
    await mouseDrag(page, from, {
      x: lastBox.x + lastBox.width * 0.85,
      y: lastBox.y + lastBox.height / 2,
    });

    await expect.poll(() => designOrder(page)).toEqual(['B', 'C', 'A']);
    await expectAtRest(tiles);
  });

  test('the new order is still there after a reload', async ({ page }) => {
    const tiles = page.locator(DESIGNS);

    const lastBox = await tiles.nth(2).boundingBox();
    await mouseDrag(page, await middleOf(tiles.nth(0)), {
      x: lastBox.x + lastBox.width * 0.85,
      y: lastBox.y + lastBox.height / 2,
    });
    await expect.poll(() => designOrder(page)).toEqual(['B', 'C', 'A']);

    await page.reload();

    await expect.poll(() => designOrder(page)).toEqual(['B', 'C', 'A']);
  });

  test('the design that is open beside the list stays open after it moved', async ({ page }) => {
    const tiles = page.locator(DESIGNS);

    await tiles.nth(0).click();
    await expect(page.locator('.presentation-design-actions h6')).toHaveText('A');

    const lastBox = await tiles.nth(2).boundingBox();
    await mouseDrag(page, await middleOf(tiles.nth(0)), {
      x: lastBox.x + lastBox.width * 0.85,
      y: lastBox.y + lastBox.height / 2,
    });

    await expect.poll(() => designOrder(page)).toEqual(['B', 'C', 'A']);
    await expect(page.locator('.presentation-design-actions h6')).toHaveText('A');
    await expect(tiles.nth(2)).toHaveClass(/active/);
  });

  test('the default design is still the same design after it moved', async ({ page }) => {
    // The defect this whole change had to avoid: the default is stored as a
    // position, and a move that forgot it would quietly make whatever slid
    // into that position the default.
    const tiles = page.locator(DESIGNS);

    const lastBox = await tiles.nth(2).boundingBox();
    await mouseDrag(page, await middleOf(tiles.nth(0)), {
      x: lastBox.x + lastBox.width * 0.85,
      y: lastBox.y + lastBox.height / 2,
    });
    await expect.poll(() => designOrder(page)).toEqual(['B', 'C', 'A']);

    const stored = await storedSettings(page);
    expect(stored.presentation_designs[stored.default_design_index].name).toBe('A');
  });

  test('a click without moving chooses the design and moves nothing', async ({ page }) => {
    const tiles = page.locator(DESIGNS);

    await tiles.nth(1).click();

    await expect(tiles.nth(1)).toHaveClass(/active/);
    expect(await designOrder(page)).toEqual(['A', 'B', 'C']);
    await expect(page.locator('.reorder-marker-left, .reorder-marker-right')).toHaveCount(0);
  });

  test('the arrows on a tile turn its pages and pick nothing up', async ({ page }) => {
    const tile = page.locator(DESIGNS).nth(0);

    await tile.hover();
    const next = tile.locator('.preview-navigation-button').nth(1);
    const box = await next.boundingBox();
    // Pressed and wiggled, as an unsteady hand does on a small button.
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width / 2 + 30, box.y + box.height / 2, { steps: 5 });
    await expect(page.locator('.reorder-dragging')).toHaveCount(0);
    await page.mouse.up();

    expect(await designOrder(page)).toEqual(['A', 'B', 'C']);
  });

  test('Escape during a drag puts everything back', async ({ page }) => {
    const tiles = page.locator(DESIGNS);
    const before = await storedSettings(page);

    const from = await middleOf(tiles.nth(0));
    const lastBox = await tiles.nth(2).boundingBox();
    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(lastBox.x + lastBox.width * 0.85, lastBox.y + lastBox.height / 2, {
      steps: 12,
    });
    await expect(page.locator('.reorder-dragging')).toHaveCount(1);
    await page.keyboard.press('Escape');
    await expect(page.locator('.reorder-dragging')).toHaveCount(0);
    await page.mouse.up();

    expect(await designOrder(page)).toEqual(['A', 'B', 'C']);
    expect(await storedSettings(page)).toEqual(before);
    await expectAtRest(tiles);
  });
});

test.describe('the designs, with the keyboard', () => {
  test.beforeEach(async ({ page }) => {
    await openSeededSettings(page);
  });

  test('Alt and an arrow move the focused design and keep the focus on it', async ({ page }) => {
    const tiles = page.locator(DESIGNS);

    await tiles.nth(0).focus();
    await page.keyboard.press('Alt+ArrowRight');

    await expect.poll(() => designOrder(page)).toEqual(['B', 'A', 'C']);
    await expect(tiles.nth(1)).toBeFocused();

    await page.keyboard.press('Alt+ArrowLeft');
    await expect.poll(() => designOrder(page)).toEqual(['A', 'B', 'C']);
  });
});

test.describe('the slide settings, with a mouse', () => {
  test.beforeEach(async ({ page }) => {
    await openSeededSettings(page);
    await showSection(page, '#settings-slides');
  });

  test('there is a list where the drop-down was', async ({ page }) => {
    const section = page.locator('#settings-slides');

    await expect(section.locator('select')).toHaveCount(0);
    await expect(section.locator('[role="listbox"]')).toBeVisible();
    await expect(section.locator('[role="option"][aria-selected="true"]')).toHaveCount(1);
  });

  test('a click on a row opens it beside the list', async ({ page }) => {

    await page.locator(DIVISIONS).nth(2).click();

    await expect(page.locator(DIVISIONS).nth(2)).toHaveAttribute('aria-selected', 'true');
    await expect(page.locator('#settings-slides article h6')).toHaveText('Three');
    expect(await divisionOrder(page)).toEqual(['One', 'Two', 'Three']);
  });

  test('a row dragged to the top is the first row, and stays it', async ({ page }) => {
    const rows = page.locator(DIVISIONS);

    const firstBox = await rows.nth(0).boundingBox();
    await mouseDrag(page, await middleOf(rows.nth(2)), {
      x: firstBox.x + firstBox.width / 2,
      y: firstBox.y + 2,
    });

    await expect.poll(() => divisionOrder(page)).toEqual(['Three', 'One', 'Two']);
    await expectAtRest(rows);

    await page.reload();
    await expect.poll(() => divisionOrder(page)).toEqual(['Three', 'One', 'Two']);
  });

  test('the open division and the default follow the move', async ({ page }) => {
    const rows = page.locator(DIVISIONS);
    await rows.nth(0).click();

    const lastBox = await rows.nth(2).boundingBox();
    await mouseDrag(page, await middleOf(rows.nth(0)), {
      x: lastBox.x + lastBox.width / 2,
      y: lastBox.y + lastBox.height - 2,
    });

    await expect.poll(() => divisionOrder(page)).toEqual(['Two', 'Three', 'One']);
    await expect(page.locator('#settings-slides article h6')).toHaveText('One');
    await expect(rows.nth(2)).toHaveAttribute('aria-selected', 'true');

    const stored = await storedSettings(page);
    expect(stored.song_slide_settings[stored.default_slide_settings_index].name).toBe('One');
  });

  test('a cancelled drag changes nothing and saves nothing', async ({ page }) => {
    const rows = page.locator(DIVISIONS);
    const before = await storedSettings(page);

    const from = await middleOf(rows.nth(0));
    const lastBox = await rows.nth(2).boundingBox();
    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(from.x, lastBox.y + lastBox.height - 2, { steps: 10 });
    await expect(page.locator('.reorder-marker-below, .reorder-marker-above')).toHaveCount(1);
    // What a browser sends when the system takes the pointer away.
    await page.locator('#slide-settings-list').dispatchEvent('pointercancel');
    await page.mouse.up();

    expect(await divisionOrder(page)).toEqual(['One', 'Two', 'Three']);
    expect(await storedSettings(page)).toEqual(before);
  });

  test('an unnamed division is named by where it now is', async ({ page }) => {
    // Decided in 0006, question 3: a division without a name is called by its
    // position, and keeps being called by its position after it has moved.
    await openSeededSettings(page, (stored) => {
      stored.song_slide_settings[0].name = '';
    });
    const rows = page.locator(DIVISIONS);
    const unnamed = (await divisionOrder(page))[0];

    await rows.nth(0).focus();
    await page.keyboard.press('Alt+ArrowDown');

    await expect.poll(() => divisionOrder(page)).toEqual(['Two', unnamed.replace('1', '2'), 'Three']);
  });
});

test.describe('the slide settings, with the keyboard', () => {
  test.beforeEach(async ({ page }) => {
    await openSeededSettings(page);
  });

  test('Alt and an arrow move the focused row and keep the focus on it', async ({ page }) => {
    const rows = page.locator(DIVISIONS);

    await rows.nth(0).focus();
    await page.keyboard.press('Alt+ArrowDown');

    await expect.poll(() => divisionOrder(page)).toEqual(['Two', 'One', 'Three']);
    await expect(rows.nth(1)).toBeFocused();
  });
});

test.describe('with a finger, on a phone', () => {
  test.use({ hasTouch: true, isMobile: true, viewport: { width: 412, height: 800 } });

  test('a design dragged by its grip moves', async ({ page }) => {
    await openSeededSettings(page);
    const tiles = page.locator(DESIGNS);
    await tiles.nth(1).scrollIntoViewIfNeeded();

    const grip = await middleOf(tiles.nth(1).locator('.presentation-design-grip'));
    const firstBox = await tiles.nth(0).boundingBox();
    // On a phone the tiles are a column: the upper half of the first tile is
    // the gap before it.
    await touchDrag(page, grip, { x: grip.x, y: Math.max(firstBox.y + 10, 5) });

    await expect.poll(() => designOrder(page)).toEqual(['B', 'A', 'C']);
    await expectAtRest(tiles);
  });

  test('a finger on a tile, away from its grip, scrolls the page instead', async ({ page }) => {
    await openSeededSettings(page);
    const tiles = page.locator(DESIGNS);
    await tiles.nth(1).scrollIntoViewIfNeeded();

    const box = await tiles.nth(1).boundingBox();
    const before = await pageScroll(page);
    // From the lower middle of the tile, upwards: a swipe that scrolls down.
    const start = { x: box.x + box.width / 2, y: box.y + box.height * 0.7 };
    await touchDrag(page, start, { x: start.x, y: start.y - 150 });

    await expect.poll(() => pageScroll(page)).toBeGreaterThan(before);
    expect(await designOrder(page)).toEqual(['A', 'B', 'C']);
  });

  test('a design carried to the bottom of the screen scrolls the page and lands at the end', async ({
    page,
  }) => {
    await openSeededSettings(page);
    const tiles = page.locator(DESIGNS);
    await tiles.nth(0).scrollIntoViewIfNeeded();

    const grip = await middleOf(tiles.nth(0).locator('.presentation-design-grip'));
    const viewport = page.viewportSize();
    // Held at the very bottom of the screen until the page has scrolled the
    // rest of the list past.
    await touchDrag(page, grip, { x: grip.x, y: viewport.height - 4 }, { holdAtEnd: 60 });

    await expect.poll(() => designOrder(page)).toEqual(['B', 'C', 'A']);
  });

  test('a slide division dragged by its grip moves', async ({ page }) => {
    await openSeededSettings(page);
    const rows = page.locator(DIVISIONS);
    await rows.nth(0).scrollIntoViewIfNeeded();

    const grip = await middleOf(rows.nth(0).locator('.slide-settings-grip'));
    const lastBox = await rows.nth(2).boundingBox();
    await touchDrag(page, grip, { x: grip.x, y: lastBox.y + lastBox.height - 2 });

    await expect.poll(() => divisionOrder(page)).toEqual(['Two', 'Three', 'One']);
    await expectAtRest(rows);
  });

  test('a finger on a slide division, away from its grip, does not move it', async ({ page }) => {
    await openSeededSettings(page);
    const rows = page.locator(DIVISIONS);
    await rows.nth(0).scrollIntoViewIfNeeded();

    const box = await rows.nth(0).boundingBox();
    const start = { x: box.x + box.width * 0.7, y: box.y + box.height / 2 };
    await touchDrag(page, start, { x: start.x, y: start.y + 120 });

    expect(await divisionOrder(page)).toEqual(['One', 'Two', 'Three']);
  });
});
