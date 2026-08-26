import { test as setup, expect, type Page } from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';

// 登录冒烟 setup：通过浏览器完整走一遍「系统初始化 → 错误密码 → 登录成功」用户流程，
// 成功后保存 storageState（Cookie + localStorage），后续 spec 直接复用登录态。

const ORG_NAME = 'E2E 测试组织';
const ADMIN_USER = 'admin';
const ADMIN_PASS = 'E2E-admin-pass';

// Playwright 将 TS 转译为 CJS 执行，此处用 __dirname 定位 e2e/.auth（与 config 中 storageState 路径一致）
const stateDir = path.join(__dirname, '..', '.auth');
const STATE_FILE = path.join(stateDir, 'state.json');

/**
 * 接待页控件均带 data-testid 锚点（稳定选择器，不随文案变化）。
 * 仅在 testid 未覆盖的元素上回退到文本/class 选择器。
 */
const tid = (page: Page, id: string) => page.getByTestId(id);

setup('系统初始化 → 错误密码 → 登录成功（全流程冒烟）', async ({ page }) => {
  setup.setTimeout(180_000);

  await page.goto('/login');
  // WASM 首次加载较慢，等待初始化表单渲染完成
  await expect(tid(page, 'init-org-name')).toBeVisible({ timeout: 60_000 });

  // ---- 填写系统初始化表单（模型配置使用占位值，初始化不发外部请求）----
  await tid(page, 'init-org-name').fill(ORG_NAME);
  await tid(page, 'init-username').fill(ADMIN_USER);
  await tid(page, 'init-password').fill(ADMIN_PASS);

  // 两步向导：Step 1 基础信息填完后点「下一步」进入 Step 2 模型配置
  await tid(page, 'init-next-step').click();
  await expect(tid(page, 'init-wizard-step-2')).toBeVisible();

  await tid(page, 'init-chat-provider-name').fill('TestChat');
  await tid(page, 'init-chat-provider-type').selectOption({ label: 'OpenAI' });
  await tid(page, 'init-chat-model-name').fill('gpt-4o-mini');
  await tid(page, 'init-chat-api-key').fill('sk-e2e-dummy-key');

  // 向量模型默认启用且默认 FastEmbed（本地，无需 API Key），Provider 名称与模型名必填否则前端校验拦截
  await expect(tid(page, 'init-enable-embedding')).toBeChecked();
  await tid(page, 'init-embedding-provider-name').fill('FastEmbed');
  await tid(page, 'init-embedding-model-name').fill('BAAI/bge-small-en-v1.5');

  await tid(page, 'init-submit').click();

  // 初始化进度展示后，前端轮询完成会自动刷新进入登录表单
  await expect(page.getByRole('heading', { name: '欢迎回来' })).toBeVisible({ timeout: 120_000 });

  // ---- 选择组织 + 错误密码：应出现错误提示 ----
  await page.locator('.reception-org-item', { hasText: ORG_NAME }).click();
  await tid(page, 'login-username').fill(ADMIN_USER);
  await tid(page, 'login-password').fill('wrong-password');
  await tid(page, 'login-submit').click();
  await expect(page.locator('.alert-error')).toBeVisible();

  // ---- 正确密码：登录后跳转对话首页 ----
  await tid(page, 'login-password').fill(ADMIN_PASS);
  await tid(page, 'login-submit').click();
  await page.waitForURL(/\/$/, { timeout: 60_000 });
  // WASM 登录后重载较慢：等导航栏出现，确认页面已接管（统一布局渲染完成）。
  // 注意：不要用 getByText('项目列表') 断言——它可能匹配到导航栏里默认隐藏的
  // <a href="/projects">项目列表</a> 链接（hidden），导致 toBeVisible 误判失败。
  await expect(page.getByRole('link', { name: 'AI Orz' })).toBeVisible({ timeout: 60_000 });

  // ---- 保存登录态（HttpOnly Cookie + localStorage 标志位）----
  fs.mkdirSync(stateDir, { recursive: true });
  await page.context().storageState({ path: STATE_FILE });
});
