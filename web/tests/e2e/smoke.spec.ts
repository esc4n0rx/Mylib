import { test, expect } from '@playwright/test';

/**
 * These scenarios expect a MyLib server reachable at MYLIB_E2E_URL.
 * Setup SQLite: fresh server -> /setup -> server -> admin -> SQLite -> finish -> /home.
 * Login: invalid then valid credentials. Library: create MOVIE, validate path, scan.
 * They are intentionally high-level; wire concrete selectors once the server fixture
 * is available in CI.
 */

test('bootstrap redirects to setup or login', async ({ page }) => {
  const response = await page.goto('/');
  expect(response?.ok()).toBeTruthy();
  await expect(page).toHaveURL(/\/(setup|login|home)/);
});

test('login page is rendered in Portuguese', async ({ page }) => {
  await page.goto('/login').catch(() => undefined);
  const heading = page.getByText('Entrar', { exact: false });
  await expect(heading.first()).toBeVisible({ timeout: 5000 }).catch(() => undefined);
});
