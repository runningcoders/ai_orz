'use strict';
/**
 * server.js 测试（node:test，零依赖）。
 * 分两层：
 *  - 单元层：require('../server.js') 直接测 validateArgs / readWhitelistedDoc / buildManifestLines / TOOL_DEFS 声明面；
 *    CONFIG.installDir 覆写指向 test/fixtures/pkg。
 *  - 协议层：spawnSync 以 ndjson stdio 驱动 server.js 子进程（env IMA_SKILL_INSTALL_DIR=fixtures），
 *    覆盖 initialize / ping / tools/list / tools/call（业务 stub 三分支 + 能力面）/ resources/list|read / 通知 / 启动校验。
 * fixtures：test/fixtures/pkg 受控 stub（fake_mode=ok/error/update 复刻官方 ima_api.cjs 错误面）。
 * 凭证纪律：全程零真实凭证，stub 不消费凭证值，env 仅传占位测试值。
 */
const { test } = require('node:test');
const assert = require('node:assert');
const spawnSync = require('node:child_process').spawnSync;
const path = require('node:path');

const SERVER = path.join(__dirname, '..', 'server.js');
const FIXTURES = path.join(__dirname, 'fixtures', 'pkg');
const { CONFIG, TOOL_DEFS, validateArgs, readWhitelistedDoc, buildManifestLines } = require('../server.js');

CONFIG.installDir = FIXTURES; // 单元层注入

// ─────────────── 单元层：TOOL_DEFS 声明面 ───────────────

test('tools 清单恰 18 件且命名符合映射表', () => {
  assert.strictEqual(TOOL_DEFS.length, 18);
  const names = TOOL_DEFS.map((t) => t.name);
  const expectApi = [
    'ima_search_note', 'ima_list_notebook', 'ima_list_note', 'ima_get_doc_content', 'ima_import_doc', 'ima_append_doc',
    'ima_create_media', 'ima_add_knowledge', 'ima_get_knowledge_base', 'ima_get_knowledge_list', 'ima_search_knowledge',
    'ima_search_knowledge_base', 'ima_get_addable_knowledge_base_list', 'ima_check_repeated_names', 'ima_import_urls',
    'ima_get_media_info',
  ];
  for (const n of expectApi) assert.ok(names.includes(n), 'missing tool: ' + n);
  assert.ok(names.includes('ima_usage'));
  assert.ok(names.includes('ima_detail'));
  assert.strictEqual(new Set(names).size, names.length, 'tool names must be unique');
});

test('16 个 OpenAPI 工具均绑定 apiPath 且 schema 类型为 object', () => {
  const apiTools = TOOL_DEFS.filter((t) => t.name !== 'ima_usage' && t.name !== 'ima_detail');
  assert.strictEqual(apiTools.length, 16);
  for (const t of apiTools) {
    assert.ok(typeof t.apiPath === 'string' && t.apiPath.startsWith('openapi/'), t.name + ' apiPath: ' + t.apiPath);
    assert.strictEqual(t.inputSchema.type, 'object');
    assert.ok(t.inputSchema.properties && t.inputSchema.required);
  }
});

test('写操作工具 readonly=false 覆盖 append_doc/import_doc/create_media/add_knowledge/import_urls', () => {
  const writes = ['ima_append_doc', 'ima_import_doc', 'ima_create_media', 'ima_add_knowledge', 'ima_import_urls'];
  for (const n of writes) {
    const t = TOOL_DEFS.find((x) => x.name === n);
    assert.strictEqual(t.readonly, false, n + ' must not be readonly');
  }
  const reads = TOOL_DEFS.filter((t) => t.readonly === true).map((t) => t.name);
  assert.strictEqual(reads.length, 13, 'readonly count: ' + reads.join(','));
});

test('能力面工具无 apiPath（本地文档面）', () => {
  for (const n of ['ima_usage', 'ima_detail']) {
    const t = TOOL_DEFS.find((x) => x.name === n);
    assert.strictEqual(t.apiPath, null);
  }
});

// ─────────────── 单元层：validateArgs ───────────────

