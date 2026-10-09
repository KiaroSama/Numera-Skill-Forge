import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests',
  outputDir: './coverage/ui-results',
  fullyParallel: false,
  workers: 1,
  forbidOnly: true,
  retries: 0,
  timeout: 15_000,
  globalTimeout: 120_000,
  expect: { timeout: 3_000 },
  reporter: 'line',
  webServer: {
    command: 'npm run dev',
    url: 'http://127.0.0.1:1420',
    timeout: 15_000,
    reuseExistingServer: false,
    stdout: 'pipe',
    stderr: 'pipe'
  },
  use: {
    baseURL: 'http://127.0.0.1:1420',
    browserName: 'chromium',
    headless: true,
    actionTimeout: 3_000,
    navigationTimeout: 5_000,
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    viewport: { width: 1280, height: 900 }
  }
});
