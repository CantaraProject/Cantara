// Administering the organisation from the web build, and the way in that an
// invitation is.

import { test, expect } from '@playwright/test';
import { logIn } from './server.js';

test('an invitation lets a new person in', async ({ page, browser }) => {
  await logIn(page, '/admin');
  await expect(page.getByRole('heading', { name: 'Members' })).toBeVisible();

  const email = `singer-${Date.now()}@example.org`;
  await page.getByPlaceholder('E-mail address').fill(email);
  await page.getByRole('button', { name: 'Invite' }).click();
  const link = await page.locator('code', { hasText: '/invite/' }).textContent();
  expect(link).toContain('/invite/');

  // The invited person, in a browser of their own.
  const context = await browser.newContext();
  const invited = await context.newPage();
  await invited.goto(link);
  await invited.getByRole('link', { name: 'Create an account' }).click();
  await invited.getByLabel('Name').fill('Neue Sängerin');
  await invited.getByLabel('Password').fill('a long enough password');
  await invited.getByRole('button', { name: 'Create account' }).click();
  await expect(invited).not.toHaveURL(/\/register/);

  const me = await (await invited.request.get('/api/v1/me')).json();
  expect(me.user.email).toBe(email);
  expect(me.tenants).toHaveLength(1);
  await context.close();

  // And the administrator sees the new member.
  await page.reload();
  await expect(page.getByText(email).first()).toBeVisible();
});
