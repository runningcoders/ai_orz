import { test, expect } from '@playwright/test';

// 页面导航巡检：登录态下逐个访问主要路由，断言布局渲染 + 页面关键元素出现 + 无错误提示。
// marker 取自各页面 card-title / 区块标题文案，白屏或渲染失败会直接暴露。

interface RouteCase {
  path: string;
  /** 页面关键文案（可选：Canvas/HUD 页面无稳定文案时只做布局级断言） */
  marker?: string;
}

const ROUTES: RouteCase[] = [
  // MessageChat 已接入统一 Navbar（h-screen 布局），与其他页面一样断言导航栏
  { path: '/', marker: '项目列表' },
  { path: '/messages/search', marker: '消息搜索' },
  { path: '/hr/agents', marker: 'Agent 管理' },
  { path: '/hr/skills', marker: '技能库' },
  { path: '/hr/knowledge-graph', marker: '知识图谱' },
  { path: '/finance/model-providers', marker: '模型提供商管理' },
  { path: '/finance/tools', marker: '工具管理' },
  { path: '/finance/identity', marker: '身份凭证' },
  { path: '/finance/attachments', marker: '附件管理' },
  { path: '/projects', marker: '项目管理' },
  { path: '/tasks', marker: '任务管理' },
  { path: '/organization', marker: '组织信息' },
  { path: '/organization/users', marker: '用户管理' },
  { path: '/system/logs', marker: '日志查询' },
  // Canvas/HUD 页面：仅断言布局渲染与无错误
  { path: '/workspace' },
  { path: '/system/health' },
];

// 当前用例捕获到的页面致命错误 / 控制台 error，失败时由 afterEach 归档到报告
let capturedPageErrors: string[] = [];

test.afterEach(async ({}, testInfo) => {
  if (testInfo.status !== testInfo.expectedStatus && capturedPageErrors.length > 0) {
    await testInfo.attach('page-errors', {
      body: capturedPageErrors.join('\n---\n'),
      contentType: 'text/plain',
    });
  }
});

for (const { path: route, marker } of ROUTES) {
  test(`页面可访问：${route}`, async ({ page }) => {
    // 捕获页面致命错误与控制台 error，失败时随报告归档，便于定位 wasm 白屏类问题
    capturedPageErrors = [];
    page.on('pageerror', (err) => capturedPageErrors.push(`pageerror: ${err.message}`));
    page.on('console', (msg) => {
      if (msg.type() === 'error') capturedPageErrors.push(`console.error: ${msg.text()}`);
    });

    await page.goto(route);

    // 导航栏渲染说明认证态生效、全局布局正常。
    // debug wasm ~15MB 每个新 context 都要重新编译，冷启动时序紧，给足 45s
    await expect(page.getByRole('link', { name: 'AI Orz' })).toBeVisible({ timeout: 45_000 });

    if (marker) {
      await expect(page.getByText(marker).first()).toBeVisible({ timeout: 30_000 });
    }

    // 页面级错误提示不应出现（Toast error / ErrorState 共用 alert-error 样式）
    await expect(page.locator('.alert-error')).toHaveCount(0);
  });
}
