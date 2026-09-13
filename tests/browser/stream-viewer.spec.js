// The page a phone in the congregation opens.
//
// Stage 4 of `docs/specs/0004-testing-playwright.md`, and the surface where
// most of 0003's damage lived: of the nine defects that came out of building
// the monitor views, six were visible here and nowhere else.
//
// What makes these worth having on top of the Rust markup tests is that they
// run the parts a renderer cannot: the socket, the page's own JavaScript, and
// the browser's layout. The markup tests say the slide is *in* the HTML. These
// say it is on the screen, the right size, and still there a second later.

import { test, expect } from '@playwright/test';
import { show, next, open, ADDRESSES } from './service.js';

test.describe('the address before and during a service', () => {
  test('the address answers with a page', async ({ page }) => {
    // Deliberately the least interesting assertion in the file, and first: if
    // this fails, nothing below means anything, and the reason will be
    // somewhere else entirely.
    const response = await page.goto(ADDRESSES.congregation);

    expect(response.status()).toBe(200);
    await expect(page.locator('#stage')).toBeVisible();
  });

  test('the words of the slide that is up are on the screen', async ({ page, request }) => {
    await show(request, 'words');
    const stage = await open(page, ADDRESSES.congregation);

    await expect(stage).toContainText('Amazing grace how sweet the sound');
  });

  test('pressing next moves the page on', async ({ page, request }) => {
    await show(request, 'song');
    const stage = await open(page, ADDRESSES.congregation);
    await expect(stage).toContainText('Amazing Grace');

    await next(request);

    // The whole point of the stream: the page follows without being reloaded.
    // A test that navigated again here would pass on a page that had no live
    // connection at all.
    await expect(stage).toContainText('Amazing grace how sweet the sound');
  });
});

test.describe('each address shows its own view', () => {
  // Reported from a real service and fixed in 0003: both addresses showed the
  // monitor. It is worth three tests rather than one because there are three
  // ways to get it wrong — the wrong view, the same view twice, and no view.

  test('the congregation is shown a slide, not a stage monitor', async ({ page, request }) => {
    await show(request, 'words');
    const stage = await open(page, ADDRESSES.congregation);

    await expect(stage.locator('.presentation')).toBeVisible();
    await expect(stage.locator('.monitor-view')).toHaveCount(0);
  });

  test('a monitor design arrives as its layout and not as a wall of text', async ({ page, request }) => {
    // The defect that moved the rendering out of the page altogether: the
    // stream's own JavaScript renderer had never heard of monitor layouts, so
    // a stage monitor reached a phone as a list of every line in the song.
    await show(request, 'song');
    const stage = await open(page, ADDRESSES.stage);

    await expect(stage.locator('.monitor-view')).toBeVisible();
    await expect(stage.locator('.presenter-text-panel')).toBeVisible();
  });

  test('the speaker layout shows what is up and what is next', async ({ page, request }) => {
    await show(request, 'song');
    const stage = await open(page, ADDRESSES.speaker);

    await expect(stage.locator('.monitor-speaker-current')).toBeVisible();
    await expect(stage.locator('.monitor-speaker-next')).toBeVisible();
  });

  test('no two addresses show the same thing', async ({ browser, request }) => {
    // The assertion the three above cannot make between them. Each of them
    // would pass on a Cantara that served the *same* correct-looking view at
    // every address, so long as that view happened to satisfy them — and
    // "every address shows the monitor" is exactly that failure.
    await show(request, 'song');

    const drawn = [];
    for (const address of Object.values(ADDRESSES)) {
      // `browser.newPage()` and not the `page` fixture, because this test
      // needs three of them. A review flagged this as broken on the grounds
      // that a raw page does not inherit `use.baseURL`; it does — Playwright
      // Test wraps `browser` so that pages made through it carry the project's
      // context options. Checked rather than assumed: a throwaway spec calling
      // `goto('/')` on such a page landed on http://127.0.0.1:8430/.
      const page = await browser.newPage();
      const stage = await open(page, address);
      drawn.push(await stage.innerHTML());
      await page.close();
    }

    expect(drawn[0]).not.toBe(drawn[1]);
    expect(drawn[1]).not.toBe(drawn[2]);
    expect(drawn[0]).not.toBe(drawn[2]);
  });
});

