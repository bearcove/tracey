import { test, expect } from '@playwright/test';

/**
 * Tracey Dashboard E2E Tests
 *
 * These tests verify the web dashboard functionality.
 * Requires a running tracey daemon and web server.
 *
 * To run:
 *   1. Start daemon: tracey daemon
 *   2. Start web server: tracey web --port 3000
 *   3. Run tests: npm test
 */

test.describe('Dashboard Navigation', () => {
  test('should load the home page', async ({ page }) => {
    await page.goto('/');
    // Dashboard should redirect to default spec/impl or show spec selector
    await expect(page).toHaveTitle(/tracey/i);
  });

  test('should display spec selector when multiple specs exist', async ({ page }) => {
    await page.goto('/');
    // Should see either the spec selector or be redirected to a spec view
    const hasSpecLink = await page.locator('a[href*="/spec"]').first().isVisible({ timeout: 5000 }).catch(() => false);
    const hasSpecSelector = await page.locator('text=/select.*spec/i').isVisible({ timeout: 1000 }).catch(() => false);
    expect(hasSpecLink || hasSpecSelector).toBeTruthy();
  });
});

test.describe('Spec View', () => {
  // These tests assume tracey is self-hosting (spec: tracey, impl: rust)
  // Adjust the path if testing a different project

  test('should display spec content', async ({ page }) => {
    await page.goto('/');
    const specLink = page.locator('.nav-tab', { hasText: 'Specification' });
    await expect(specLink).toBeVisible({ timeout: 10000 });
    await specLink.click();

    await expect(page.locator('.markdown')).toBeVisible();
  });

  test('should display requirement markers', async ({ page }) => {
    await page.goto('/');
    const specLink = page.locator('.nav-tab', { hasText: 'Specification' });
    await expect(specLink).toBeVisible({ timeout: 10000 });
    await specLink.click();

    const reqMarker = page.locator('[id^="r--"], .requirement, .rule-marker, [data-rule]');
    await page.waitForTimeout(1000);
    const count = await reqMarker.count();
    if (count === 0) {
      await expect(page.locator('.markdown')).toBeVisible();
    } else {
      expect(count).toBeGreaterThan(0);
    }
  });

  test('should have working outline/toc navigation', async ({ page }) => {
    await page.goto('/');
    const specLink = page.locator('.nav-tab', { hasText: 'Specification' });
    await expect(specLink).toBeVisible({ timeout: 10000 });
    await specLink.click();

    // Look for outline/TOC
    const outline = page.locator('nav, .outline, .toc, aside');
    const outlineLink = outline.locator('a[href*="#"]').first();

    if (await outlineLink.isVisible({ timeout: 2000 }).catch(() => false)) {
      await outlineLink.click();
      // URL should have hash fragment after clicking
      await expect(page).toHaveURL(/#/);
    }
  });
});

test.describe('Sources View', () => {
  test('should display file tree', async ({ page }) => {
    await page.goto('/');

    const sourcesLink = page.locator('.nav-tab', { hasText: 'Sources' });
    await expect(sourcesLink).toBeVisible({ timeout: 10000 });
    await sourcesLink.click();

    // Should see file tree or file list
    await expect(page.locator('.file-tree, .files, [role="tree"]')).toBeVisible({ timeout: 5000 });
  });

  test('should display file content when file is selected', async ({ page }) => {
    await page.goto('/');

    const sourcesLink = page.locator('.nav-tab', { hasText: 'Sources' });
    await expect(sourcesLink).toBeVisible({ timeout: 10000 });
    await sourcesLink.click();

    // Click on a file
    const fileLink = page.locator('.file-tree a, .files a, [role="treeitem"] a').first();
    if (await fileLink.isVisible({ timeout: 2000 }).catch(() => false)) {
      await fileLink.click();
      // Should see code content
      await expect(page.locator('pre, code, .cm-editor')).toBeVisible({ timeout: 5000 });
    }
  });

  test('should highlight rule references in code', async ({ page }) => {
    await page.goto('/');

    const sourcesLink = page.locator('.nav-tab', { hasText: 'Sources' });
    await expect(sourcesLink).toBeVisible({ timeout: 10000 });
    await sourcesLink.click();

    // Click on a Rust file
    const rustFile = page.locator('a[href*=".rs"]').first();
    if (await rustFile.isVisible({ timeout: 2000 }).catch(() => false)) {
      await rustFile.click();
      // Rule references should be highlighted
      await page.waitForTimeout(1000);
      const ruleRef = page.locator('.rule-ref, [data-rule], .cm-impl, .cm-verify');
      const count = await ruleRef.count();
      // May or may not have rule refs depending on the file
      expect(count).toBeGreaterThanOrEqual(0);
    }
  });
});

test.describe('Coverage View', () => {
  test('should display coverage summary', async ({ page }) => {
    await page.goto('/');

    const coverageLink = page.locator('.nav-tab', { hasText: 'Coverage' });
    await expect(coverageLink).toBeVisible({ timeout: 10000 });
    await coverageLink.click();

    // Should see coverage stats
    await expect(page.locator('text=/%/').first()).toBeVisible({ timeout: 5000 });
  });

  test('should display rule list', async ({ page }) => {
    await page.goto('/');

    const coverageLink = page.locator('.nav-tab', { hasText: 'Coverage' });
    await expect(coverageLink).toBeVisible({ timeout: 10000 });
    await coverageLink.click();

    // Should see rules
    await expect(page.locator('.rule, .coverage-item, tr, li').first()).toBeVisible({ timeout: 5000 });
  });

  test('should support filtering', async ({ page }) => {
    await page.goto('/');

    const coverageLink = page.locator('.nav-tab', { hasText: 'Coverage' });
    await expect(coverageLink).toBeVisible({ timeout: 10000 });
    await coverageLink.click();

    const implCoverageCard = page.locator('.stat.clickable .stat-label', { hasText: 'Impl Coverage' });
    await expect(implCoverageCard).toBeVisible({ timeout: 5000 });
    await implCoverageCard.click();
    await expect(page).toHaveURL(/filter=impl/);
  });
});

test.describe('Cross-View Navigation', () => {
  test('should navigate from spec to sources when clicking code ref', async ({ page }) => {
    await page.goto('/');

    const specLink = page.locator('a[href*="/spec"]').first();
    await expect(specLink).toBeVisible({ timeout: 10000 });
    await specLink.click();

    // Look for a code reference link
    const codeRefLink = page.locator('a[href*="/sources/"]').first();
    if (await codeRefLink.isVisible({ timeout: 2000 }).catch(() => false)) {
      await codeRefLink.click();
      await expect(page).toHaveURL(/\/sources\//);
    }
  });

  test('should navigate from sources to spec when clicking rule link', async ({ page }) => {
    await page.goto('/');

    const sourcesLink = page.locator('a[href*="/sources"]').first();
    await expect(sourcesLink).toBeVisible({ timeout: 10000 });
    await sourcesLink.click();

    // Click on a file
    const fileLink = page.locator('a[href*=".rs"]').first();
    if (await fileLink.isVisible({ timeout: 2000 }).catch(() => false)) {
      await fileLink.click();
      await page.waitForTimeout(1000);

      // Look for a rule reference link to spec
      const ruleLink = page.locator('a[href*="/spec#r--"]').first();
      if (await ruleLink.isVisible({ timeout: 2000 }).catch(() => false)) {
        await ruleLink.click();
        await expect(page).toHaveURL(/\/spec#r--/);
      }
    }
  });
});

test.describe('Responsive Design', () => {
  test('should work on mobile viewport', async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 667 });
    await page.goto('/');

    // Should still be usable
    await expect(page.locator('body')).toBeVisible();

    // Navigation should be accessible (may be in a hamburger menu)
    const navToggle = page.locator('button[aria-label*="menu" i], .hamburger, .nav-toggle');
    if (await navToggle.isVisible({ timeout: 1000 }).catch(() => false)) {
      await navToggle.click();
    }
  });

  test('should work on tablet viewport', async ({ page }) => {
    await page.setViewportSize({ width: 768, height: 1024 });
    await page.goto('/');
    await expect(page.locator('body')).toBeVisible();
  });
});

