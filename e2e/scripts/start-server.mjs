#!/usr/bin/env node
// E2E 服务编排脚本（由 Playwright webServer 调用）
//
// 职责：
// 1. 校验 dist/ 前端产物存在（不存在提示先执行 make build）
// 2. 创建干净的隔离数据目录 .e2e-runtime/，写入独立端口配置
// 3. 定位后端二进制（AI_ORZ_E2E_BINARY > release > debug），缺失时自动 cargo build
// 4. 以后台进程方式启动服务，Playwright 通过 /health 轮询就绪

import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, '..', '..');

const port = process.env.AI_ORZ_E2E_PORT ?? '3310';
const runtimeDir = path.join(repoRoot, '.e2e-runtime');
const distDir = path.join(repoRoot, 'dist');

// ---- 1. 前端产物检查（兼容两种 dx 产出布局）----
// 新版 dx：dist/assets/frontend-*.wasm（带 hash）；旧版/回退：dist/pkg/frontend_bg.wasm
const distAssets = path.join(distDir, 'assets');
const hasHashedWasm =
  fs.existsSync(distAssets) && fs.readdirSync(distAssets).some((f) => f.endsWith('.wasm'));
const hasLegacyWasm = fs.existsSync(path.join(distDir, 'pkg', 'frontend_bg.wasm'));
if (!fs.existsSync(path.join(distDir, 'index.html')) || (!hasHashedWasm && !hasLegacyWasm)) {
  console.error('[e2e] dist/ 缺少前端构建产物，请先执行 make build');
  process.exit(1);
}

// ---- 2. 干净的数据目录 + 独立端口配置（避免与本地开发服务冲突）----
fs.rmSync(runtimeDir, { recursive: true, force: true });
fs.mkdirSync(runtimeDir, { recursive: true });
const distDirToml = distDir.replaceAll('\\', '/');
fs.writeFileSync(
  path.join(runtimeDir, 'ai_orz.toml'),
  `# E2E 测试运行时自动生成，勿手工编辑（每次运行会被重写）
[server]
listen_addr = "127.0.0.1:${port}"

[database]
db_file_name = "ai_orz.db"

[frontend]
dist_dir = "${distDirToml}"

[logging]
enable_file_log = false
log_subdir = "logs"
`,
);

// ---- 3. 定位后端二进制 ----
// 优先级：AI_ORZ_E2E_BINARY > debug > release。
// 注意 debug 优先于 release：本地 target/release 常是陈旧构建（不含最新路由/修复），
// E2E 必须与当前代码一致；CI job 显式传入 AI_ORZ_E2E_BINARY。
let binary = process.env.AI_ORZ_E2E_BINARY;
if (!binary) {
  for (const rel of ['target/debug/ai_orz', 'target/release/ai_orz']) {
    const candidate = path.join(repoRoot, rel);
    if (fs.existsSync(candidate)) {
      binary = candidate;
      break;
    }
  }
}
if (!binary) {
  console.log('[e2e] 未找到后端二进制，执行 cargo build（首次较慢）...');
  const result = spawnSync('cargo', ['build'], { cwd: repoRoot, stdio: 'inherit' });
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
  binary = path.join(repoRoot, 'target/debug/ai_orz');
}

// ---- 4. 启动服务 ----
console.log(`[e2e] 启动 ${binary}，监听 127.0.0.1:${port}，数据目录 ${runtimeDir}`);
const child = spawn(binary, {
  cwd: repoRoot,
  env: {
    ...process.env,
    AI_ORZ_BASE_PATH: runtimeDir,
    FRONTEND_DIST_DIR: distDir,
  },
  stdio: 'inherit',
});

child.on('exit', (code) => process.exit(code ?? 0));
process.on('SIGTERM', () => {
  child.kill('SIGTERM');
});