test.describe('slides that are not words', () => {
  // Reported: "PDF and video in the browser do not work, only the background
  // is shown." A PDF page is drawn by pdf.js into a canvas that no rendering
  // without a browser can fill, and a video's address means nothing off the
  // machine that made it. Both are rewritten on the way out — and both were
  // rewritten wrongly at first, in ways only a browser could show.

  test('a picture slide arrives with its picture', async ({ page, request }) => {
    await show(request, 'picture');
    const stage = await open(page, ADDRESSES.congregation);

    const picture = stage.locator('img').first();
    await expect(picture).toBeVisible();
    // Loaded, not merely present. A broken image is visible too, and it is
    // exactly what a wrong address produces.
    await expect
      .poll(() => picture.evaluate((img) => img.naturalWidth))
      .toBeGreaterThan(0);
  });

  test('a pdf page arrives as a picture rather than an empty canvas', async ({ page, request }) => {
    await show(request, 'pdf page');
    const stage = await open(page, ADDRESSES.congregation);

    // The canvas is the local form; over the network the page travels as an
    // image the server holds. A canvas reaching a viewer *is* the defect.
    await expect(stage.locator('canvas')).toHaveCount(0);
    const page_image = stage.locator('img').first();
    await expect(page_image).toBeVisible();
    await expect
      .poll(() => page_image.evaluate((img) => img.naturalWidth))
      .toBeGreaterThan(0);
  });

  test('a pdf page keeps its proportions', async ({ page, request }) => {
    // Reported with a screenshot: the right page, at its own size, sitting
    // small in the middle of the design's background. `height: 100%` against a
    // parent with no height is nothing, and on the machine it was built on it
    // never showed — pdf.js sizes its own canvas.
    await show(request, 'pdf page');
    const stage = await open(page, ADDRESSES.congregation);

    const fitted = await stage.locator('img').first().evaluate((img) => {
      const style = getComputedStyle(img);
      return {
        fit: style.objectFit,
        // Filling its cell rather than sitting at its natural size.
        wide: img.getBoundingClientRect().width,
        cell: img.parentElement.getBoundingClientRect().width,
      };
    });

    expect(fitted.fit).toBe('contain');
    expect(fitted.wide).toBeGreaterThan(fitted.cell * 0.5);
  });

  test('a video slide arrives with a source the viewer can ask for', async ({ page, request }) => {
    await show(request, 'video');
    const stage = await open(page, ADDRESSES.congregation);

    const source = await stage
      .locator('video source')
      .first()
      .getAttribute('src');

    // Cantara's own handler is a scheme that means nothing off this machine,
    // and a loopback address on a phone is the phone. Either reaching a viewer
    // is the defect that showed as an empty rectangle over the background.
    expect(source).not.toContain('cantara-video');
    expect(source).not.toContain('127.0.0.1');
    expect(source).toMatch(/^video\//);
  });
});

test.describe('a slide is fitted to the screen it is on', () => {
  // A slide's type is set in points, so a slide put straight into a smaller box
  // overflows it. The fitting is measured in the browser — which is why it can
  // only be tested in one. Three separate attempts at this failed in 0003: a
  // CSS `scale()` the web view silently dropped, a default that hid everything
  // when measuring did not run, and a refresh that undid it after one second.

  test('the slide is inside the window rather than overflowing it', async ({ page, request }) => {
    await show(request, 'song');
    const stage = await open(page, ADDRESSES.speaker);

    const frame = stage.locator('.monitor-slide-frame').first();
    await expect(frame).toBeVisible();

    const fits = await frame.evaluate((element) => {
      const box = element.getBoundingClientRect();
      const inner = element.querySelector('.monitor-slide-stage').getBoundingClientRect();
      return {
        // Some room, not none: a stage of zero size is "inside the window" too,
        // and that is the failure where nothing was displayed at all.
        drawn: inner.width > 1 && inner.height > 1,
        withinWidth: inner.width <= box.width + 1,
        withinHeight: inner.height <= box.height + 1,
      };
    });

    expect(fits.drawn, 'the slide was scaled to nothing').toBeTruthy();
    expect(fits.withinWidth, 'the slide is wider than its box').toBeTruthy();
    expect(fits.withinHeight, 'the slide is taller than its box').toBeTruthy();
  });

  test('the slide is still fitted a moment later', async ({ page, request }) => {
    // "The scaling is right for the first second, and then it jumps back to
    // the wrong content." The widget refresh rebuilt the whole stage every
    // second, losing the size each frame had been fitted to. Nothing but
    // waiting can catch this.
    await show(request, 'song');
    const stage = await open(page, ADDRESSES.speaker);

    const sizeNow = () =>
      stage
        .locator('.monitor-slide-stage')
        .first()
        .evaluate((element) => element.getBoundingClientRect().width);

    const first = await sizeNow();
    expect(first).toBeGreaterThan(1);

    await page.waitForTimeout(2500);

    expect(Math.abs((await sizeNow()) - first)).toBeLessThan(2);
  });

  test('the slide is refitted when the window changes size', async ({ page, request }) => {
    await show(request, 'song');
    const stage = await open(page, ADDRESSES.speaker);
    const frame = stage.locator('.monitor-slide-stage').first();

    await page.setViewportSize({ width: 1280, height: 800 });
    await expect.poll(() => frame.evaluate((e) => e.getBoundingClientRect().width))
      .toBeGreaterThan(1);
    const wide = await frame.evaluate((e) => e.getBoundingClientRect().width);

    await page.setViewportSize({ width: 480, height: 800 });

    // Smaller, and still drawn. A phone turned sideways mid-service is the
    // ordinary case here, not an edge one.
    await expect
      .poll(() => frame.evaluate((e) => e.getBoundingClientRect().width))
      .toBeLessThan(wide);
    expect(await frame.evaluate((e) => e.getBoundingClientRect().width)).toBeGreaterThan(1);
  });
});

test.describe('the widgets on a stage monitor', () => {
  test('a clock and a timer are drawn in the corners they were given', async ({ page, request }) => {
    await show(request, 'song', { widgets: true });
    const stage = await open(page, ADDRESSES.stage);

    await expect(stage.locator('.monitor-widget-top-right .monitor-clock')).toBeVisible();
    await expect(stage.locator('.monitor-widget-bottom-left .monitor-timer')).toBeVisible();
  });

  test('the time keeps moving without the slide being rebuilt', async ({ page, request }) => {
    // Reported: "on the streamed monitor the times are only updated on a slide
    // change, not continuously." Fixed by re-rendering on a timer — which then
    // caused the jumping above, because the fix rebuilt the whole stage. Both
    // halves have to hold at once, so both are asserted here.
    await show(request, 'song', { widgets: true });
    const stage = await open(page, ADDRESSES.stage);

    const timer = stage.locator('.monitor-timer-value');
    await expect(timer).toBeVisible();

    const before = await timer.textContent();
    // Something that would be lost if the stage were rebuilt rather than
    // patched, so that "the clock moved" and "the page was thrown away and
    // redrawn" can be told apart.
    await stage.evaluate((element) => {
      element.querySelector('.presenter-text-panel').dataset.survived = 'yes';
    });

    await expect.poll(() => timer.textContent(), { timeout: 8000 }).not.toBe(before);

    await expect(stage.locator('.presenter-text-panel[data-survived="yes"]')).toHaveCount(
      1,
      { message: 'the whole stage was rebuilt to move a clock on by a second' },
    );
  });
});
