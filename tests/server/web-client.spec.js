// The web build a server serves: its library is the server's, and a song
// edited in it goes back to the server.

import { test, expect } from '@playwright/test';
import { seedLibrary, logIn } from './server.js';

test.beforeAll(async () => {
  await seedLibrary();
});

test('somebody not logged in is sent to the login page', async ({ page }) => {
  await page.goto('/');
  await expect(page).toHaveURL(/\/login/);
  await expect(page.getByRole('button', { name: 'Log in' })).toBeVisible();
});

test('the library on the server is the song list', async ({ page }) => {
  await logIn(page);
  await expect(page.getByText('Alas, and Did My Savior Bleed').first()).toBeVisible();
  await expect(page.getByText('Weiß ich den Weg auch nicht').first()).toBeVisible();
});

test('a song carries the identity the server gave it', async ({ page }) => {
  await logIn(page);
  await page.getByText('Alas, and Did My Savior Bleed').first().click();
  // The address names the song by the uuid the server wrote into it.
  await expect(page).toHaveURL(/\/detail\/[0-9a-f]{8}-[0-9a-f]{4}-7/);
});

test('a song edited here becomes a new version on the server', async ({ page }) => {
  const { api, tenant, library } = await seedLibrary();
  const index = async () => (await (await api.get(`/api/v1/tenants/${tenant}/libraries/${library}/index`)).json()).entries;
  const before = (await index()).find((entry) => entry.file_name === 'Sei nicht stolz auf das, was du bist.song.yml');
  expect(before).toBeTruthy();

  await logIn(page);
  await page.getByText('Sei nicht stolz auf das, was du bist').first().click();
  await page.getByRole('button', { name: 'Edit', exact: true }).click();
  const editor = page.locator('.detail-editor-textarea');
  await expect(editor).toBeVisible();
  const text = await editor.inputValue();
  await editor.fill(`${text.trimEnd()}\n# edited in the browser ${Date.now()}\n`);
  await page.getByRole('button', { name: 'Save', exact: true }).click();

  // It goes up within one round of the queue (fifteen seconds at most).
  await expect
    .poll(async () => (await index()).find((entry) => entry.lineage === before.lineage).version, { timeout: 30000 })
    .not.toBe(before.version);
});
