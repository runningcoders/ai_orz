#!/usr/bin/env node
'use strict';
/**
 * MCP 通用子包代理框架（阶段 1）—— 配置外置驱动子包加载，ima 为首个子系统实例
 * （承阶段 0 单包封装 commit=5bc379bd；唯一执行依据：评估工件 01a157ce-e111 v2 终态 §2.3/§2.7）
 *
 * 架构口径（AMan 通用化定调 msg 01a15803 原话：「根据不同的配置文件加载不同的子包」）：
 * 1. 零依赖 Node stdio MCP server（node >= 18）。本文件=通用框架（配置加载+协议处理+三类工具执行器），
 *    子包差异全部外置到 configs/<subpkg>.json —— 新子包接入=新增一个配置文件，框架零改动。
 * 2. 配置六要素（评估 v2 §2.3）：subpkg/source/install_dir/runtime/entry/tools（含 credentials 与 resources 面 docs）。
 *    加载链：resolveConfigName（argv[2] 安全 token 优先 → env MCP_SUBPKG_CONFIG → 默认 'ima'）
 *    → loadConfig（读 configs/<name>.json + 结构校验 fail-fast + installDir 解析）
 *    → checkMappingOrExit（入口脚本+全部白名单文档存在性，缺件退出非 0）→ 主循环。
 * 3. tools 三类型（本单实现面，第二子包出现时再校准抽象，严禁预先虚构）：
 *    - api：spawn 子进程执行 entry 脚本（argv=[apiPath, bodyJson, optionsJson]），stdout=响应/stderr=结构化错误；
 *    - usage（能力面方法①）：清单自动生成+说明书全文（module 参数可选，读 <module>/SKILL.md）；
 *    - detail（能力面方法②）：读白名单文档全文（doc 参数 enum 由 docs 单一事实源生成）。
 * 4. spawn 隔离为结构性必需（阶段 0 定谳）：官方脚本在 -200（skill 需更新）时直接 process.stdout.write
 *    更新上下文——require 进本进程会污染 stdio JSON-RPC 通道，spawn 天然隔离。
 * 5. 凭证链路：平台经 Stdio+Env binding 按配置 credentials 声明向本进程注入 env（键名仅存于配置文件，
 *    本框架源码零凭证键名字面量、零凭证值），本进程按声明透传给子进程；argv 只携带 apiPath 与 body。
 *    凭证零明文：configs/*.json 仅含键名与占位符（占位符口径见配置文件 placeholder 字段）。
 * 6. 安全边界：resources/能力面读取=文档白名单（配置 docs）+ URI 前缀 + 安装目录前缀双重校验防路径穿越。
 * 7. sync/主动同步/分发面：零字段零实现（AMan 定调③，分发面不做）。
 */

const { spawn } = require('child_process');
const fs = require('fs');
const os = require('os');
const path = require('path');

const SERVER_VERSION = '0.2.0';
const PROTOCOL_VERSION = '2025-06-18';
const CHILD_TIMEOUT_MS = 60_000;
const CONFIGS_DIR = path.join(__dirname, 'configs');
const CONFIG_NAME_RE = /^[A-Za-z0-9_-]+$/;

const s = (type, description, extra) => Object.assign({ type, description }, extra || {});
const schema = (properties, required) => ({ type: 'object', properties: properties || {}, required: required || [] });

// ─────────────────────────── 配置加载（阶段 1 核心） ───────────────────────────

/** 配置名解析：argv[2] 合法 token 优先（显式指定最强），其次 env MCP_SUBPKG_CONFIG，默认 'ima'。
 *  node:test 场景 argv[2] 为测试文件路径（含路径分隔符，不匹配 token）自然回退，单测零冲突。 */
function resolveConfigName() {
  const arg = process.argv[2];
  if (arg && CONFIG_NAME_RE.test(arg)) return arg;
  const envName = process.env.MCP_SUBPKG_CONFIG;
  if (envName && CONFIG_NAME_RE.test(envName)) return envName;
  return 'ima';
}

function expandHome(p) {
  if (p === '~') return os.homedir();
  if (p.startsWith('~/')) return path.join(os.homedir(), p.slice(2));
  return p;
}

