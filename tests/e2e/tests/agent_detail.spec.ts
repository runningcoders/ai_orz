import { test, expect, type Page } from '@playwright/test';

// Agent 详情页子 tab 巡检：登录态下进入首个 Agent 详情页，逐个切换子 tab，
// 用「心跳法」验证主线程不被死循环冻结，并断言关系图等 Canvas tab 正常渲染。
//
// 背景：曾出现过 CanvasScene sync_nodes effect 在 use_effect 内订阅并 set 自身
// 依赖的 nodes_state，被 RAF 每帧触发后形成无限循环，冻结 WASM 主线程直到浏览器
// 崩溃（详情页关系图 tab 卡死）。此处用主线程心跳作为通用回归保护。

/** 注入心跳计数器（50ms 一次）。若主线程被死循环占满，计数器将停止增长。 */
async function injectHeartbeat(page: Page): Promise<void> {
  await page.evaluate(() => {
    const w = window as any;
    w.__heart = 0;
    if (!w.__heartTimer) {
      w.__heartTimer = setInterval(() => {
        w.__heart++;
      }, 50);
    }
  });
}

/** 等待 ms 毫秒并返回心跳增量。主线程冻结时增量 ≈ 0，且 evaluate 可能卡住。 */
async function heartbeatDelta(page: Page, ms = 1000): Promise<number> {
  const before = await page.evaluate(() => (window as any).__heart ?? 0, { timeout: 5000 });
  await page.waitForTimeout(ms);
  const after = await page.evaluate(() => (window as any).__heart ?? 0, { timeout: 5000 });
  return after - before;
}

/** 进入首个 Agent 的详情页（从列表页取第一个 /hr/agents/{id} 链接）。 */
async function gotoFirstAgentDetail(page: Page): Promise<void> {
  await page.goto('/hr/agents');
  // 直接等 Agent 名称链接出现。注意：不要用 getByText('Agent 管理') 断言——
  // 它可能匹配到导航栏里默认隐藏的 <a href="/hr/agents">Agent 管理</a> 链接（hidden）。
  const link = page.locator('a[href^="/hr/agents/"]').first();
  await expect(link).toBeVisible({ timeout: 45_000 });
  const href = await link.getAttribute('href');
  expect(href, '应能从 Agent 列表拿到详情链接').toBeTruthy();
  await page.goto(href!);
  // 等「概览」tab 按钮出现（tab 是可见的 button 元素，避免文本歧义）
  await expect(page.getByRole('button', { name: /概览/ }).first()).toBeVisible({
    timeout: 45_000,
  });
}

/** 点击指定子 tab（按文本子串匹配）。 */
async function clickTab(page: Page, name: string): Promise<void> {
  const tab = page.getByRole('button', { name: new RegExp(name) });
  await expect(tab).toBeVisible({ timeout: 10_000 });
  await tab.click();
}

test.describe('Agent 详情页子 tab 巡检', () => {
  test('关系图 tab：主线程不被冻结且渲染 canvas', async ({ page }) => {
    await gotoFirstAgentDetail(page);
    await injectHeartbeat(page);
    await page.waitForTimeout(500);

    await clickTab(page, '关系图');
    await page.waitForTimeout(2000);

    // 主线程存活：心跳持续增长（若死循环，第二次 evaluate 会卡住/超时）
    const delta = await heartbeatDelta(page, 1200);
    expect(delta, '关系图 tab 切换后主线程应保持响应').toBeGreaterThan(0);

    // CanvasScene 渲染出画布
    await expect(page.locator('canvas').first()).toBeVisible({ timeout: 10_000 });
    // 页面级错误提示不应出现
    await expect(page.locator('.alert-error')).toHaveCount(0);
  });

  // 其余子 tab 逐一巡检，防止同类「effect 内 set 自身依赖」死循环回归
  for (const tabName of ['概览', '工具与技能', '状态图', '对话与记忆', '知识图谱', '运行时']) {
    test(`tab「${tabName}」：主线程不被冻结`, async ({ page }) => {
      await gotoFirstAgentDetail(page);
      await injectHeartbeat(page);
      await page.waitForTimeout(500);

      await clickTab(page, tabName);
      await page.waitForTimeout(1500);

      const delta = await heartbeatDelta(page, 1000);
      expect(delta, `tab「${tabName}」切换后主线程应保持响应`).toBeGreaterThan(0);
      await expect(page.locator('.alert-error')).toHaveCount(0);
    });
  }
});
