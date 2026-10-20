#!/usr/bin/env node
'use strict';
/**
 * ima MCP 代理 server —— 阶段 0 单包封装（唯一执行依据：评估工件 01a157ce-e111 §2.3 v2 终态）
 *
 * 架构口径（TL 设计决策，挂任务 01a15804）：
 * 1. 零依赖 Node stdio MCP server（node >= 18，meta.json required_binaries 口径）；无 npm install，无第三方包。
 * 2. tools 业务面 = 16 个 ima OpenAPI 接口的具名 schema 映射（镜像包内 api.md 字段表，声明稳定）；
 *    每次调用 spawn 子进程执行官方 ima_api.cjs（stdout=响应 / stderr=结构化错误），零协议重写、零函数级适配。
 *    spawn 隔离为结构性必需：官方脚本在 -200（skill 需更新）时直接 process.stdout.write 更新上下文——
 *    若 require 进本进程会污染 stdio JSON-RPC 通道，spawn 天然隔离。
 * 3. 凭证链路：平台经 Stdio+Env binding 向本进程注入 IMA_CLIENT_ID / IMA_API_KEY（generic_token platform=ima），
 *    本进程 env 透传给子进程；argv 只携带 apiPath 与 body，凭证零 argv 暴露（防同机 ps 泄露）。
 *    本文件不出现任何凭证明文，占位符口径 <IMA_CLIENT_ID>/<IMA_API_KEY>。
 * 4. 能力面两类方法（AMan 定调①②，勿翻案）：
 *    方法① 获取使用说明+技能列表 —— ima_usage 工具（说明书全文+清单头）+ MCP resources/list（协议标准路径）；
 *    方法② 获取技能详情 —— ima_detail 工具 + MCP resources/read（skill 文档全文按 URI 读取）。
 *    平台 client 现状 resources/list|read 零支持（src/ common/ grep 实锚），等价轻量 tools 保平台可达；
 *    resources 原生面照落（协议面标准落法），Agent 读后自行总结沉淀，平台不代管学习质量。
 * 5. resources 读取白名单 = 包内实际存在文档（v1.1.9 实锚 5 件）+ URI 前缀 + 安装目录前缀双重校验防路径穿越。
 * 6. 启动映射一致性校验（评估 §2.3 教训条款）：入口脚本 + 全部白名单文档存在性 fail-fast，缺件退出非 0。
 * 7. sync/主动同步/分发面：零字段零实现（AMan 定调③，评估 v2 已删除 sync 配置段）。
 */

const { spawn } = require('child_process');
const fs = require('fs');
const os = require('os');
const path = require('path');

const SERVER_INFO = { name: 'ima-mcp-proxy', version: '0.1.0' };
const PROTOCOL_VERSION = '2025-06-18';
const CHILD_TIMEOUT_MS = 60_000;

/** 子包配置（阶段 0 单包内嵌常量；阶段 1 通用化时外置 config.json + loader，评估 v2 §2.7 口径） */
const CONFIG = {
  subpkg: 'ima',
  runtime: 'node',
  installDir:
    process.env.IMA_SKILL_INSTALL_DIR ||
    path.join(os.homedir(), '.skillhub', 'skills', '@tencent-adm', 'ima-skills'),
  entry: 'ima_api.cjs',
  baseUri: 'ima://skill/',
  // 凭证绑定声明：平台 Stdio+Env 注入到本进程 env，本 server 透传子进程（零变换）。
  credentials: [
    { env: 'IMA_CLIENT_ID', platformKind: 'generic_token', platform: 'ima', placeholder: '<IMA_CLIENT_ID>' },
    { env: 'IMA_API_KEY', platformKind: 'generic_token', platform: 'ima', placeholder: '<IMA_API_KEY>' },
  ],
  // 能力面：白名单文档（v1.1.9 包内实锚 5 件；缺件在启动校验中 fail-fast）
  docs: [
    { rel: 'SKILL.md', name: 'ima-skill 总使用说明', desc: '统一入口：强制规则/模块决策表/凭证检查/API 调用模板/更新机制' },
    { rel: 'notes/SKILL.md', name: '笔记模块（notes）使用说明', desc: '搜索/列表/读取/新建/追加个人笔记的规则与工作流' },
    { rel: 'knowledge-base/SKILL.md', name: '知识库模块（knowledge-base）使用说明', desc: '上传文件/添加网页/关联笔记到知识库的安全门与工作流' },
    { rel: 'notes/references/api.md', name: '笔记模块 API 参考', desc: 'openapi/note/v1 六接口请求响应结构与错误码' },
    { rel: 'knowledge-base/references/api.md', name: '知识库模块 API 参考', desc: 'openapi/wiki/v1 十接口请求响应结构与错误码' },
  ],
};