/** 配置结构校验（纯函数，非法即抛错；单元层直接对配置对象断言负例，零坏文件落盘） */
function validateConfig(cfg) {
  const bad = (msg) => { throw new Error('配置校验失败：' + msg); };
  if (!cfg || typeof cfg !== 'object' || Array.isArray(cfg)) bad('配置须为 JSON 对象');
  for (const key of ['subpkg', 'serverName', 'runtime', 'entry', 'baseUri']) {
    if (typeof cfg[key] !== 'string' || !cfg[key]) bad('缺少必填字符串字段 ' + key);
  }
  if (cfg.runtime !== 'node') bad('runtime 暂仅支持 "node"（实际：' + cfg.runtime + '）；其他执行器候第二子包出现时校准');
  if (!cfg.installDir && !cfg.installDirEnv) bad('installDir 与 installDirEnv 至少声明其一');
  if (cfg.installDir !== undefined && (typeof cfg.installDir !== 'string' || !cfg.installDir)) bad('installDir 须为非空字符串');
  if (cfg.installDirEnv !== undefined && (typeof cfg.installDirEnv !== 'string' || !cfg.installDirEnv)) bad('installDirEnv 须为非空字符串');
  if (!Array.isArray(cfg.tools) || cfg.tools.length === 0) bad('tools 须为非空数组');
  if (!Array.isArray(cfg.docs) || cfg.docs.length === 0) bad('docs（文档白名单）须为非空数组');
  if (!Array.isArray(cfg.credentials)) bad('credentials 须为数组（可为空）');
  const seenNames = new Set();
  for (const t of cfg.tools) {
    if (!t || typeof t !== 'object') bad('tools 项须为对象');
    if (typeof t.name !== 'string' || !t.name) bad('工具缺少 name');
    if (seenNames.has(t.name)) bad('工具名重复：' + t.name);
    seenNames.add(t.name);
    if (typeof t.description !== 'string' || !t.description) bad('工具 ' + t.name + ' 缺少 description');
    if (t.type === 'api') {
      if (typeof t.apiPath !== 'string' || !t.apiPath) bad('api 工具 ' + t.name + ' 缺少 apiPath');
      if (!t.inputSchema || t.inputSchema.type !== 'object') bad('api 工具 ' + t.name + ' inputSchema 须为 object 类型声明');
      if (typeof t.readonly !== 'boolean') bad('api 工具 ' + t.name + ' 缺少 readonly 布尔声明');
    } else if (t.type === 'usage') {
      if ('apiPath' in t) bad('usage 工具 ' + t.name + ' 不应声明 apiPath');
      if (t.moduleEnum !== undefined && (!Array.isArray(t.moduleEnum) || t.moduleEnum.some((m) => typeof m !== 'string'))) {
        bad('usage 工具 ' + t.name + ' moduleEnum 须为字符串数组');
      }
    } else if (t.type === 'detail') {
      if ('apiPath' in t) bad('detail 工具 ' + t.name + ' 不应声明 apiPath');
    } else {
      bad('工具 ' + t.name + ' 类型不支持：' + t.type + '（当前支持 api/usage/detail）');
    }
  }
  for (const d of cfg.docs) {
    if (!d || typeof d !== 'object') bad('docs 项须为对象');
    if (typeof d.rel !== 'string' || !d.rel || d.rel.includes('..') || d.rel.startsWith('/')) bad('docs 项 rel 非法：' + JSON.stringify(d.rel));
    if (typeof d.name !== 'string' || !d.name || typeof d.desc !== 'string' || !d.desc) bad('docs 项 ' + d.rel + ' 缺少 name/desc');
  }
  for (const c of cfg.credentials) {
    if (!c || typeof c !== 'object') bad('credentials 项须为对象');
    for (const key of ['env', 'platformKind', 'platform', 'placeholder']) {
      if (typeof c[key] !== 'string' || !c[key]) bad('credentials 项缺少 ' + key + '（配置仅声明键名与占位符，严禁出现凭证值）');
    }
  }
}

