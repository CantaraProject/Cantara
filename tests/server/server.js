// What the server-mode tests share: logging in, a library to look at, and
// the harness that plays the desktop program.

import { request as playwrightRequest, expect } from '@playwright/test';
import { readdirSync, readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

export const SERVER = 'http://127.0.0.1:8095';
export const CONTROL = 'http://127.0.0.1:8431';
export const ADMIN = { email: 'admin@example.org', password: 'admin-password' };

// Playwright runs from the repository's root.
const root = resolve('.');

/// The API, logged in as the administrator.
export async function adminApi() {
  const api = await playwrightRequest.newContext({ baseURL: SERVER });
  const login = await api.post('/api/v1/auth/login', { data: ADMIN });
  expect(login.ok()).toBeTruthy();
  return api;
}

/// A library called "Lieder" with the files of `testfiles/` in it. Storing a
/// file twice changes nothing, so every test may ask for it.
export async function seedLibrary() {
  const api = await adminApi();
  const me = await (await api.get('/api/v1/me')).json();
  const tenant = me.tenants[0].tenant.uuid;
  let libraries = await (await api.get(`/api/v1/tenants/${tenant}/libraries`)).json();
  let library = libraries.find((candidate) => candidate.name === 'Lieder');
  if (!library) {
    library = await (await api.post(`/api/v1/tenants/${tenant}/libraries`, { data: { name: 'Lieder' } })).json();
  }
  const folder = join(root, 'testfiles');
  for (const name of readdirSync(folder)) {
    const response = await api.post(`/api/v1/tenants/${tenant}/libraries/${library.uuid}/items`, {
      multipart: { file: { name, mimeType: 'application/octet-stream', buffer: readFileSync(join(folder, name)) } },
    });
    expect(response.ok(), `${name}: ${response.status()}`).toBeTruthy();
  }
  return { api, tenant, library: library.uuid };
}

/// Logs the page in through the server's own login page.
export async function logIn(page, next = '/') {
  await page.goto(`/login?next=${encodeURIComponent(next)}`);
  await page.getByLabel('E-mail address').fill(ADMIN.email);
  await page.getByLabel('Password').fill(ADMIN.password);
  await page.getByRole('button', { name: 'Log in' }).click();
}

/// Asks the desktop program's harness to do something, and returns what it
/// said.
export async function harness(request, path) {
  const response = await request.get(`${CONTROL}${path}`);
  const text = await response.text();
  expect(response.ok(), text).toBeTruthy();
  return text;
}
