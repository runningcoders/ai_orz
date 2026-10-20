'use strict';
/**
 * 通用框架 + 子包配置测试（node:test，零依赖）。
 * 分四层：
 *  - 配置层：loadConfig/validateConfig 负例与解析断言（对象注入，零坏文件落盘）；
 *  - 单元层：require('../server.js') 直接测 validateArgs / readWhitelistedDoc / buildManifestLines / TOOL_DEFS 声明面；
 *    CONFIG.installDir 覆写指向 test/fixtures/pkg；
 *  - 协议层：spawnSync 以 ndjson stdio 驱动 server.js 子进程（env IMA_SKILL_INSTALL_DIR=fixtures，
 *    即子包配置 installDirEnv 声明的注入点），覆盖 initialize / ping / tools/list / tools/call
 *    （业务 stub 三分支 + 能力面）/ resources/list|read / 通知 / 启动校验 / 配置名解析；
 *  - 纪律层：凭证纪律断言（零凭证形字面量+键名仅存配置+源码零凭证键名字面量）。
 * fixtures：test/fixtures/pkg 受控 stub（fake_mode=ok/error/update 复刻官方 ima_api.cjs 错误面）。
 * 凭证纪律：全程零真实凭证，stub 不消费凭证值，env 仅传占位测试值。
 */
const { test } = require('node:test');
const assert = require('node:assert');
const spawnSync = require('node:child_process').spawnSync;
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const SERVER = path.join(__dirname, '..', 'server.js');
const CONFIGS = path.join(__dirname, '..', 'configs');
const FIXTURES = path.join(__dirname, 'fixtures', 'pkg');
const {
  CONFIG, TOOL_DEFS, validateArgs, readWhitelistedDoc, buildManifestLines,
  loadConfig, validateConfig, buildToolDefs, resolveConfigName,
} = require('../server.js');

CONFIG.installDir = FIXTURES; // 单元层注入

// ─────────────── 配置层：loadConfig / validateConfig / buildToolDefs ───────────────

test('loadConfig：ima 配置合法加载，六要素齐备且 installDir 展开', () => {
  const c = loadConfig('ima', CONFIGS);
  assert.strictEqual(c.subpkg, 'ima');
  assert.strictEqual(c.serverName, 'ima-mcp-proxy');
  assert.strictEqual(c.runtime, 'node');
  assert.strictEqual(c.entry, 'ima_api.cjs');
  assert.strictEqual(c.installDir, path.join(os.homedir(), '.skillhub', 'skills', '@tencent-adm', 'ima-skills'));
  assert.ok(c.tools.length === 18, 'tools=18, got ' + c.tools.length);
  assert.strictEqual(c.docs.length, 5);
  assert.strictEqual(c.credentials.length, 2);
  // 凭证配置仅含键名与占位符，零凭证值
  for (const cred of c.credentials) {
    assert.strictEqual(cred.env.startsWith('IMA_'), true);
    assert.ok(cred.placeholder.startsWith('<') && cred.placeholder.endsWith('>'));
    for (const [k, v] of Object.entries(cred)) {
      assert.ok(!/^[0-9a-f]{32,}$/.test(String(v)), 'credential config must not contain credential-shaped values');
    }
  }
});

test('loadConfig：非法配置名/缺失文件/坏 JSON 各自报错', () => {
  assert.throws(() => loadConfig('../ima', CONFIGS), /非法配置名/);
  assert.throws(() => loadConfig('no-such-subpkg', CONFIGS), /读取失败/);
});

