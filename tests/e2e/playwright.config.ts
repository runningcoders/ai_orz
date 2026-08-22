import { defineConfig } from '@playwright/test';

// 绕过系统代理，确保 Playwright 能直接访问本地后端服务
// （企业环境常有 HTTP_PROXY，会拦截 127.0.0.1 请求导致 502）
process.env.NO_PROXY = '127.0.0.1,localhost';
process.env.no_proxy = '127.0.0.1,localhost';

// E2E 专用端口：与本地开发服务（3000/8080）隔离，可通过环境变量覆盖
const PORT = Number(process.env.AI_ORZ_E2E_PORT ?? 3310);
const BASE_URL = `http://127.0.0.1:${PORT}`;

export default defineConfig({
  testDir: './tests',
  outputDir: './test-results',
  // WASM 首屏加载与系统初始化较慢，放宽单测超时
  timeout: 90_000,
  expect: { timeout: 10_000 },
  // 所有用例共享同一个服务器实例与数据库（初始化只能执行一次），串行执行
  fullyParallel: false,
  workers: 1,
  retries: process.env.CI ? 1 : 0,
  reporter: [['list'], ['html', { open: 'never' }]],
  use: {
    baseURL: BASE_URL,
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    launchOptions: {
      args: ['--no-proxy-server', '--disable-features=IsolateOrigins,site-per-process'],
    },
  },
  projects: [
    {
      // setup：通过浏览器完成系统初始化 + 登录，保存登录态供后续用例复用
      name: 'setup',
      testMatch: /.*\.setup\.ts/,
    },
    {
      name: 'chromium',
      use: {
        browserName: 'chromium',
        storageState: '.auth/state.json',
      },
      dependencies: ['setup'],
      testMatch: /.*\.spec\.ts/,
    },
  ],
  webServer: {
    command: 'node scripts/start-server.mjs',
    url: `${BASE_URL}/health`,
    // 首次运行可能触发 cargo build，预留充足时间
    timeout: 600_000,
    reuseExistingServer: !process.env.CI,
    stdout: 'pipe',
    stderr: 'pipe',
  },
});