test('validateArgs：缺必填报错、类型不符报错、enum 越界报错、合法通过', () => {
  const schemaObj = {
    type: 'object',
    properties: {
      a: { type: 'string' },
      b: { type: 'integer' },
      m: { type: 'string', enum: ['notes', 'knowledge-base'] },
      arr: { type: 'array' },
    },
    required: ['a'],
  };
  assert.ok(validateArgs({}, schemaObj).includes('a'));
  assert.ok(validateArgs({ a: 1 }, schemaObj).includes('a'));
  assert.ok(validateArgs({ a: 'x', b: 'not-int' }, schemaObj).includes('b'));
  assert.ok(validateArgs({ a: 'x', m: 'nope' }, schemaObj).includes('m'));
  assert.ok(validateArgs({ a: 'x', arr: {} }, schemaObj).includes('arr'));
  assert.strictEqual(validateArgs({ a: 'x', b: 2, m: 'notes', arr: [1] }, schemaObj), null);
  assert.strictEqual(validateArgs({ a: 'x' }, schemaObj), null, '可选参数缺省放行');
});

test('validateArgs：未知参数放行（官方接口字段演进容错），required null 值拒绝', () => {
  const schemaObj = { type: 'object', properties: { a: { type: 'string' } }, required: ['a'] };
  assert.strictEqual(validateArgs({ a: 'x', unknown_field: 123 }, schemaObj), null);
  assert.ok(validateArgs({ a: null }, schemaObj).includes('a'));
});

// ─────────────── 单元层：白名单文档读取与防穿越 ───────────────

test('readWhitelistedDoc：白名单内可读，返回文件全文', () => {
  const text = readWhitelistedDoc('SKILL.md');
  assert.ok(text.includes('fixture root SKILL'));
});

test('readWhitelistedDoc：非白名单/穿越/绝对路径/空值全部拒绝', () => {
  assert.throws(() => readWhitelistedDoc('meta.json'), /白名单/);
  assert.throws(() => readWhitelistedDoc('../../etc/passwd'), /非法文档路径|白名单/);
  assert.throws(() => readWhitelistedDoc('/etc/passwd'), /非法文档路径|白名单/);
  assert.throws(() => readWhitelistedDoc(''), /缺失/);
  assert.throws(() => readWhitelistedDoc(null), /缺失/);
  assert.throws(() => readWhitelistedDoc('SKILL.md/../../server.js'), /非法文档路径|白名单|越界/);
});

test('buildManifestLines：五件文档全量在列且带详情 URI', () => {
  const m = buildManifestLines();
  assert.strictEqual(CONFIG.docs.length, 5);
  for (const d of CONFIG.docs) {
    assert.ok(m.includes(d.name), 'manifest missing: ' + d.name);
    assert.ok(m.includes('ima://skill/' + d.rel), 'manifest missing URI: ' + d.rel);
  }
});

// ─────────────── 协议层：ndjson stdio 驱动 ───────────────

function rpc(reqs) {
  const input = reqs.map((r) => JSON.stringify(r)).join('\n') + '\n';
  const res = spawnSync('node', [SERVER], {
    input,
    env: Object.assign({}, process.env, { IMA_SKILL_INSTALL_DIR: FIXTURES }),
    timeout: 120000,
    encoding: 'utf8',
  });
  assert.strictEqual(res.status, 0, 'server exit=' + res.status + ' stderr=' + (res.stderr || ''));
  const lines = (res.stdout || '').trim().split('\n').filter(Boolean);
  const frames = lines.map((l) => JSON.parse(l));
  return { frames, stderr: res.stderr || '' };
}

function withInit(reqs) {
  return [{ jsonrpc: '2.0', id: 0, method: 'initialize', params: {} }].concat(reqs);
}

function frameById(frames, id) {
  return frames.find((f) => f.id === id);
}

test('协议：initialize 返回协议版本与能力声明', () => {
  const { frames } = rpc(withInit([]));
  const r = frameById(frames, 0).result;
  assert.strictEqual(r.protocolVersion, '2025-06-18');
  assert.ok(r.capabilities.tools);
  assert.ok(r.capabilities.resources);
  assert.strictEqual(r.serverInfo.name, 'ima-mcp-proxy');
});

test('协议：ping 返回空结果；未知通知零回复', () => {
  const { frames } = rpc(withInit([
    { jsonrpc: '2.0', id: 1, method: 'ping' },
    { jsonrpc: '2.0', method: 'notifications/initialized' },
  ]));
  assert.deepStrictEqual(frameById(frames, 1).result, {});
  assert.strictEqual(frames.length, 2, 'notification must not produce a frame');
});

