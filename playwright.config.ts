import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  workers: 1,
  outputDir: 'artifacts/playwright',
  use: {
    baseURL: 'http://127.0.0.1:1420',
    viewport: { width: 1440, height: 1040 },
  },
  webServer: {
    command: 'npm run dev',
    url: 'http://127.0.0.1:1420',
    reuseExistingServer: true,
  },
});