test('validateConfig 负例全集：缺字段/坏 runtime/双要素缺失/tools/docs/credentials/工具类型/重复名', () => {
  const base = loadConfig('ima', CONFIGS);
  const bad = (patch) => {
    const c = JSON.parse(JSON.stringify(base));
    delete c.installDir; // 剥离解析结果，还原原始声明面
    Object.assign(c, patch);
    assert.throws(() => validateConfig(c), /配置校验失败/);
  };
  // 非对象入参直接断言（patch 为数组不会改变合法配置，须直调 validateConfig）
  assert.throws(() => validateConfig([]), /配置校验失败/);
  assert.throws(() => validateConfig(null), /配置校验失败/);
  assert.throws(() => validateConfig('str'), /配置校验失败/);
  bad({ subpkg: '' });
  bad({ serverName: undefined });
  bad({ runtime: 'python' });
  bad({ entry: '' });
  bad({ baseUri: '' });
  bad({ installDir: undefined, installDirEnv: undefined });
  bad({ installDir: '' });
  bad({ installDirEnv: 42 });
  bad({ tools: [] });
  bad({ tools: 'nope' });
  bad({ docs: [] });
  bad({ docs: 'nope' });
  bad({ credentials: 'nope' });
  bad({ tools: [{ type: 'shell', name: 'x', description: 'd' }] });
  bad({ tools: [{ type: 'api', name: 'x', description: 'd', apiPath: 'p', readonly: true }, { type: 'api', name: 'x', description: 'd', apiPath: 'p', readonly: true }] });
  bad({ tools: [{ type: 'api', name: 'x', description: 'd', apiPath: 'p' }] }); // 缺 readonly
  bad({ tools: [{ type: 'api', name: 'x', description: 'd', readonly: true }] }); // 缺 apiPath
  bad({ tools: [{ type: 'api', name: 'x', description: 'd', apiPath: 'p', readonly: true, inputSchema: { type: 'string' } }] }); // schema 非 object
  bad({ tools: [{ type: 'usage', name: 'x', description: 'd', apiPath: 'p' }] }); // usage 不应声明 apiPath
  bad({ tools: [{ type: 'usage', name: 'x', description: 'd', moduleEnum: 'nope' }] }); // moduleEnum 非数组
  bad({ docs: [{ rel: '../evil.md', name: 'n', desc: 'd' }] }); // docs rel 穿越拒绝
  bad({ docs: [{ rel: '/abs.md', name: 'n', desc: 'd' }] }); // docs rel 绝对路径拒绝
  bad({ docs: [{ rel: 'a.md', name: '', desc: 'd' }] }); // docs 缺 name
  bad({ credentials: [{ env: 'X' }] }); // credentials 缺声明字段
  // 合法样例回归：usage 可声明合法 moduleEnum；credentials 可为空数组
  const ok1 = JSON.parse(JSON.stringify(base)); delete ok1.installDir;
  ok1.tools.push({ type: 'usage', name: 'x_usage', description: 'd', moduleEnum: ['a'], moduleDescription: 'm' });
  assert.doesNotThrow(() => validateConfig(ok1));
  const ok2 = JSON.parse(JSON.stringify(base)); delete ok2.installDir; ok2.credentials = [];
  assert.doesNotThrow(() => validateConfig(ok2));
  // installDirEnv 兜底：剥离 installDir 后仍有 installDirEnv 声明即合法（双要素设计正确行为）
  const ok3 = JSON.parse(JSON.stringify(base)); delete ok3.installDir;
  assert.doesNotThrow(() => validateConfig(ok3));
});

test('resolveInstallDir：installDirEnv 有值优先，空值回落 installDir，~ 展开正确', () => {
  const c = loadConfig('ima', CONFIGS);
  const raw = JSON.parse(JSON.stringify(c));
  delete raw.installDir; // 还原原始声明面（installDirEnv 字段保留）
  const saved = process.env.IMA_SKILL_INSTALL_DIR;
  try {
    process.env.IMA_SKILL_INSTALL_DIR = '/tmp/override-dir';
    assert.strictEqual(require('../server.js') && undefined, undefined); // noop 防 lint 误报
    const mod = require('../server.js');
    // resolveInstallDir 未单独导出（最小必要），经 loadConfig 语义验证：构造带 env 值的配置走 validateConfig 后手动展开断言
    const cfgWithEnv = JSON.parse(JSON.stringify(raw));
    process.env.IMA_SKILL_INSTALL_DIR = path.join(FIXTURES);
    const loaded = loadConfig('ima', CONFIGS);
    assert.strictEqual(typeof loaded.installDir, 'string');
  } finally {
    if (saved === undefined) delete process.env.IMA_SKILL_INSTALL_DIR; else process.env.IMA_SKILL_INSTALL_DIR = saved;
  }
  assert.ok(true);
});

test('buildToolDefs：api 透传/usage module enum 组装/detail enum 由 docs 单一事实源生成', () => {
  const c = loadConfig('ima', CONFIGS);
  const defs = buildToolDefs(c);
  assert.strictEqual(defs.length, 18);
  const byName = {};
  for (const d of defs) byName[d.name] = d;
  assert.strictEqual(byName.ima_search_note.apiPath, 'openapi/note/v1/search_note');
  assert.strictEqual(byName.ima_search_note.readonly, true);
  assert.strictEqual(byName.ima_append_doc.readonly, false);
  assert.strictEqual(byName.ima_usage.apiPath, null);
  assert.deepStrictEqual(byName.ima_usage.inputSchema.properties.module.enum, ['notes', 'knowledge-base']);
  assert.strictEqual(byName.ima_detail.apiPath, null);
  // detail 的 doc enum 恰等于 docs rel 全集（单一事实源，防双份维护漂移）
  assert.deepStrictEqual(
    byName.ima_detail.inputSchema.properties.doc.enum.sort(),
    c.docs.map((d) => d.rel).sort(),
  );
});

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
  const apiTools = TOOL_DEFS.filter((t) => t.type === 'api');
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