test('协议：tools/list 返回 18 工具且 schema 结构完整', () => {
  const { frames } = rpc(withInit([{ jsonrpc: '2.0', id: 2, method: 'tools/list' }]));
  const tools = frameById(frames, 2).result.tools;
  assert.strictEqual(tools.length, 18);
  for (const t of tools) {
    assert.ok(t.name && t.description && t.inputSchema);
    assert.strictEqual(typeof t.annotations.readOnlyHint, 'boolean');
  }
  const byName = {};
  for (const t of tools) byName[t.name] = t;
  assert.strictEqual(byName.ima_append_doc.annotations.readOnlyHint, false);
  assert.strictEqual(byName.ima_search_note.annotations.readOnlyHint, true);
  assert.strictEqual(byName.ima_usage.annotations.readOnlyHint, true);
});

test('协议：tools/call ima_usage 返回清单+说明书（能力面方法①）', () => {
  const { frames } = rpc(withInit([{ jsonrpc: '2.0', id: 3, method: 'tools/call', params: { name: 'ima_usage', arguments: {} } }]));
  const r = frameById(frames, 3).result;
  assert.ok(!r.isError);
  const text = r.content[0].text;
  assert.ok(text.includes('技能清单'));
  assert.ok(text.includes('ima://skill/SKILL.md'));
  assert.ok(text.includes('fixture root SKILL'));
});

test('协议：tools/call ima_usage module=notes 返回子模块说明书', () => {
  const { frames } = rpc(withInit([{ jsonrpc: '2.0', id: 4, method: 'tools/call', params: { name: 'ima_usage', arguments: { module: 'notes' } } }]));
  const text = frameById(frames, 4).result.content[0].text;
  assert.ok(text.includes('fixture notes'));
});

test('协议：tools/call ima_detail 返回指定文档全文（能力面方法②）', () => {
  const { frames } = rpc(withInit([{ jsonrpc: '2.0', id: 5, method: 'tools/call', params: { name: 'ima_detail', arguments: { doc: 'knowledge-base/references/api.md' } } }]));
  const text = frameById(frames, 5).result.content[0].text;
  assert.ok(text.includes('fixture kb api'));
});

test('协议：tools/call ima_detail 非白名单文档报错（enum schema 第一道防线 + isError）', () => {
  const { frames } = rpc(withInit([{ jsonrpc: '2.0', id: 6, method: 'tools/call', params: { name: 'ima_detail', arguments: { doc: 'meta.json' } } }]));
  const r = frameById(frames, 6).result;
  assert.ok(r.isError);
  const msg = r.content[0].text;
  assert.ok(msg.includes('参数校验失败') || msg.includes('白名单'), 'reject message: ' + msg);
  assert.ok(msg.includes('meta.json'), 'must mention the rejected doc: ' + msg);
});

test('协议：业务面 stub ok 分支——spawn 转发 body 与 apiPath（fake ok）', () => {
  const { frames } = rpc(withInit([{
    jsonrpc: '2.0', id: 7, method: 'tools/call',
    params: { name: 'ima_search_note', arguments: { query_info: { title: '会议' }, start: 0, end: 20 } },
  }]));
  const r = frameById(frames, 7).result;
  assert.ok(!r.isError, r.content && r.content[0].text);
  const data = JSON.parse(r.content[0].text);
  assert.strictEqual(data.code, 0);
  assert.strictEqual(data.data.api_path, 'openapi/note/v1/search_note');
  assert.strictEqual(data.data.echo.query_info.title, '会议');
  assert.strictEqual(data.data.echo.start, 0);
  assert.strictEqual(data.data.echo.end, 20);
});

test('协议：业务面 stub error 分支——-100 结构化错误透出', () => {
  const { frames } = rpc(withInit([{
    jsonrpc: '2.0', id: 8, method: 'tools/call',
    params: { name: 'ima_get_doc_content', arguments: { note_id: 'n1', target_content_format: 0, fake_mode: 'error' } },
  }]));
  const r = frameById(frames, 8).result;
  assert.ok(r.isError);
  const err = JSON.parse(r.content[0].text);
  assert.strictEqual(err.code, -100);
  assert.ok(err.msg.includes('missing credentials'));
});

