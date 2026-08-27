// 页面白屏/崩溃诊断脚本：全新隔离服务器 + API 直调初始化登录，
// 串行巡检指定路由（命令行传入，默认 /hr/knowledge-graph），
// 记录每个请求生命周期 + 页面崩溃/浏览器断开事件。
// 用法：node scripts/diagnose-page.mjs [/path1 /path2 ...]
import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { rmSync, mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const PORT = 3312;
const BASE = `http://127.0.0.1:${PORT}`;

// 全新数据目录 + 独立配置（对齐 start-server.mjs）
const dataDir = path.join(root, '.ai_orz_e2e_dbg');
rmSync(dataDir, { recursive: true, force: true });
mkdirSync(dataDir, { recursive: true });
writeFileSync(
  path.join(dataDir, 'ai_orz.toml'),
  `[server]
listen_addr = "127.0.0.1:${PORT}"

[database]
db_file_name = "ai_orz.db"

[frontend]
dist_dir = "${path.join(root, 'dist').replaceAll('\\', '/')}"

[logging]
enable_file_log = false
log_subdir = "logs"
`,
);

const server = spawn(path.join(root, 'target/debug/ai_orz'), [], {
  cwd: root,
  env: { ...process.env, AI_ORZ_BASE_PATH: dataDir },
});
server.stdout.on('data', (d) => process.stdout.write(`[server] ${d}`));
server.stderr.on('data', (d) => process.stderr.write(`[server-err] ${d}`));

// 后端未实现优雅退出，任何退出路径都必须 SIGKILL，否则残留进程占住 DB
for (const sig of ['exit', 'SIGINT', 'SIGTERM', 'uncaughtException', 'unhandledRejection']) {
  process.on(sig, () => {
    try {
      server.kill('SIGKILL');
    } catch {}
  });
}

// 等待健康检查
for (let i = 0; i < 60; i++) {
  try {
    const r = await fetch(`${BASE}/health`);
    if (r.ok) break;
  } catch {}
  await new Promise((r) => setTimeout(r, 1000));
}

const t0 = Date.now();
const log = (m) => console.log(`[${((Date.now() - t0) / 1000).toFixed(1)}s] ${m}`);
const die = (msg) => {
  log(msg);
  server.kill('SIGKILL');
  process.exit(1);
};

const browser = await chromium.launch();
browser.on('disconnected', () => log('!!! BROWSER DISCONNECTED (crashed?)'));
const page = await browser.newPage();
page.on('crash', () => log('!!! PAGE CRASHED'));
page.on('console', (m) => log(`console.${m.type()}: ${m.text().slice(0, 200)}`));
page.on('pageerror', (e) => log(`pageerror: ${e.message}`));
const pending = new Map();
page.on('request', (r) => {
  pending.set(r.url(), Date.now());
  log(`REQ ${r.method()} ${r.url().replace(BASE, '')}`);
});
page.on('requestfinished', (r) => {
  pending.delete(r.url());
  log(`DONE ${r.url().replace(BASE, '')}`);
});
page.on('requestfailed', (r) => {
  pending.delete(r.url());
  log(`FAIL ${r.url().replace(BASE, '')} ${r.failure()?.errorText}`);
});

// 1) 初始化（异步 task）+ 轮询进度 + 登录（对齐集成测试工厂契约）
const init = await fetch(`${BASE}/api/v1/organization/initialize`, {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({
    organization_name: 'E2E Debug Org',
    admin_username: 'admin',
    admin_password: 'hash-debug',
    chat_model: {
      name: 'TestChat',
      provider_type: 0,
      model_name: 'gpt-4o-mini',
      api_key: 'sk-e2e-dummy-key',
    },
    // 与 auth.setup.ts 一致：启用 FastEmbed 向量模型
    embedding_model: {
      name: 'FastEmbed',
      provider_type: 6,
      model_name: 'BAAI/bge-small-en-v1.5',
      api_key: '',
    },
  }),
});
const initBody = await init.json();
log(`init API -> ${init.status} ${JSON.stringify(initBody).slice(0, 200)}`);
const taskId = initBody?.data?.task_id;

let orgId = null;
for (let i = 0; i < 300; i++) {
  const p = await fetch(`${BASE}/api/v1/organization/initialize/progress?task_id=${taskId}`);
  const pb = await p.json();
  const st = pb?.data?.status;
  if (st === 'completed') {
    orgId = pb.data.result?.organization_id;
    log(`init completed, org_id=${orgId}`);
    break;
  }
  if (st === 'failed') {
    die(`init FAILED: ${JSON.stringify(pb.data)}`);
  }
  await new Promise((r) => setTimeout(r, 500));
}
if (!orgId) {
  die('init progress timeout');
}

const login = await fetch(`${BASE}/api/v1/organization/auth/login`, {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ organization_id: orgId, username: 'admin', password: 'hash-debug' }),
});
const setCookie = login.headers.get('set-cookie');
log(`login API -> ${login.status} cookie=${setCookie ? setCookie.split(';')[0].split('=')[0] : 'NO'}`);
const cookie = setCookie?.split(';')[0] ?? '';
await page.context().addCookies([
  { name: cookie.split('=')[0], value: cookie.split('=').slice(1).join('='), url: BASE },
]);

// 2) 登录标志：用 addInitScript 在任何导航前注入（避免额外 warmup 导航）
await page.context().addInitScript(() => localStorage.setItem('ai_orz_logged_in', 'true'));

// 3) 串行巡检路由（命令行传入，默认 kg）：验证白屏与路由/导航次序的关系
const routes = process.argv.slice(2);
const patrol = routes.length > 0 ? routes : ['/hr/knowledge-graph'];
for (const route of patrol) {
  if (!browser.isConnected()) {
    log('browser gone, stop patrol');
    break;
  }
  log(`goto ${route} ...`);
  try {
    await page.goto(`${BASE}${route}`, { timeout: 60_000 });
    let booted = false;
    try {
      await page.waitForFunction(
        () => document.body.innerText.length > 20 && !!document.querySelector('a, button, h1, h2'),
        null,
        { timeout: 25_000, polling: 500 },
      );
      booted = true;
    } catch {}
    const body = await page.evaluate(() => document.body.innerText.replace(/\n+/g, ' | ').slice(0, 120));
    log(`${route} -> ${booted ? 'BOOTED' : 'BLANK'} body=${JSON.stringify(body)}`);
  } catch (e) {
    log(`${route} -> ERROR: ${e.message.split('\n')[0]}`);
  }
}
log(`pending requests: ${[...pending.keys()].join(', ') || '(none)'}`);

try {
  await browser.close();
} catch {}
server.kill('SIGKILL');
process.exit(0);