/** installDir 解析：installDirEnv 指向的进程环境变量优先（测试/多环境注入点），其次 installDir（支持 ~ 展开） */
function resolveInstallDir(cfg) {
  const fromEnv = cfg.installDirEnv ? process.env[cfg.installDirEnv] : undefined;
  const raw = (fromEnv && fromEnv.trim()) || cfg.installDir;
  if (!raw) {
    throw new Error('配置校验失败：installDir 解析为空（installDirEnv=' + (cfg.installDirEnv || '') + ' 无值且 installDir 未声明）');
  }
  return expandHome(raw);
}

/** 加载子包配置：读 configs/<name>.json → JSON.parse → 结构校验 → installDir 解析。
 *  name/dir 可注入（单元层负例零坏文件）；失败抛错，由顶层捕获 logErr+exit(1)。 */
function loadConfig(name, dir) {
  if (!name || !CONFIG_NAME_RE.test(name)) throw new Error('非法配置名：' + JSON.stringify(name) + '（仅允许 [A-Za-z0-9_-]）');
  const file = path.join(dir || CONFIGS_DIR, name + '.json');
  let raw;
  try {
    raw = fs.readFileSync(file, 'utf8');
  } catch (e) {
    throw new Error('子包配置读取失败：' + file + '（' + e.message + '）');
  }
  let cfg;
  try {
    cfg = JSON.parse(raw);
  } catch (e) {
    throw new Error('子包配置非法 JSON：' + file + '（' + e.message + '）');
  }
  validateConfig(cfg);
  cfg.installDir = resolveInstallDir(cfg);
  cfg.source = cfg.source || null;
  return cfg;
}

/** 由配置构建 tools 映射表：api 透传声明；usage/detail 框架内置行为+schema 组装。
 *  detail 的 doc enum 由 docs 单一事实源生成（防双份维护漂移）。 */
function buildToolDefs(cfg) {
  return cfg.tools.map((t) => {
    if (t.type === 'api') {
      return {
        type: 'api', name: t.name, description: t.description,
        apiPath: t.apiPath, readonly: t.readonly, inputSchema: t.inputSchema,
      };
    }
    if (t.type === 'usage') {
      const props = {};
      if (Array.isArray(t.moduleEnum) && t.moduleEnum.length > 0) {
        props.module = s('string', t.moduleDescription || '子模块（省略=总使用说明）', { enum: t.moduleEnum });
      }
      return { type: 'usage', name: t.name, description: t.description, apiPath: null, readonly: true, inputSchema: schema(props, []) };
    }
    return {
      type: 'detail', name: t.name, description: t.description, apiPath: null, readonly: true,
      inputSchema: schema({ doc: s('string', '文档标识（与技能列表返回的详情 URI 一致）', { enum: cfg.docs.map((d) => d.rel) }) }, ['doc']),
    };
  });
}

// ─────────────── 子包配置装配（顶层加载，fail-fast；测试单元层可覆写 CONFIG.installDir） ───────────────

let CONFIG;
let TOOL_DEFS;
try {
  CONFIG = loadConfig(resolveConfigName());
  TOOL_DEFS = buildToolDefs(CONFIG);
} catch (e) {
  logErr(e.message);
  if (require.main === module) process.exit(1);
  throw e;
}

// ─────────────────────────── 基础设施 ───────────────────────────

function logErr(msg) {
  try { process.stderr.write('[mcp-subpkg-proxy] ' + msg + '\n'); } catch (_) { /* ignore */ }
}

function sendFrame(obj) {
  try { process.stdout.write(JSON.stringify(obj) + '\n'); } catch (e) { logErr('stdout write failed: ' + e.message); }
}

function replyResult(id, result) {
  sendFrame({ jsonrpc: '2.0', id, result });
}

function replyError(id, code, message) {
  sendFrame({ jsonrpc: '2.0', id, error: { code, message } });
}