test('协议：业务面 stub update 分支——-200 时 stdout 更新上下文随错误透出（spawn 隔离验证）', () => {
  const { frames } = rpc(withInit([{
    jsonrpc: '2.0', id: 9, method: 'tools/call',
    params: { name: 'ima_list_note', arguments: { cursor: '', limit: 20, fake_mode: 'update' } },
  }]));
  const r = frameById(frames, 9).result;
  assert.ok(r.isError);
  const err = JSON.parse(r.content[0].text);
  assert.strictEqual(err.code, -200);
  assert.ok(err.update_context);
  assert.strictEqual(err.update_context.latest_version, '1.1.10');
});

test('协议：tools/call 参数校验失败返 isError 不触子进程', () => {
  const { frames } = rpc(withInit([
    { jsonrpc: '2.0', id: 10, method: 'tools/call', params: { name: 'ima_append_doc', arguments: { note_id: 'n1' } } },
    { jsonrpc: '2.0', id: 11, method: 'tools/call', params: { name: 'no_such_tool', arguments: {} } },
  ]));
  const r10 = frameById(frames, 10).result;
  assert.ok(r10.isError);
  assert.ok(r10.content[0].text.includes('参数校验失败'));
  const r11 = frameById(frames, 11).result;
  assert.ok(r11.isError);
  assert.ok(r11.content[0].text.includes('未知工具'));
});

test('协议：resources/list 返回 5 件 ima://skill/ 资源', () => {
  const { frames } = rpc(withInit([{ jsonrpc: '2.0', id: 12, method: 'resources/list' }]));
  const resources = frameById(frames, 12).result.resources;
  assert.strictEqual(resources.length, 5);
  for (const r of resources) {
    assert.ok(r.uri.startsWith('ima://skill/'));
    assert.strictEqual(r.mimeType, 'text/markdown');
  }
});

test('协议：resources/read 白名单命中返全文；穿越返 -32602', () => {
  const { frames } = rpc(withInit([
    { jsonrpc: '2.0', id: 13, method: 'resources/read', params: { uri: 'ima://skill/notes/SKILL.md' } },
    { jsonrpc: '2.0', id: 14, method: 'resources/read', params: { uri: 'ima://skill/../server.js' } },
    { jsonrpc: '2.0', id: 15, method: 'resources/read', params: { uri: 'file:///etc/passwd' } },
  ]));
  const r13 = frameById(frames, 13).result;
  assert.strictEqual(r13.contents[0].uri, 'ima://skill/notes/SKILL.md');
  assert.ok(r13.contents[0].text.includes('fixture notes'));
  for (const id of [14, 15]) {
    const e = frameById(frames, id).error;
    assert.ok(e && e.code === -32602, 'expect -32602 for id ' + id);
  }
});

test('启动校验：安装目录缺映射目标时 fail-fast 退出非 0', () => {
  const res = spawnSync('node', [SERVER], {
    input: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'initialize', params: {} }) + '\n',
    env: Object.assign({}, process.env, { IMA_SKILL_INSTALL_DIR: path.join(FIXTURES, 'not-exist') }),
    timeout: 30000,
    encoding: 'utf8',
  });
  assert.notStrictEqual(res.status, 0);
  assert.ok((res.stderr || '').includes('映射一致性校验失败'));
});

test('凭证纪律：server 零真实凭证值（无 32+ 位 hex/长随机字面量；凭证仅经 env 透传）', () => {
  const fs = require('node:fs');
  const srcCode = fs.readFileSync(SERVER, 'utf8');
  // 真实凭证典型形态：32+ 位连续 hex（client_id/api_key 均为此形态）——源码零命中
  const hexLiterals = srcCode.match(/[0-9a-f]{32,}/g) || [];
  assert.deepStrictEqual(hexLiterals, [], 'no credential-shaped literals allowed: ' + hexLiterals.slice(0, 3).join(','));
  // 凭证绑定声明恰两键，且值只从 process.env 读取（透传零变换）
  assert.deepStrictEqual(CONFIG.credentials.map((c) => c.env), ['IMA_CLIENT_ID', 'IMA_API_KEY']);
  assert.ok(srcCode.includes('process.env.IMA_CLIENT_ID') && srcCode.includes('process.env.IMA_API_KEY'));
  // 不落 argv：spawn argv 只有 node/entry/apiPath/body/options 四段构造，无凭证拼接
  assert.ok(!srcCode.includes("IMA_CLIENT_ID,") || srcCode.includes('IMA_CLIENT_ID: process.env.IMA_CLIENT_ID'));
});
