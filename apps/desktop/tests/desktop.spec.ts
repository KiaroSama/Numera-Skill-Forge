import { expect, test } from '@playwright/test';
import type { Page } from '@playwright/test';

// UI contracts only: these controlled IPC responses do not certify Rust, native hosts,
// installation, signing or platform webviews. Native smoke runs separately on each OS:
// build the actual bundle, launch in an isolated offscreen user session, select a scratch
// workspace, inspect status/sync/log output, reject stale approval and ownership conflicts,
// then close the app and assert process cleanup. Never run against a real user harness.
interface Captured { root: string; request: Record<string, unknown> }
declare global {
  interface Window {
    forgeTest: { requests: Captured[]; active: number; maxActive: number; fail: string };
  }
}
const digest = 'a'.repeat(64);

class ForgePage {
  constructor(readonly page: Page) {}
  async view(name: string): Promise<void> {
    await this.page.getByRole('navigation', { name: 'Workspace views' }).getByRole('link', { name, exact: true }).click();
    await expect(this.page.getByRole('heading', { name, exact: true, level: 1 })).toBeVisible();
  }
  async open(): Promise<void> {
    await this.page.getByLabel('Workspace folder').fill('C:/isolated/forge workspace');
    await this.page.getByRole('button', { name: 'Open workspace', exact: true }).click();
    await expect(this.page.getByRole('status').filter({ hasText: 'Workspace opened.' })).toBeVisible();
  }
  async requests(): Promise<Captured[]> {
    return this.page.evaluate(() => window.forgeTest.requests);
  }
}
async function mockBridge(page: Page): Promise<void> {
  await page.addInitScript(() => {
    window.forgeTest = { requests: [], active: 0, maxActive: 0, fail: '' };
    Object.defineProperty(window, 'isTauri', { value: true });
    Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {
      invoke: async (command: string, args: Captured): Promise<unknown> => {
        if (command !== 'dispatch') throw new Error('Unexpected IPC command');
        const state = window.forgeTest;
        state.requests.push(args);
        state.active += 1;
        state.maxActive = Math.max(state.maxActive, state.active);
        try {
          await Promise.resolve();
          const request = args.request['action'] === 'job-run' ? args.request['request'] as Record<string, unknown> : args.request;
          if (args.request['action'] === 'job-run') state.requests[state.requests.length - 1] = { root: args.root, request };
          if (state.fail === request['action']) throw new Error('ownership-conflict: Existing file was edited by the user');
          switch (request['action']) {
            case 'status': return { workspace: args.root, sources: 0, packages: 0, installed: 0 };
            case 'sync': return { status: 'partial', concurrency: 1, sources: [{ source: 'example', status: 'review-required' }] };
            case 'source-list': return [{ id: 'example' }];
            case 'catalog': return [{ id: 'sample' }];
            case 'harnesses': return [{ profile: 'claude', verification: 'static-only' }];
            case 'discover': return { source: request['source'], content_digest: 'a'.repeat(64), components: [], files: [], license_digest: 'b'.repeat(64) };
            case 'approve': return { approved: request['source'], digest: request['digest'] };
            case 'install': return request['dry_run'] === true
              ? { dry_run: true, destination: request['destination'], files: ['.claude/skills/sample/SKILL.md'], changes: 1 }
              : { id: 'receipt-1', enabled: true };
            case 'publish': throw new Error('publication-review: Automatic publication is not configured');
            default: return [];
          }
        } finally { state.active -= 1; }
      }
    } });
  });
}

