// The desktop program as a client of a server — its real code, run by the
// test harness, with this browser playing the user: logging in through the
// browser (the loopback flow), logging in with a code, and sharing a
// presentation whose picture a viewer elsewhere then sees.

import { test, expect } from '@playwright/test';
import { SERVER, ADMIN, harness, logIn } from './server.js';

test.describe.configure({ mode: 'serial' });

async function loggedIn(request) {
  await expect
    .poll(async () => harness(request, '/server-login-result'), { timeout: 20000 })
    .toBe(`ok ${ADMIN.email}`);
}

test('the desktop logs in through the browser', async ({ page, request }) => {
  const authorize = await harness(request, `/server-login?url=${encodeURIComponent(SERVER)}`);
  expect(authorize).toContain('/authorize?client_id=cantara-desktop');

  // What the system browser would do: open the page, log in, allow.
  await page.goto(authorize);
  await page.getByLabel('E-mail address').fill(ADMIN.email);
  await page.getByLabel('Password').fill(ADMIN.password);
  await page.getByRole('button', { name: 'Log in' }).click();
  await page.getByRole('button', { name: 'Allow' }).click();

  // The browser is sent back to the program's port, which answers.
  await expect(page.getByText('Cantara is connected')).toBeVisible();
  await loggedIn(request);
});

test('a device without a browser logs in with a code', async ({ page, request }) => {
  const shown = await harness(request, `/server-login-code?url=${encodeURIComponent(SERVER)}`);
  const [code, address] = shown.split(' ');
  expect(address).toBe(`${SERVER}/device`);
  expect(code).toMatch(/^[A-Z]{4}-[A-Z]{4}$/);

  // Somebody on a phone: opens the address, logs in, types the code in.
  await logIn(page, '/device');
  await page.getByLabel('Code').fill(code.toLowerCase());
  await page.getByRole('button', { name: /Allow/ }).click();
  await expect(page.getByText('The device is now logged in')).toBeVisible();
  await loggedIn(request);
});

test('a shared presentation shows its picture on another device', async ({ browser, request }) => {
  const link = await harness(request, '/server-share?service=picture');
  expect(link).toMatch(new RegExp(`^${SERVER}/s/`));

  // A viewer who is not logged in anywhere.
  const context = await browser.newContext();
  const page = await context.newPage();
  await page.goto(link);
  await expect(page.locator('img[src^="data:image"]').first()).toBeVisible({ timeout: 30000 });
  await context.close();
});

test('a shared video plays from the server on another device', async ({ browser, request }) => {
  const link = await harness(request, '/server-share?service=video');

  const context = await browser.newContext();
  const page = await context.newPage();
  await page.goto(link);
  // Not fetched whole into the page: the video element streams it from the
  // server, under the path the slide names it by.
  const video = page.locator('video source').first();
  await expect(video).toHaveAttribute('src', /\/api\/v1\/sessions\/[0-9a-f-]+\/media\?path=.*Clip\.mp4/, { timeout: 30000 });
  await context.close();
});
