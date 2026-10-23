// The map of the screens, and dropping a window onto one.
//
// `docs/specs/0007-screen-map.md`, decision D8. The arithmetic — the bounding
// box, the transform, which screen a point is inside — is covered by the unit
// tests in `logic::screen_map`, and which chip is drawn where by the markup
// tests beside `components::screen_map`. What is left for a browser is
// everything those cannot see: that the proportions survive the trip through
// CSS, that a real pointer and a real finger reach the drop at all, that the
// map and the drop-down beside it agree, and that a choice is still there
// after a reload.
//
// A browser enumerates no monitors, which is why none of this could be tested
// before. The web build is made with `--features test-harness`, which turns
// the `screens` code on and hands it a fabricated set of screens — a primary
// laptop, a smaller beamer to its right, and a stage monitor above the laptop.
// See `logic::screens::harness_monitors`, and `playwright.config.js` for where
// the feature is passed.

import { test, expect } from '@playwright/test';
import { WEB } from '../../playwright.config.js';

test.use({ baseURL: WEB });

/// Where the web build keeps its settings. `logic::settings::SETTINGS_KEY`.
const SETTINGS_KEY = 'cantara-settings';

/// The screens the fabricated set contains, by the name the map shows and the
/// key the settings store. `logic::screens::{harness_monitors, screen_key}`.
const LAPTOP = { name: 'Laptop', key: 'Laptop#0' };
const BEAMER = { name: 'Beamer', key: 'Beamer#1' };
const STAGE = { name: 'Bühne', key: 'Bühne#2' };

const MAP = '#screen-map';
const SCREENS = '.screen-map-screen';

/// Opens the screen settings with two views that both want a screen and
/// neither of which has chosen one.
///
/// Both start as "automatic", so every assertion below about a chosen screen
/// is about something this test did. The settings are seeded through local
/// storage, which is where the web build keeps them — the same way in as the
/// reorder tests use, and the real one.
async function openSeededSettings(page, change = () => {}) {
  await page.goto('/Cantara/settings');
  await expect(page.locator(MAP)).toBeVisible();

  // The settings are only in storage once the program has written them, and
  // it writes them when something changes. Duplicating a presentation design
  // is the change these tests use for that, because it is the one furthest
  // from anything the map reads — the same trigger `settings-reorder.spec.js`
  // uses, for the same reason.
  await page.locator('.presentation-design-actions button', { hasText: 'Duplicate' }).click();
  await page.waitForFunction((key) => localStorage.getItem(key) !== null, SETTINGS_KEY);

  const stored = JSON.parse(await page.evaluate((key) => localStorage.getItem(key), SETTINGS_KEY));
  const view = stored.views[0];
  stored.views = [
    { ...view, name: 'Wall', enabled: true, output: { Screen: { monitor_name: null } } },
    { ...view, id: globalThis.crypto.randomUUID(), name: 'Band', enabled: true, output: { Screen: { monitor_name: null } } },
  ];
  stored.reference_view_index = 0;
  change(stored);

  await page.evaluate(
    ([key, value]) => localStorage.setItem(key, value),
    [SETTINGS_KEY, JSON.stringify(stored)],
  );
  await page.reload();
  await expect(page.locator(MAP)).toBeVisible();
  await expect(page.locator(SCREENS)).toHaveCount(3);
  await showMap(page);
}

/// Puts the whole map in the middle of the window.
///
/// A pointer can only press what is on screen, and the settings are longer
/// than any window. Centred rather than scrolled to the top of the section,
/// because the holding area is the *bottom* of the map and it is half of what
/// these tests aim at — with the section at the top of the window it sat just
/// below the fold, and a drop at a coordinate the window does not contain goes
/// nowhere at all.
async function showMap(page) {
  await page.locator(MAP).evaluate((map) => map.scrollIntoView({ block: 'center' }));
}

/// The settings document as it is stored right now.
async function storedSettings(page) {
  return JSON.parse(await page.evaluate((key) => localStorage.getItem(key), SETTINGS_KEY));
}

/// The screen a view is set to, as the settings store it. `null` is
/// "automatic".
async function chosenScreen(page, name) {
  const stored = await storedSettings(page);
  return stored.views.find((view) => view.name === name).output.Screen.monitor_name;
}

/// The rectangle a screen is drawn as, by the name on it.
function screenNamed(page, name) {
  return page.locator(SCREENS).filter({ has: page.getByText(name, { exact: true }) });
}

/// The chip for a window, by the label on it.
function chipFor(page, label) {
  return page.locator('.screen-map-chip').filter({ hasText: label });
}