test('browser mode never invents workspace success; nine views stay accessible', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto('/');
  await expect(page.getByRole('status').filter({ hasText: 'Desktop bridge unavailable' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Refresh status' })).toBeDisabled();
  const forge = new ForgePage(page);
  for (const name of ['Sources', 'Catalog', 'Installed', 'Updates', 'Harnesses', 'History', 'Logs', 'Settings', 'Dashboard']) await forge.view(name);
  await page.getByLabel('Workspace folder').fill('C:/not-a-real-workspace');
  await page.getByRole('button', { name: 'Open workspace', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('Desktop bridge unavailable');
  expect(errors).toEqual([]);
});

test('workspace open and reopen dispatch one ordered check-only sync', async ({ page }) => {
  await mockBridge(page);
  await page.goto('/');
  const forge = new ForgePage(page);
  await forge.open();
  const requests = await forge.requests();
  expect(requests.map(row => row.request['action'])).toEqual(['status', 'sync', 'source-list', 'catalog', 'harnesses']);
  expect(requests[1]?.request).toEqual({ action: 'sync', apply: false, offline: false });
  expect(await page.evaluate(() => window.forgeTest.maxActive)).toBe(1);
  await forge.view('Updates');
  await expect(page.getByRole('region', { name: 'Updates', exact: true }).getByLabel('Service response JSON')).toContainText('review-required');
  await page.reload();
  await expect(page.getByRole('status').filter({ hasText: 'Workspace opened.' })).toBeVisible();
  expect((await forge.requests()).filter(row => row.request['action'] === 'sync')).toHaveLength(1);
});

test('approval binds the discovered digest; executable trust is explicit and reset', async ({ page }) => {
  await mockBridge(page);
  await page.goto('/');
  const forge = new ForgePage(page);
  await forge.open();
  await forge.view('Sources');
  const approve = page.getByRole('button', { name: 'Approve reviewed digest' });
  await expect(approve).toBeDisabled();
  await page.getByLabel('Source to inspect', { exact: true }).fill('example');
  await page.getByRole('button', { name: 'Discover candidate' }).click();
  await expect(page.getByLabel('Reviewed content digest')).toHaveValue(digest);
  await expect(page.getByLabel('Explicitly trust executable content in this digest')).not.toBeChecked();
  await expect(approve).toBeDisabled();
  await page.getByLabel('I reviewed this source and exact digest').check();
  await approve.click();
  expect((await forge.requests()).find(row => row.request['action'] === 'approve')?.request).toEqual({ action: 'approve', source: 'example', digest, allow_executable: false });
  await expect(page.getByLabel('Reviewed content digest')).toHaveValue('');
  await expect(approve).toBeDisabled();
});

test('installation requires a matching preview; edited destinations relock apply and errors remain errors', async ({ page }) => {
  await mockBridge(page);
  await page.goto('/');
  const forge = new ForgePage(page);
  await forge.open();
  await forge.view('Installed');
  const apply = page.getByRole('button', { name: 'Apply installation' });
  await page.getByLabel('Package to install', { exact: true }).fill('sample');
  await page.getByLabel('Installation target', { exact: true }).fill('claude');
  await page.getByLabel('Destination folder', { exact: true }).fill('C:/isolated/approved destination');
  await expect(apply).toBeDisabled();
  await page.getByRole('button', { name: 'Preview installation' }).click();
  await page.getByLabel('I authorize the reviewed destination changes').check();
  await expect(apply).toBeEnabled();
  await page.getByLabel('Installation scope', { exact: true }).selectOption('user');
  await expect(apply).toBeDisabled();
  await page.getByLabel('Destination folder', { exact: true }).fill('C:/different/destination');
  await expect(apply).toBeDisabled();
  await page.getByRole('button', { name: 'Preview installation' }).click();
  await page.getByLabel('I authorize the reviewed destination changes').check();
  await page.evaluate(() => { window.forgeTest.fail = 'install'; });
  await apply.click();
  await expect(page.getByRole('alert').filter({ hasText: 'ownership-conflict' })).toHaveCount(2);
  expect((await forge.requests()).filter(row => row.request['action'] === 'install').map(row => row.request['dry_run'])).toEqual([true, true, false]);
});

test('responsive light/dark presentation contains untrusted response text without markup execution', async ({ page }) => {
  await mockBridge(page);
  await page.goto('/');
  const forge = new ForgePage(page);
  await forge.open();
  await forge.view('Sources');
  await page.getByLabel('Source to inspect', { exact: true }).fill('<img src=x onerror=alert(1)>');
  await page.getByRole('button', { name: 'Discover candidate' }).click();
  await expect(page.getByRole('region', { name: 'Sources', exact: true }).getByLabel('Service response JSON')).toContainText('<img src=x onerror=alert(1)>');
  expect(await page.getByRole('region', { name: 'Sources', exact: true }).getByLabel('Service response JSON').locator('img').count()).toBe(0);
  await page.setViewportSize({ width: 375, height: 812 });
  for (const appearance of ['Light', 'Dark']) {
    await page.getByRole('combobox', { name: /Appearance/ }).selectOption({ label: appearance });
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    await expect(page.getByRole('button', { name: 'Discover candidate' })).toBeVisible();
  }
  await page.setViewportSize({ width: 812, height: 375 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});