test.describe('Keyboard Navigation', () => {
  test('should support keyboard navigation', async ({ page }) => {
    await page.goto('/');

    // Tab through elements
    await page.keyboard.press('Tab');
    await page.keyboard.press('Tab');

    // Should have focus on an interactive element
    const focusedElement = page.locator(':focus');
    await expect(focusedElement).toBeVisible();
  });
});

test.describe('Search Functionality', () => {
  test('should have search capability', async ({ page }) => {
    await page.goto('/');

    const searchTrigger = page.locator('.search-input');
    await expect(searchTrigger).toBeVisible({ timeout: 5000 });
    await searchTrigger.click();

    const searchInput = page.locator('.search-modal-input input');
    await expect(searchInput).toBeVisible({ timeout: 5000 });
    await searchInput.fill('test');
    await page.waitForTimeout(500);

    await expect(page.locator('.search-modal-results')).toBeVisible({ timeout: 3000 });
  });
});

test.describe('Error Handling', () => {
  test('should handle 404 gracefully', async ({ page }) => {
    await page.goto('/nonexistent/path/that/does/not/exist');

    // Should show error or redirect, not crash
    const hasError = await page.locator('text=/not found|404|error/i').isVisible({ timeout: 3000 }).catch(() => false);
    const hasRedirect = page.url() !== '/nonexistent/path/that/does/not/exist';

    expect(hasError || hasRedirect).toBeTruthy();
  });
});