/// The name of the screen a chip is drawn on, and whether it is drawn dashed —
/// which is the map saying "nobody chose this, but this is where it will go".
async function chipSitsOn(page, label) {
  return page.locator(MAP).evaluate((map, text) => {
    const chip = [...map.querySelectorAll('.screen-map-chip')].find(
      (node) => node.textContent.trim() === text,
    );
    if (!chip) {
      return { screen: null, inferred: null };
    }
    const screen = chip.closest('.screen-map-screen');
    return {
      screen: screen ? screen.querySelector('.screen-map-screen-name').textContent.trim() : null,
      inferred: chip.classList.contains('screen-map-chip-inferred'),
    };
  }, label);
}

/// The middle of an element, on screen.
async function middleOf(locator) {
  const box = await locator.boundingBox();
  return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
}

/// Drags with the mouse from one point to another, in steps, the way a hand
/// does. A single jump would be one `pointermove`, which is not a drag anyone
/// makes and would not exercise the movement threshold.
async function mouseDrag(page, from, to) {
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(from.x + 3, from.y + 3, { steps: 2 });
  await page.mouse.move(to.x, to.y, { steps: 12 });
  await page.mouse.up();
}

/// Drags with a finger, through the DevTools protocol.
///
/// Playwright's own `touchscreen` only taps. `Input.dispatchTouchEvent` goes
/// through the browser's input pipeline like a real finger, so it produces
/// pointer events of type `touch` and it honours `touch-action`. Chromium
/// only — which is the only browser this suite runs (spec 0004).
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