const NOTE = 'openapi/note/v1/';
const WIKI = 'openapi/wiki/v1/';

const s = (type, description, extra) => Object.assign({ type, description }, extra || {});
const schema = (properties, required) => ({ type: 'object', properties: properties || {}, required: required || [] });

/** tools 业务面映射表：16 OpenAPI 接口 + 2 能力面工具（readonly→annotations.readOnlyHint） */
const TOOL_DEFS = [
  // ── notes 模块（6 接口）──
  {
    name: 'ima_search_note', apiPath: NOTE + 'search_note', readonly: true,
    description: '搜索个人笔记（标题/正文检索），返回 note_id、标题、摘要与命中高亮。读取操作。',
    inputSchema: schema({
      search_type: s('integer', '检索方式：0=标题（默认），1=正文'),
      query_info: s('object', '查询条件', { properties: { title: s('string', '标题关键词'), content: s('string', '正文关键词') } }),
      start: s('integer', '偏移量分页起点，首页 0'),
      end: s('integer', '偏移量分页终点，首页 20'),
    }, ['query_info', 'start', 'end']),
  },
  {
    name: 'ima_list_notebook', apiPath: NOTE + 'list_notebook', readonly: true,
    description: '列出笔记本（folder 列表），游标分页。读取操作。',
    inputSchema: schema({
      cursor: s('string', '游标：首页传 "0"，后续取返回 next_cursor'),
      limit: s('integer', '数量限制'),
    }, ['cursor', 'limit']),
  },
  {
    name: 'ima_list_note', apiPath: NOTE + 'list_note', readonly: true,
    description: '列出笔记（可按笔记本过滤），游标分页。读取操作。',
    inputSchema: schema({
      folder_id: s('string', '笔记本 ID，空字符串=全部笔记'),
      sort_type: s('integer', '排序：0=更新时间（默认），1=创建时间，2=标题，3=大小'),
      cursor: s('string', '游标：首页传空字符串 ""，后续取返回 next_cursor'),
      limit: s('integer', '数量限制'),
    }, ['cursor', 'limit']),
  },
  {
    name: 'ima_get_doc_content', apiPath: NOTE + 'get_doc_content', readonly: true,
    description: '读取笔记正文（纯文本，需为笔记作者）。读取操作。',
    inputSchema: schema({
      note_id: s('string', '笔记 ID'),
      target_content_format: s('integer', '内容格式：推荐 0=纯文本（Markdown 格式当前不支持）'),
    }, ['note_id', 'target_content_format']),
  },
  {
    name: 'ima_import_doc', apiPath: NOTE + 'import_doc', readonly: false,
    description: '新建一篇笔记（Markdown 内容；不支持本地图片）。写操作，执行前须用户确认。',
    inputSchema: schema({
      content: s('string', '笔记内容（Markdown；写入前须校验 UTF-8 并过滤本地图片引用）'),
      content_format: s('integer', '固定传 1（Markdown）'),
      folder_id: s('string', '目标笔记本 ID，省略=默认位置'),
    }, ['content', 'content_format']),
  },
  {
    name: 'ima_append_doc', apiPath: NOTE + 'append_doc', readonly: false,
    description: '往已有笔记末尾追加内容（不可撤销修改，敏感操作：用户未明确指定目标笔记时先确认再执行）。写操作，执行前须用户确认。',
    inputSchema: schema({
      note_id: s('string', '目标笔记 ID'),
      content: s('string', '追加内容（Markdown；写入前须校验 UTF-8 并过滤本地图片引用）'),
      content_format: s('integer', '固定传 1（Markdown）'),
    }, ['note_id', 'content', 'content_format']),
  },
  // ── knowledge-base 模块（10 接口）──
  {
    name: 'ima_create_media', apiPath: WIKI + 'create_media', readonly: false,
    description: '创建媒体（上传文件到知识库第一步）：获取 COS 临时上传凭证。写操作，执行前须用户确认。',
    inputSchema: schema({
      file_name: s('string', '文件名称（最长 1024 字符）'),
      file_size: s('integer', '文件大小（字节）'),
      content_type: s('string', 'MIME 类型'),
      knowledge_base_id: s('string', '知识库 ID'),
      file_ext: s('string', '文件后缀名（无点号，如 pdf）'),
    }, ['file_name', 'file_size', 'content_type', 'knowledge_base_id', 'file_ext']),
  },
  {
    name: 'ima_add_knowledge', apiPath: WIKI + 'add_knowledge', readonly: false,
    description: '添加知识：文件上传流程最后一步，或直接添加网页/关联笔记（media_type=2 网页/6 微信文章/11 笔记）。写操作，执行前须用户确认。',
    inputSchema: schema({
      media_type: s('integer', '媒体类型（1 文档/2 网页/6 微信文章/11 笔记/13 图片 等，枚举见 api.md）'),
      title: s('string', '标题（文件上传时必须等于 file_name，含扩展名，不得改名）'),
      knowledge_base_id: s('string', '知识库 ID'),
      media_id: s('string', '媒体 ID（文件上传时= create_media 返回值）'),
      folder_id: s('string', '文件夹 ID，省略=根目录'),
      note_info: s('object', '笔记内容信息（media_type=11 时提供）', { properties: { content_id: s('string', '笔记 ID') } }),
      web_info: s('object', '网页内容信息（media_type=2 时必填）', { properties: { content_id: s('string', '网页 URL') } }),
      session_info: s('object', '会话内容信息（完整字段见知识库 API 参考）'),
      file_info: s('object', '文件信息（文件上传流程必填，完整字段见知识库 API 参考 FileInfo）'),
    }, ['media_type', 'title', 'knowledge_base_id']),
  },
  {
    name: 'ima_get_knowledge_base', apiPath: WIKI + 'get_knowledge_base', readonly: true,
    description: '获取知识库信息（按 ID 批量，1-20 个）。读取操作。',
    inputSchema: schema({
      ids: s('array', '知识库 ID 列表（1-20 个，不重复）', { items: { type: 'string' }, maxItems: 20 }),
    }, ['ids']),
  },
  {
    name: 'ima_get_knowledge_list', apiPath: WIKI + 'get_knowledge_list', readonly: true,
    description: '浏览知识库内容列表（可按文件夹），游标分页。读取操作。',
    inputSchema: schema({
      knowledge_base_id: s('string', '知识库 ID'),
      cursor: s('string', '游标：首页传空字符串 ""'),
      limit: s('integer', '数量限制（1-50）'),
      folder_id: s('string', '文件夹 ID，省略=根目录'),
    }, ['knowledge_base_id', 'cursor', 'limit']),
  },
  {
    name: 'ima_search_knowledge', apiPath: WIKI + 'search_knowledge', readonly: true,
    description: '在指定知识库内搜索内容（含文件和文件夹）。读取操作。',
    inputSchema: schema({
      query: s('string', '搜索关键词'),
      cursor: s('string', '游标：首页传空字符串 ""'),
      knowledge_base_id: s('string', '知识库 ID'),
    }, ['query', 'cursor', 'knowledge_base_id']),
  },
  {
    name: 'ima_search_knowledge_base', apiPath: WIKI + 'search_knowledge_base', readonly: true,
    description: '按名称搜索知识库（query 传空字符串=列出全部知识库）。读取操作。',
    inputSchema: schema({
      query: s('string', '搜索关键词（空字符串=列出全部）'),
      cursor: s('string', '游标：首页传空字符串 ""'),
      limit: s('integer', '数量限制（1-20）'),
    }, ['query', 'cursor', 'limit']),
  },
  {
    name: 'ima_get_addable_knowledge_base_list', apiPath: WIKI + 'get_addable_knowledge_base_list', readonly: true,
    description: '列出当前用户有权限添加内容的知识库（用户未指定目标知识库时展示给用户选择）。读取操作。',
    inputSchema: schema({
      cursor: s('string', '游标：首页传空字符串 ""'),
      limit: s('integer', '数量限制（1-50）'),
    }, ['cursor', 'limit']),
  },
  {
    name: 'ima_check_repeated_names', apiPath: WIKI + 'check_repeated_names', readonly: true,
    description: '上传文件前检查文件名是否重复（文件上传安全门之一，仅文件类型 media_type）。读取操作。',
    inputSchema: schema({
      params: s('array', '待检查文件列表（1-2000 项）', { items: { type: 'object', properties: { name: s('string', '文件名称'), media_type: s('integer', '媒体类型') }, required: ['name', 'media_type'] } }),
      knowledge_base_id: s('string', '知识库 ID'),
      folder_id: s('string', '文件夹 ID，省略=检查根目录'),
    }, ['params', 'knowledge_base_id']),
  },
  {
    name: 'ima_import_urls', apiPath: WIKI + 'import_urls', readonly: false,
    description: '批量导入网页/微信公众号文章到知识库（1-10 个 URL，服务端自动识别类型）。写操作，执行前须用户确认。',
    inputSchema: schema({
      knowledge_base_id: s('string', '知识库 ID'),
      folder_id: s('string', '文件夹 ID'),
      urls: s('array', 'URL 列表（1-10 个，每个非空字符串）', { items: { type: 'string' }, maxItems: 10 }),
    }, ['knowledge_base_id', 'folder_id', 'urls']),
  },
  {
    name: 'ima_get_media_info', apiPath: WIKI + 'get_media_info', readonly: true,
    description: '按 media_id 获取媒体访问信息（查看/分析/导出原文）。读取操作。',
    inputSchema: schema({
      media_id: s('string', '媒体 ID（不可为空）'),
    }, ['media_id']),
  },
  // ── 能力面两类方法（AMan 定调①②，勿翻案）──
  {
    name: 'ima_usage', apiPath: null, readonly: true,
    description: '方法①·获取 ima 技能使用说明+技能列表：返回技能清单（能力名+一句话+详情 URI）与总使用说明书全文。Agent 读后自行总结沉淀。',
    inputSchema: schema({
      module: s('string', '子模块（省略=总使用说明）：notes=笔记模块使用说明，knowledge-base=知识库模块使用说明', { enum: ['notes', 'knowledge-base'] }),
    }, []),
  },
  {
    name: 'ima_detail', apiPath: null, readonly: true,
    description: '方法②·获取技能详情：按文档读取说明书/API 参考全文（相当于人看说明书学经验）。',
    inputSchema: schema({
      doc: s('string', '文档标识（与技能列表返回的详情 URI 一致）', { enum: CONFIG.docs.map((d) => d.rel) }),
    }, ['doc']),
  },
];