/** 启动映射一致性校验：入口脚本 + 全部白名单文档存在且可读，缺件 fail-fast（评估 §2.3 教训条款） */
function checkMappingOrExit() {
  const missing = [];
  const entryPath = path.join(CONFIG.installDir, CONFIG.entry);
  if (!fs.existsSync(entryPath)) missing.push(CONFIG.entry);
  for (const d of CONFIG.docs) {
    const p = path.join(CONFIG.installDir, d.rel);
    if (!fs.existsSync(p)) missing.push(d.rel);
  }
  if (missing.length > 0) {
    logErr('映射一致性校验失败：安装目录 ' + CONFIG.installDir + ' 缺失映射目标：' + missing.join(', '));
    process.exit(1);
  }
}

/** 最小入参校验：required 存在性 + 类型 + enum（镜像 LLM 出站 schema 声明，声明稳定口径） */
function validateArgs(args, inputSchema) {
  const props = (inputSchema && inputSchema.properties) || {};
  const required = (inputSchema && inputSchema.required) || [];
  for (const key of required) {
    if (args[key] === undefined || args[key] === null) {
      return '缺少必填参数：' + key;
    }
  }
  const typeCheck = (v, t) => {
    switch (t) {
      case 'string': return typeof v === 'string';
      case 'integer': return typeof v === 'number' && Number.isInteger(v);
      case 'number': return typeof v === 'number' && Number.isFinite(v);
      case 'boolean': return typeof v === 'boolean';
      case 'array': return Array.isArray(v);
      case 'object': return typeof v === 'object' && v !== null && !Array.isArray(v);
      default: return true;
    }
  };
  for (const [key, def] of Object.entries(props)) {
    const v = args[key];
    if (v === undefined) continue;
    if (def && def.type && !typeCheck(v, def.type)) return '参数 ' + key + ' 类型应为 ' + def.type;
    if (def && Array.isArray(def.enum) && !def.enum.includes(v)) {
      return '参数 ' + key + ' 取值须为：' + def.enum.join(' | ') + '（实际收到：' + JSON.stringify(v) + '）';
    }
  }
  return null;
}

/** tools/call 白名单文档读取（usage/detail 与 resources/read 共用） */
function readWhitelistedDoc(rel) {
  if (!rel || typeof rel !== 'string') throw new Error('文档标识缺失');
  if (rel.includes('..') || rel.startsWith('/') || rel.includes('\\')) {
    throw new Error('非法文档路径：' + rel);
  }
  const allowed = CONFIG.docs.find((d) => d.rel === rel);
  if (!allowed) throw new Error('文档不在白名单内：' + rel + '（可用：' + CONFIG.docs.map((d) => d.rel).join(', ') + '）');
  const abs = path.resolve(CONFIG.installDir, rel);
  const base = path.resolve(CONFIG.installDir);
  if (abs !== base && !abs.startsWith(base + path.sep)) {
    throw new Error('路径越界拒绝读取：' + rel);
  }
  return fs.readFileSync(abs, 'utf8');
}

/** 方法①清单自动生成：单一事实源=配置（评估 v2 §2.3 manifest: auto 口径） */
function buildManifestLines() {
  return CONFIG.docs
    .map((d, i) => (i + 1) + '. ' + d.name + ' —— ' + d.desc + '（详情 URI: ' + CONFIG.baseUri + d.rel + '）')
    .join('\n');
}

// ─────────────────────────── entry 子进程调用 ───────────────────────────

/**
 * spawn 子包入口脚本：argv=[entry, apiPath, bodyJson, optionsJson]，凭证经 env 按配置声明透传零 argv 暴露。
 * options 第三参系当前 entry 侧脚本约定（ima_api.cjs 官方模板形态），第二子包若 options 形态不同再配置化。
 * 返回 {ok:true, text} 或 {ok:false, code, msg, updateContext?}。
 */