test.describe('the map', () => {
  // Wide enough for the map to be drawn at a size a pointer can aim inside,
  // and tall enough for the whole screen section to be on screen at once.
  test.use({ viewport: { width: 1600, height: 1000 } });
  test.beforeEach(async ({ page }) => {
    await openSeededSettings(page);
  });

  /// The point of the whole feature (D-nothing, the request itself): the
  /// screens are drawn in the proportions and the arrangement the platform
  /// reports. The beamer is 1280×800 against the laptop's 1920×1080, so it is
  /// drawn both narrower and shorter; the stage monitor is above the laptop,
  /// which is a negative origin and the thing a naive transform gets wrong.
  test('the screens are drawn as they are arranged', async ({ page }) => {
    const laptop = await screenNamed(page, LAPTOP.name).boundingBox();
    const beamer = await screenNamed(page, BEAMER.name).boundingBox();
    const stage = await screenNamed(page, STAGE.name).boundingBox();

    expect(beamer.width).toBeLessThan(laptop.width);
    expect(beamer.height).toBeLessThan(laptop.height);
    expect(beamer.x).toBeGreaterThan(laptop.x + laptop.width - 2);
    expect(stage.y).toBeLessThan(laptop.y - 2);
    // Same screen, same size: the arrangement is one scale, not two.
    expect(Math.abs(stage.width - laptop.width)).toBeLessThan(2);
  });

  /// Neither view has chosen a screen, and both are still drawn somewhere —
  /// dashed, on the screen they will actually open on (D4). A holding area
  /// full of unplaced chips would answer nothing.
  test('a window that has chosen nothing is drawn where it will land', async ({ page }) => {
    const wall = await chipSitsOn(page, 'Wall');

    expect(wall.inferred).toBe(true);
    expect(wall.screen).not.toBe(null);
    expect(await chosenScreen(page, 'Wall')).toBe(null);
  });

  /// A drop writes the screen, and the chip stops being dashed: this one was
  /// chosen. The laptop is the primary screen, which is the one a projection
  /// is never given automatically, so there is no reading of this that the
  /// arithmetic could have produced by itself.
  test('a window dragged onto a screen is set to it', async ({ page }) => {
    await mouseDrag(page, await middleOf(chipFor(page, 'Wall')), await middleOf(screenNamed(page, LAPTOP.name)));

    await expect.poll(() => chosenScreen(page, 'Wall')).toBe(LAPTOP.key);
    await expect.poll(async () => (await chipSitsOn(page, 'Wall')).screen).toBe(LAPTOP.name);
    expect((await chipSitsOn(page, 'Wall')).inferred).toBe(false);
  });

  /// The chip has to stay in sight on its way. It is still its screen's child
  /// while it is carried, moved only by a transform, and a screen clips what
  /// overflows it — so it used to disappear at the edge of the screen it came
  /// from, which is exactly when the user is looking for it. Found by Copilot
  /// on PR 106.
  test('a carried window stays visible on its way to another screen', async ({ page }) => {
    const from = await middleOf(chipFor(page, 'Wall'));
    const to = await middleOf(screenNamed(page, LAPTOP.name));

    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(from.x + 3, from.y + 3, { steps: 2 });
    await page.mouse.move(to.x, to.y, { steps: 12 });

    const carried = await page.locator(MAP).evaluate((map) => {
      const chip = map.querySelector('.screen-map-chip-carried');
      if (!chip) {
        return null;
      }
      const screen = chip.closest('.screen-map-screen');
      const c = chip.getBoundingClientRect();
      const s = screen.getBoundingClientRect();
      return {
        outsideItsScreen: c.right < s.left || c.left > s.right || c.bottom < s.top || c.top > s.bottom,
        overflow: getComputedStyle(screen).overflow,
      };
    });
    await page.mouse.up();

    expect(carried).not.toBe(null);
    expect(carried.outsideItsScreen).toBe(true);
    expect(carried.overflow).toBe('visible');
  });

  /// And back again. The holding area is what a chip is dragged *to* in order
  /// to stop having been chosen (D4) — it is not where unplaced chips live.
  test('a window dragged to the holding area is automatic again', async ({ page }) => {
    await mouseDrag(page, await middleOf(chipFor(page, 'Wall')), await middleOf(screenNamed(page, LAPTOP.name)));
    await expect.poll(() => chosenScreen(page, 'Wall')).toBe(LAPTOP.key);

    await mouseDrag(
      page,
      await middleOf(chipFor(page, 'Wall')),
      await middleOf(page.locator('.screen-map-holding-label')),
    );

    await expect.poll(() => chosenScreen(page, 'Wall')).toBe(null);
    await expect.poll(async () => (await chipSitsOn(page, 'Wall')).inferred).toBe(true);
  });

  /// A finger, which is the other half of why the drag is written on pointer
  /// events at all. A chip is small and has no room for a grip, so it claims
  /// the whole of itself — `touch-action: none`.
  test('a finger can drag a window onto a screen', async ({ page }) => {
    await touchDrag(page, await middleOf(chipFor(page, 'Band')), await middleOf(screenNamed(page, LAPTOP.name)));

    await expect.poll(() => chosenScreen(page, 'Band')).toBe(LAPTOP.key);
  });

  /// Where the main window sits is the window manager's business (D3). It is
  /// on the map to orient by, and dragging it does nothing at all.
  test('the main window cannot be dragged off its screen', async ({ page }) => {
    const before = await storedSettings(page);

    await mouseDrag(page, await middleOf(chipFor(page, 'Cantara')), await middleOf(screenNamed(page, BEAMER.name)));

    await expect.poll(async () => (await storedSettings(page)).views).toEqual(before.views);
  });

  /// The map is a picture of what the drop-downs under it say, and there is
  /// one write path behind both (D1, D9). Setting the projection's screen in
  /// the drop-down moves the chip on the map.
  test('the drop-down and the map say the same thing', async ({ page }) => {
    // By the value rather than the label: the value *is* the screen key the
    // settings store, so this is the drop-down saying the same thing the map
    // would have said, in the same words.
    await page
      .locator('select')
      .filter({ has: page.locator(`option[value="${BEAMER.key}"]`) })
      .first()
      .selectOption(BEAMER.key);

    await expect.poll(() => chosenScreen(page, 'Wall')).toBe(BEAMER.key);
    await expect.poll(async () => (await chipSitsOn(page, 'Wall')).screen).toBe(BEAMER.name);
  });

  /// A choice that is not there after a reload was not stored, and a settings
  /// page that forgets is worse than one that cannot be dragged.
  test('the choice is still there after a reload', async ({ page }) => {
    await mouseDrag(page, await middleOf(chipFor(page, 'Wall')), await middleOf(screenNamed(page, LAPTOP.name)));
    await expect.poll(() => chosenScreen(page, 'Wall')).toBe(LAPTOP.key);

    await page.reload();
    await expect(page.locator(SCREENS)).toHaveCount(3);
    await showMap(page);

    expect(await chosenScreen(page, 'Wall')).toBe(LAPTOP.key);
    await expect.poll(async () => (await chipSitsOn(page, 'Wall')).screen).toBe(LAPTOP.name);
  });
});

test.describe('a screen that is not plugged in', () => {
  test.use({ viewport: { width: 1600, height: 1000 } });

  /// D10: a screen that is gone is not drawn — it is not there — and its
  /// windows appear on whatever screen they now fall back to, dashed, with a
  /// line saying so. What is *stored* is untouched, so plugging the beamer
  /// back in restores the arrangement instead of silently leaving the
  /// projection on the laptop.
  test('its windows fall back, and nothing stored is lost', async ({ page }) => {
    await openSeededSettings(page, (stored) => {
      stored.views[0].output = { Screen: { monitor_name: 'Beamer#7' } };
    });

    await expect(page.locator('.screen-map-note')).toBeVisible();

    const wall = await chipSitsOn(page, 'Wall');
    expect(wall.screen).not.toBe(null);
    expect(wall.inferred).toBe(true);
    expect(await chosenScreen(page, 'Wall')).toBe('Beamer#7');
  });
});