// ─────────────────────────── 基础设施 ───────────────────────────

function logErr(msg) {
  try { process.stderr.write('[ima-mcp-proxy] ' + msg + '\n'); } catch (_) { /* ignore */ }
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

/** 启动映射一致性校验：入口脚本 + 全部白名单文档存在且可读，缺件 fail-fast */
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

// ─────────────────────────── ima_api.cjs 子进程调用 ───────────────────────────

/**
 * spawn 官方 ima_api.cjs：argv=[apiPath, bodyJson, '{}']，凭证经 env 透传零 argv 暴露。
 * 返回 {ok:true, text} 或 {ok:false, code, msg, updateContext?}。
 */
function callImaApi(apiPath, body) {
  return new Promise((resolve) => {
    const entryPath = path.join(CONFIG.installDir, CONFIG.entry);
    // env 透传：本进程已由平台注入 IMA_CLIENT_ID/IMA_API_KEY（Stdio+Env binding），子进程继承；
    // 显式 setattr 仅做声明性兜底，值永不落日志/argv/磁盘。
    const childEnv = Object.assign({}, process.env, {
      IMA_CLIENT_ID: process.env.IMA_CLIENT_ID || '',
      IMA_API_KEY: process.env.IMA_API_KEY || '',
    });
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
      resolve({ ok: false, code: -100, msg: 'ima API 调用超时（' + CHILD_TIMEOUT_MS + 'ms），请稍后重试' });
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
    serverInfo: SERVER_INFO,
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
  // 能力面工具：本地读文档（不 spawn）
  if (def.name === 'ima_usage') {
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
  if (def.name === 'ima_detail') {
    try {
      const text = readWhitelistedDoc(args.doc);
      replyResult(id, { content: [{ type: 'text', text: '【技能详情】(' + args.doc + ')\n' + text }] });
    } catch (e) {
      replyResult(id, { content: [{ type: 'text', text: JSON.stringify({ code: -100, msg: e.message }) }], isError: true });
    }
    return;
  }
  // 业务面：spawn 官方脚本（挂 pending 计数：stdin 关闭后仍在飞的调用先完成再退出）
  if (handleToolsCall.trackPending) handleToolsCall.trackPending.inc();
  try {
    await runBusinessTool(id, def, args);
  } finally {
    if (handleToolsCall.trackPending) handleToolsCall.trackPending.dec();
  }
}

async function runBusinessTool(id, def, args) {
  const r = await callImaApi(def.apiPath, args);
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

module.exports = { CONFIG, TOOL_DEFS, validateArgs, readWhitelistedDoc, buildManifestLines, handleMessage };
