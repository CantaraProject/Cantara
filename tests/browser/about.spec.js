// What the program says about itself, and the door that leads there.
//
// `docs/specs/0005-add-info-page.md` builds this page for two reasons, and
// only one of them is politeness. Cantara is under the **AGPL** and the web
// build serves it to people over a network, which is the licence's whole point
// of difference from the GPL: those people have to be able to get the source.
// `COPYING` is in the repository and the README is on GitHub — neither is
// handed to somebody who opens the demo. This page is.
//
// So the test that matters most here is the dullest one: that a visitor to the
// web build can reach the licence and the source from inside the program.
//
// The unit tests in `logic::about` cover the language fallback and the
// copyright, and the markup tests in `components::about_components` cover what
// the page renders. What is left for a browser is the part neither can see:
// that the settings actually lead here, and that the way back works.

import { test, expect } from '@playwright/test';
import { WEB } from '../../playwright.config.js';

test.use({ baseURL: WEB });

/// Opens the settings page.
///
/// By address rather than by clicking the icon in the library's bar: which
/// icon sits where is the library view's business, and a test that went
/// through it would fail there for reasons that have nothing to do with this
/// page. What these tests are about begins at the bottom of the settings.
async function openSettings(page) {
  await page.goto('/Cantara/settings');
  await expect(page.locator('#settings-about-button')).toBeVisible();
}

test.describe('reaching the about page', () => {
  test('the settings offer a way to it, at the bottom', async ({ page }) => {
    await openSettings(page);

    await page.locator('#settings-about-button').click();

    await expect(page).toHaveURL(/\/about$/);
    await expect(page.locator('h1')).toHaveText('Cantara');
  });

  test('it is an address of its own, so it can be linked to', async ({ page }) => {
    // The point of the page being a route rather than a panel: the source
    // offer is exactly the kind of link people send each other, and it has to
    // work for somebody who has never opened the settings.
    await page.goto('/Cantara/about');

    await expect(page.locator('h1')).toHaveText('Cantara');
  });
});

test.describe('what the page says', () => {
  test('it names the version the visitor is running', async ({ page }) => {
    await page.goto('/Cantara/about');

    // Not the number itself — that changes with every release, and a test
    // asserting `0.3.0` would be a test somebody edits without reading. What
    // must be true is that *a* version is shown.
    await expect(page.locator('.about-version')).toHaveText(/\d+\.\d+\.\d+/);
  });

  test('it carries a copyright running to a year', async ({ page }) => {
    await page.goto('/Cantara/about');

    await expect(page.locator('.about-copyright')).toContainText('2015');
    await expect(page.locator('.about-copyright')).toContainText('Jan Martin Reckel');
  });

  test('it names the licence and offers the source', async ({ page }) => {
    // The reason the page exists. If this fails, the web build is serving
    // Cantara to people without telling them what they are owed.
    await page.goto('/Cantara/about');

    const prose = page.locator('.about-text');
    await expect(prose).toContainText('Affero');
    await expect(
      prose.getByRole('link', { name: /github\.com\/CantaraProject\/cantara/ }),
    ).toBeVisible();
  });

  test('the prose arrives as markup rather than as Markdown source', async ({ page }) => {
    await page.goto('/Cantara/about');

    const prose = page.locator('.about-text');
    await expect(prose.locator('h2').first()).toBeVisible();
    await expect(prose).not.toContainText('## ');
  });
});

test.describe('the language the reader gets', () => {
  // The one part of this page with real logic behind it, and the only place
  // the whole chain can be seen at once: the browser's language reaches
  // `sys_locale`, which reaches `logic::about::text_for`, which picks one of
  // the files `build.rs` compiled in. The unit tests cover the picking; this
  // covers that the browser's answer arrives there at all.

  test.describe('a German browser', () => {
    test.use({ locale: 'de-DE' });

    test('is given the German text, region and all', async ({ page }) => {
      await page.goto('/Cantara/about');

      await expect(page.locator('.about-text')).toContainText('Lizenz');
      await expect(page.locator('hgroup p')).toHaveText(/Gemeinden/);
    });
  });

  test.describe('a browser in a language nobody has written for', () => {
    test.use({ locale: 'pt-BR' });

    test('is given English rather than an empty page', async ({ page }) => {
      await page.goto('/Cantara/about');

      await expect(page.locator('.about-text')).toContainText('Licence');
      await expect(page.locator('.about-text')).not.toHaveText('');
    });
  });
});

test.describe('the way back', () => {
  test('it returns to wherever the reader came from', async ({ page }) => {
    // Not "back to the settings": the page can be arrived at from anywhere,
    // so the button returns along the way in rather than to a fixed place.
    await openSettings(page);
    await page.locator('#settings-about-button').click();
    await expect(page).toHaveURL(/\/about$/);

    await page.locator('footer button').click();

    await expect(page).toHaveURL(/\/settings$/);
  });

  test('a visitor who arrived by link is not left without one', async ({ page }) => {
    // With nothing behind it, going back would take a web visitor out of
    // Cantara altogether. The settings are where the only door into this page
    // is, so that is where it leads instead.
    await page.goto('/Cantara/about');

    await page.locator('footer button').click();

    await expect(page).toHaveURL(/\/settings$/);
  });
});
