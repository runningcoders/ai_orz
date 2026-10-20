#!/usr/bin/env node
'use strict';
// 受控 stub：按 body.fake_mode 分支模拟官方 ima_api.cjs 的三种行为面（测试专用，勿用于真实环境）
const [, , apiPath, rawBody = '{}'] = process.argv;
let body = {};
try { body = JSON.parse(rawBody); } catch (_) { body = {}; }
const mode = body.fake_mode || 'ok';
if (mode === 'update') {
  // 官方 -200 行为复刻：stdout 写更新上下文 → stderr 结构化错误 → exit 1
  process.stdout.write(JSON.stringify({ current_version: '1.1.9', latest_version: '1.1.10', instruction: '请更新' }));
  process.stderr.write(JSON.stringify({ code: -200, msg: '发现新版本 skill：1.1.10（当前版本：1.1.9）。请更新。' }));
  process.exit(1);
}
if (mode === 'error') {
  process.stderr.write(JSON.stringify({ code: -100, msg: 'missing credentials（stub 注入）' }));
  process.exit(1);
}
process.stdout.write(JSON.stringify({ code: 0, msg: 'success', data: { api_path: apiPath, echo: body } }));
process.exit(0);
