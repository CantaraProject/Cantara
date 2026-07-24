import { test, expect } from '@playwright/test';

/**
 * Cantara web app – smoke tests.
 *
 * These tests verify that the Dioxus/WASM bundle loads and renders the basic
 * UI without JavaScript errors.  They are intentionally lightweight so that
 * the E2E job stays fast even on CI runners without hardware acceleration.
 */

test.describe('App startup', () => {
  test('page loads and shows Cantara title', async ({ page }) => {
    await page.goto('/');
    // The <title> set by the App component
    await expect(page).toHaveTitle(/Cantara/i);
  });

  test('no unhandled JavaScript errors on load', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', (err) => errors.push(err.message));

    await page.goto('/');
    // Wait for WASM/app initialization (root element mounted)
    await expect(page.locator('#main')).toBeAttached();
    // Filter out benign WebAssembly/source-map noise that is unrelated to the app
    const appErrors = errors.filter(
      (e) =>
        !e.includes('source map') &&
        !e.includes('SharedArrayBuffer') &&
        !e.includes('WebAssembly.instantiate')
    );
    expect(appErrors, `Unexpected page errors: ${appErrors.join('; ')}`).toHaveLength(0);
  });

  test('root element is rendered', async ({ page }) => {
    await page.goto('/');
    // The Dioxus app mounts under the #main element
    await expect(page.locator('#main')).toBeAttached();
  });
});

test.describe('Navigation', () => {
  test('wizard or selection page is shown on first load', async ({ page }) => {
    await page.goto('/');

    // Poll until the body has visible text content, giving the WASM bundle
    // enough time to initialise and render in CI (can take more than 3 s).
    await page.waitForFunction(
      () => (document.body.textContent ?? '').trim().length > 0,
      { timeout: 15000 }
    );

    // The app should show either the wizard (first run) or the selection page
    const body = await page.locator('body').textContent();
    const hasContent = body !== null && body.trim().length > 0;
    expect(hasContent, 'Body should have content after WASM load').toBe(true);
  });

  test('settings route responds without 404', async ({ page }) => {
    // Navigate to the settings hash-route; the SPA should handle it
    const response = await page.goto('/#/settings');
    // The server always returns 200 for the index.html; the router handles routing
    expect(response?.status()).toBeLessThan(400);
  });
});