function callEntryApi(apiPath, body) {
  return new Promise((resolve) => {
    const entryPath = path.join(CONFIG.installDir, CONFIG.entry);
    // env 透传：本进程已由平台按配置 credentials 声明注入（Stdio+Env binding），子进程继承；
    // 显式 setattr 仅做声明性兜底，值永不落日志/argv/磁盘。
    const childEnv = Object.assign({}, process.env);
    for (const c of CONFIG.credentials) childEnv[c.env] = process.env[c.env] || '';
    const child = spawn('node', [entryPath, apiPath, JSON.stringify(body || {}), '{}'], {
      env: childEnv,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let stdout = '';
    let stderr = '';
    let settled = false;
    const timer = setTimeout(() => {
      if (settled) return;
      settled = true;
      try { child.kill('SIGKILL'); } catch (_) { /* ignore */ }
      resolve({ ok: false, code: -100, msg: '子包 API 调用超时（' + CHILD_TIMEOUT_MS + 'ms），请稍后重试' });
    }, CHILD_TIMEOUT_MS);
    child.stdout.on('data', (d) => { stdout += d; });
    child.stderr.on('data', (d) => { stderr += d; });
    child.on('error', (e) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve({ ok: false, code: -100, msg: '子进程启动失败：' + e.message });
    });
    child.on('close', (code) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      if (code === 0) {
        resolve({ ok: true, text: stdout });
        return;
      }
      // 第二层错误（官方口径）：stderr 结构化 JSON {code:-100|-200, msg}
      let err = null;
      try { err = JSON.parse((stderr || '').trim() || '{}'); } catch (_) { /* fallthrough */ }
      const errCode = err && typeof err.code === 'number' ? err.code : -100;
      const errMsg = (err && err.msg) || (stderr || '').trim() || '未知错误（exit=' + code + '）';
      const out = { ok: false, code: errCode, msg: errMsg };
      if (errCode === -200 && stdout.trim()) {
        // -200：更新上下文 JSON 在 stdout（官方口径），透出给 Agent 按 instruction 引导用户
        try { out.updateContext = JSON.parse(stdout.trim()); } catch (_) { out.updateContextRaw = stdout.trim(); }
      }
      resolve(out);
    });
  });
}

// ─────────────────────────── JSON-RPC 方法处理 ───────────────────────────

function handleInitialize(id) {
  replyResult(id, {
    protocolVersion: PROTOCOL_VERSION,
    capabilities: { tools: { listChanged: false }, resources: { subscribe: false, listChanged: false } },
    serverInfo: { name: CONFIG.serverName, version: SERVER_VERSION },
  });
}

function handleToolsList(id) {
  replyResult(id, {
    tools: TOOL_DEFS.map((t) => ({
      name: t.name,
      description: t.description,
      inputSchema: t.inputSchema,
      annotations: { readOnlyHint: !!t.readonly },
    })),
  });
}

async function handleToolsCall(id, params) {
  const name = params && params.name;
  const args = (params && params.arguments) || {};
  const def = TOOL_DEFS.find((t) => t.name === name);
  if (!def) {
    replyResult(id, {
      content: [{ type: 'text', text: JSON.stringify({ code: -100, msg: '未知工具：' + name }) }],
      isError: true,
    });
    return;
  }
  const invalid = validateArgs(args, def.inputSchema);
  if (invalid) {
    replyResult(id, {
      content: [{ type: 'text', text: JSON.stringify({ code: -100, msg: '参数校验失败：' + invalid }) }],
      isError: true,
    });
    return;
  }
  // 能力面方法①（usage）：本地读文档（不 spawn）——清单自动生成+说明书全文
  if (def.type === 'usage') {
    try {
      const rel = args.module ? args.module + '/SKILL.md' : CONFIG.docs[0].rel;
      const text = '【技能清单（manifest，配置自动生成）】\n' + buildManifestLines() +
        '\n\n【使用说明书】(' + rel + ')\n' + readWhitelistedDoc(rel);
      replyResult(id, { content: [{ type: 'text', text }] });
    } catch (e) {
      replyResult(id, { content: [{ type: 'text', text: JSON.stringify({ code: -100, msg: e.message }) }], isError: true });
    }
    return;
  }
  // 能力面方法②（detail）：按白名单读取文档全文
  if (def.type === 'detail') {
    try {
      const text = readWhitelistedDoc(args.doc);
      replyResult(id, { content: [{ type: 'text', text: '【技能详情】(' + args.doc + ')\n' + text }] });
    } catch (e) {
      replyResult(id, { content: [{ type: 'text', text: JSON.stringify({ code: -100, msg: e.message }) }], isError: true });
    }
    return;
  }
  // 业务面（api）：spawn 子包入口脚本（挂 pending 计数：stdin 关闭后仍在飞的调用先完成再退出）
  handleToolsCall.trackPending.inc();
  try {
    await runBusinessTool(id, def, args);
  } finally {
    handleToolsCall.trackPending.dec();
  }
}