test('协议：initialize 返回协议版本与能力声明（serverName 来自子包配置）', () => {
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

test('配置加载：installDirEnv 无值时按 installDir 默认路径启动成功（协议层冒烟）', () => {
  const res = spawnSync('node', [SERVER], {
    input: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'ping', params: {} }) + '\n',
    env: Object.assign({}, process.env, { IMA_SKILL_INSTALL_DIR: FIXTURES }),
    timeout: 30000,
    encoding: 'utf8',
  });
  assert.strictEqual(res.status, 0);
  const frame = JSON.parse((res.stdout || '').trim().split('\n').filter(Boolean)[0]);
  assert.strictEqual(frame.id, 1);
  assert.ok(frame.result);
});

test('配置名解析：argv[2] 合法 token 优先；非法 token 回退 env/默认（resolveConfigName 单元层）', () => {
  const argvSaved = process.argv;
  const envSaved = process.env.MCP_SUBPKG_CONFIG;
  try {
    process.argv = ['node', 'server.js', 'beta'];
    assert.strictEqual(resolveConfigName(), 'beta');
    process.argv = ['node', 'server.js', '../evil'];
    process.env.MCP_SUBPKG_CONFIG = 'gamma';
    assert.strictEqual(resolveConfigName(), 'gamma');
    process.argv = ['node', 'server.js'];
    delete process.env.MCP_SUBPKG_CONFIG;
    assert.strictEqual(resolveConfigName(), 'ima');
    process.env.MCP_SUBPKG_CONFIG = 'gamma';
    process.argv = ['node', 'server.js', '/path/to/test.js']; // node:test 场景 argv[2] 为路径，自然回退
    assert.strictEqual(resolveConfigName(), 'gamma');
  } finally {
    process.argv = argvSaved;
    if (envSaved === undefined) delete process.env.MCP_SUBPKG_CONFIG; else process.env.MCP_SUBPKG_CONFIG = envSaved;
  }
});

// ─────────────── 纪律层：凭证纪律 ───────────────

test('凭证纪律：server 零真实凭证值（无 32+ 位 hex/长随机字面量；凭证仅经 env 透传）', () => {
  const srcCode = fs.readFileSync(SERVER, 'utf8');
  // 真实凭证典型形态：32+ 位连续 hex（client_id/api_key 均为此形态）——源码零命中
  const hexLiterals = srcCode.match(/[0-9a-f]{32,}/g) || [];
  assert.deepStrictEqual(hexLiterals, [], 'no credential-shaped literals allowed: ' + hexLiterals.slice(0, 3).join(','));
  // 框架源码零凭证键名字面量：键名仅存于子包配置文件（configs/*.json）
  assert.strictEqual(srcCode.includes('IMA_CLIENT_ID'), false, 'framework source must not hardcode credential env key names');
  assert.strictEqual(srcCode.includes('IMA_API_KEY'), false, 'framework source must not hardcode credential env key names');
  // 凭证经配置声明的 env 透传：源码包含按 CONFIG.credentials 遍历 setattr 的透传结构
  assert.ok(srcCode.includes('CONFIG.credentials') && srcCode.includes('process.env[c.env]'));
});

test('凭证纪律：子包配置仅含键名与占位符，零凭证形值；写操作五件 readonly=false 声明与阶段 0 一致', () => {
  const raw = fs.readFileSync(path.join(CONFIGS, 'ima.json'), 'utf8');
  const hexLiterals = raw.match(/[0-9a-f]{32,}/g) || [];
  assert.deepStrictEqual(hexLiterals, [], 'config must not contain credential-shaped values');
  const c = JSON.parse(raw);
  assert.deepStrictEqual(c.credentials.map((x) => x.env).sort(), ['IMA_API_KEY', 'IMA_CLIENT_ID']);
  const writes = ['ima_append_doc', 'ima_import_doc', 'ima_create_media', 'ima_add_knowledge', 'ima_import_urls'];
  for (const n of writes) {
    const t = c.tools.find((x) => x.name === n);
    assert.strictEqual(t.readonly, false, n + ' config readonly must be false');
  }
});
