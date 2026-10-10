/**
 * @created 2026-10-11
 * @description 在真实 Rust 服务上验证登录、配置编辑和响应式界面。
 * @author yunhungo
 */
import { test, expect, type Page } from '@playwright/test';
import { spawn, type ChildProcess } from 'node:child_process';
import { mkdtemp, readFile, rm, mkdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { randomBytes } from 'node:crypto';
let processHandle: ChildProcess; let directory: string;
const password = randomBytes(24).toString('hex');
test.describe.configure({ mode: 'serial' });
test.beforeAll(async () => {
  directory = await mkdtemp(resolve(tmpdir(), 'openlucky-browser-'));
  processHandle = spawn(resolve('../target/debug/openlucky'), { cwd: resolve('..'), env: { ...process.env, OPENLUCKY_DATA: directory, OPENLUCKY_ADMIN_ADDR: '127.0.0.1:56302', OPENLUCKY_HTTP_ADDR: '127.0.0.1:58380', OPENLUCKY_HTTPS_ADDR: '127.0.0.1:58343' }, stdio: 'ignore' });
  for (let attempt = 0; attempt < 100; attempt++) { try { if ((await fetch('http://127.0.0.1:56302/healthz')).ok) break; } catch { /* 等待服务启动。 */ } await new Promise(r => setTimeout(r, 100)); }
  const token = await readFile(resolve(directory, 'setup-token'), 'utf8');
  const result = await fetch('http://127.0.0.1:56302/api/v1/auth/setup', { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-OpenLucky': '1' }, body: JSON.stringify({ username: 'admin', password, setup_token: token }) });
  expect(result.ok).toBe(true);
  await mkdir(resolve('../.myself/qa'), { recursive: true });
});
test.afterAll(async () => { processHandle?.kill('SIGTERM'); await new Promise<void>(r => { if (processHandle?.exitCode !== null) r(); else processHandle.once('exit', () => r()); }); if (directory) await rm(directory, { recursive: true, force: true }); });
async function login(page: Page) { await page.goto('/'); await page.getByLabel('用户名', { exact: true }).fill('admin'); await page.getByLabel('密码', { exact: true }).fill(password); await page.getByRole('button', { name: '登录', exact: true }).click(); await expect(page.getByRole('heading', { name: '概览', exact: true })).toBeVisible(); }
test('desktop login, CRUD, validation and theme', async ({ page }) => {
  const errors: string[] = []; page.on('pageerror', e => errors.push(e.message));
  await login(page);
  await page.screenshot({ path: '../.myself/qa/overview-desktop.png', fullPage: true });
  await page.getByRole('link', { name: '设置', exact: true }).click(); await page.getByRole('link', { name: 'DNS 凭据', exact: true }).click();
  await page.getByRole('button', { name: '添加', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByLabel('名称', { exact: true }).fill('测试 Cloudflare'); await dialog.getByLabel('API Token', { exact: true }).fill('browser-test-token'); await dialog.getByRole('button', { name: '保存', exact: true }).click();
  await expect(page.getByText('测试 Cloudflare', { exact: true })).toBeVisible();
  await page.getByRole('link', { name: '动态域名', exact: true }).click(); await page.getByRole('button', { name: '添加', exact: true }).click();
  await dialog.getByLabel('名称', { exact: true }).fill('测试域名'); await dialog.getByLabel('DNS 凭据', { exact: true }).selectOption({ label: '测试 Cloudflare' }); await dialog.getByLabel('主域名', { exact: true }).fill('example.test'); await dialog.getByLabel('记录域名', { exact: true }).fill('nas.example.test'); await dialog.getByLabel('IP 来源', { exact: true }).selectOption('static'); await dialog.getByLabel('网卡名称 / IP 地址', { exact: true }).fill('203.0.113.5'); await dialog.getByRole('switch', { name: '启用', exact: true }).click(); await dialog.getByRole('button', { name: '保存', exact: true }).click();
  await expect(page.getByText('nas.example.test', { exact: true })).toBeVisible();
  await page.screenshot({ path: '../.myself/qa/ddns-desktop.png', fullPage: true });
  await page.getByRole('button', { name: '编辑 测试域名', exact: true }).click(); await dialog.getByLabel('记录域名', { exact: true }).fill('invalid..test'); await dialog.getByRole('button', { name: '保存', exact: true }).click(); await expect(dialog.getByRole('alert')).toContainText('有效域名'); await expect(dialog.getByLabel('记录域名', { exact: true })).toHaveValue('invalid..test'); await dialog.getByRole('button', { name: '取消', exact: true }).click();
  await page.getByLabel('搜索动态域名').fill('missing'); await expect(page.getByText('没有匹配的结果')).toBeVisible(); await page.getByRole('button', { name: '清除筛选' }).click();
  await page.getByRole('button', { name: '深色模式', exact: true }).click(); await expect(page.locator('html')).toHaveClass(/dark/); await page.screenshot({ path: '../.myself/qa/ddns-dark.png', fullPage: true });
  await page.getByRole('button', { name: '浅色模式', exact: true }).click();
  await page.getByRole('button', { name: '删除 测试域名', exact: true }).click(); await page.getByRole('alertdialog').getByRole('button', { name: '删除', exact: true }).click(); await expect(page.getByText('暂无动态域名', { exact: true })).toBeVisible();
  expect(errors).toEqual([]);
});
test('mobile navigation and responsive layout', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 }); await login(page); await page.screenshot({ path: '../.myself/qa/overview-mobile.png', fullPage: true });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.getByRole('button', { name: '打开导航' }).click(); await page.getByRole('dialog').getByRole('link', { name: 'Web 服务', exact: true }).click(); await expect(page.getByRole('heading', { name: 'Web 服务', exact: true })).toBeVisible(); await page.getByRole('button', { name: '添加', exact: true }).click(); await expect(page.getByRole('dialog').getByLabel('目标地址')).toBeVisible(); await page.screenshot({ path: '../.myself/qa/editor-mobile.png', fullPage: true });
});
test('wrong login remains recoverable', async ({ page }) => { await page.goto('/'); await page.getByLabel('用户名', { exact: true }).fill('admin'); await page.getByLabel('密码', { exact: true }).fill('incorrect-password'); await page.getByRole('button', { name: '登录', exact: true }).click(); await expect(page.getByRole('alert')).toContainText('用户名或密码错误'); await expect(page.getByLabel('用户名', { exact: true })).toHaveValue('admin'); });
test('revoked sessions return from cached pages to login', async ({ page }) => {
  await login(page);
  const response = await page.request.post('/api/v1/auth/logout', { headers: { 'X-OpenLucky': '1' } });
  expect(response.ok()).toBe(true);
  await page.getByRole('link', { name: '动态域名', exact: true }).click();
  await expect(page.getByRole('button', { name: '登录', exact: true })).toBeVisible();
  await expect(page.getByRole('navigation', { name: '主导航' })).toHaveCount(0);
});
test('pending writes keep the editor open and preserve failed input', async ({ page }) => {
  await login(page);
  await page.getByRole('link', { name: '设置', exact: true }).click();
  await page.getByRole('link', { name: 'DNS 凭据', exact: true }).click();
  await page.getByRole('button', { name: '添加', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByLabel('名称', { exact: true }).fill('保存边界测试');
  await dialog.getByLabel('API Token', { exact: true }).fill('test-only-token');
  let release: (() => void) | undefined;
  const gate = new Promise<void>(resolve => { release = resolve; });
  await page.route('**/api/v1/resources/credentials', async route => {
    if (route.request().method() !== 'POST') { await route.continue(); return; }
    await gate;
    await route.fulfill({ status: 503, contentType: 'application/json', body: JSON.stringify({ error: '服务暂时不可用' }) });
  });
  await dialog.getByRole('button', { name: '保存', exact: true }).click();
  await expect(dialog.getByRole('button', { name: '保存', exact: true })).toBeDisabled();
  await page.keyboard.press('Escape');
  await expect(dialog).toBeVisible();
  if (release) release();
  await expect(dialog.getByRole('alert')).toContainText('服务暂时不可用');
  await expect(dialog.getByLabel('名称', { exact: true })).toHaveValue('保存边界测试');
  await expect(dialog.getByRole('button', { name: '取消', exact: true })).toBeEnabled();
});