async function runBusinessTool(id, def, args) {
  const r = await callEntryApi(def.apiPath, args);
  if (r.ok) {
    replyResult(id, { content: [{ type: 'text', text: r.text }] });
  } else {
    const payload = { code: r.code, msg: r.msg };
    if (r.updateContext) payload.update_context = r.updateContext;
    if (r.updateContextRaw) payload.update_context_raw = r.updateContextRaw;
    replyResult(id, { content: [{ type: 'text', text: JSON.stringify(payload) }], isError: true });
  }
}

function handleResourcesList(id) {
  replyResult(id, {
    resources: CONFIG.docs.map((d) => ({
      uri: CONFIG.baseUri + d.rel,
      name: d.name,
      description: d.desc,
      mimeType: 'text/markdown',
    })),
  });
}

function handleResourcesRead(id, params) {
  const uri = params && params.uri;
  try {
    if (!uri || typeof uri !== 'string' || !uri.startsWith(CONFIG.baseUri)) {
      throw new Error('非法资源 URI：' + uri + '（须以 ' + CONFIG.baseUri + ' 开头）');
    }
    const rel = uri.slice(CONFIG.baseUri.length);
    const text = readWhitelistedDoc(rel);
    replyResult(id, { contents: [{ uri, mimeType: 'text/markdown', text }] });
  } catch (e) {
    replyError(id, -32602, e.message);
  }
}

// ─────────────────────────── 主循环（ndjson stdio）───────────────────────────

function handleMessage(msg) {
  if (!msg || typeof msg !== 'object') return;
  const isNotification = msg.id === undefined || msg.id === null;
  if (isNotification) return; // notifications/initialized 等通知不回复
  const id = msg.id;
  const method = msg.method;
  try {
    if (method === 'initialize') return handleInitialize(id);
    if (method === 'ping') return replyResult(id, {});
    if (method === 'tools/list') return handleToolsList(id);
    if (method === 'tools/call') {
      handleToolsCall(id, msg.params).catch((e) => {
        replyResult(id, { content: [{ type: 'text', text: JSON.stringify({ code: -100, msg: '内部错误：' + e.message }) }], isError: true });
      });
      return;
    }
    if (method === 'resources/list') return handleResourcesList(id);
    if (method === 'resources/read') return handleResourcesRead(id, msg.params);
    replyError(id, -32601, '方法不支持：' + method);
  } catch (e) {
    replyError(id, -32603, '内部错误：' + e.message);
  }
}

function main() {
  checkMappingOrExit();
  let buf = '';
  let stdinEnded = false;
  let pendingCalls = 0;
  const maybeExit = () => {
    if (stdinEnded && pendingCalls === 0) process.exit(0);
  };
  process.stdin.setEncoding('utf8');
  process.stdin.on('data', (chunk) => {
    buf += chunk;
    let idx;
    while ((idx = buf.indexOf('\n')) >= 0) {
      const line = buf.slice(0, idx).trim();
      buf = buf.slice(idx + 1);
      if (!line) continue;
      let msg;
      try {
        msg = JSON.parse(line);
      } catch (e) {
        logErr('忽略非 JSON 行：' + e.message);
        continue;
      }
      handleMessage(msg);
    }
  });
  process.stdin.on('end', () => {
    stdinEnded = true;
    maybeExit();
  });
  // 暴露给 handleToolsCall：异步调用完成前维持进程存活（drain 语义）
  handleToolsCall.trackPending = {
    inc: () => { pendingCalls += 1; },
    dec: () => { pendingCalls -= 1; maybeExit(); },
  };
}

if (require.main === module) {
  main();
}

module.exports = { CONFIG, TOOL_DEFS, validateArgs, readWhitelistedDoc, buildManifestLines, handleMessage, loadConfig, validateConfig, buildToolDefs, resolveConfigName };
