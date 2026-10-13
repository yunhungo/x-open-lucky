/**
 * @created 2026-10-11
 * @description 配置真实后台和浏览器的串行端到端验证。
 * @author yunhungo
 */
import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './tests',
  workers: 1,
  timeout: 45000,
  forbidOnly: !!process.env.CI,
  use: {
    baseURL: 'http://127.0.0.1:56302',
    viewport: { width: 1440, height: 1000 },
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  reporter: process.env.CI
    ? [['list'], ['github'], ['html', { open: 'never' }]]
    : 'list',
});
