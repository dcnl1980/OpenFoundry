import { expect, test } from '@playwright/test';

test('login page renders the sign-in form', async ({ page }) => {
	await page.goto('/auth/login');

	await expect(page).toHaveTitle(/Login/);
	await expect(page.getByRole('heading', { name: /Sign in to OpenFoundry/ })).toBeVisible();
	await expect(page.getByLabel('Email')).toBeVisible();
	await expect(page.getByLabel('Password')).toBeVisible();
	await expect(page.getByRole('button', { name: 'Sign In' })).toBeVisible();
});

test('home page sends unauthenticated users to sign in', async ({ page }) => {
	await page.goto('/');

	await expect(page.getByText('Sign in to access your data platform.')).toBeVisible();
	await page.getByRole('link', { name: 'Sign In' }).click();
	await expect(page).toHaveURL(/\/auth\/login/);
});
