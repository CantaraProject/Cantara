// Driving the service the browser tests are looking at.
//
// Every test here needs the same first step — put a known service on the
// stream — and getting that subtly wrong in each of them is how a suite comes
// to have tests that pass for reasons nobody intended. See
// `docs/specs/0004-testing-playwright.md`.

import { expect } from '@playwright/test';
import { CONTROL } from '../../playwright.config.js';

/// Asks the harness for something, and insists it worked.
///
/// The failure mode this guards against is the quiet one: a control request
/// that 400s because a service was renamed leaves the *previous* service on
/// the stream, so the test carries on and asserts against whatever the last
/// one left behind. Then it either passes wrongly or fails somewhere far away
/// from the mistake.
async function control(request, path) {
  const response = await request.get(`${CONTROL}${path}`);
  expect(
    response.ok(),
    `the harness refused ${path}: ${await response.text()}`,
  ).toBeTruthy();
  return (await response.text()).trim();
}

/// Puts a named service on the stream.
///
/// The names are `logic::fixtures::every_slide_kind`'s, so a test here and a
/// Rust markup test are looking at the same service.
export async function show(request, service, { widgets = false } = {}) {
  const query = `service=${encodeURIComponent(service)}${widgets ? '&widgets=yes' : ''}`;
  return control(request, `/show?${query}`);
}

/// Moves the service on one slide, as pressing "next" does.
export async function next(request) {
  return control(request, '/next');
}

/// The three addresses the harness serves, and what each is set to show.
///
/// Named here rather than written into each test, because what makes them
/// worth testing is that they *differ* — and a test that hard-codes `/stage`
/// without saying what is at it reads as an arbitrary URL.
export const ADDRESSES = {
  /// What the congregation sees: the ordinary audience design.
  congregation: '/',
  /// A stage monitor showing the service as a list.
  stage: '/stage',
  /// A stage monitor showing the current slide and the next.
  speaker: '/speaker',
};

/// Opens an address and waits until the page has drawn a service.
///
/// The page connects, asks for the state and draws it, so everything arrives a
/// moment after the navigation does. Waiting for the placeholder to be gone is
/// what makes the rest of a test an assertion about the service rather than a
/// race against it.
export async function open(page, address) {
  await page.goto(address);
  await expect(page.locator('#stage')).not.toHaveText('…');
  return page.locator('#stage');
}
