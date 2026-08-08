# 通用 Builtin 工具设计 - 文件读写 & HTTP Fetch

## 概述

ai_orz 需要为 Agent 提供**基础通用能力**，方便 Agent 在运行时自动处理常见任务：
- **读写文件**：Agent 需要读取工作目录下的文件内容、修改代码、写入输出
- **HTTP Fetch**：Agent 需要动态获取 URL 内容（文档、API 响应、网页）

这些工具作为 **Builtin + Auto** 模式：
- `Builtin`：代码内置，用户不可修改/删除，受 Builtin 保护
- `Auto`：直接进入 Rig tool calling，Agent 运行时自动调用，不需要走 Manual 消息链路

设计原则：**安全第一，默认保守，最小权限，严格沙箱**。

---

## 通用安全原则

所有通用 Builtin 工具必须遵循：

1. **默认拒绝风险操作**：未知/风险路径/地址默认拒绝，不依赖调用方校验
2. **错误信息脱敏**：不暴露文件系统路径结构、网络地址、认证信息
3. **大小限制**：有默认上限和硬上限，防止 OOM
4. **追踪脱敏**：参数/结果/错误中可能包含的敏感信息在 `ToolCallEntry` 中统一脱敏
5. **沙箱隔离**：文件操作限制在 `base_data_path` 范围内，网络操作复用 HTTP Tool SSRF 防护

---

## 1. 文件读写工具设计

### 工具拆分
## 1. 文件读写工具设计

### 工具拆分

两个独立工具，职责清晰：

| 工具 ID | 工具名称 | 功能 |
|---------|----------|------|
| `fs_read` | read_file | 读取文件内容，支持范围读取和grep匹配 |
| `fs_write` | write_file | 写入文件内容，支持多种编辑模式 |

### 参数 Schema

**read_file**：
```json
{
  "type": "object",
  "properties": {
    "path": {
      "type": "string",
      "description": "Path to the file to read, relative to the project/workspace root"
    },
    "start_line": {
      "type": "integer",
      "description": "Optional: start reading from this line (1-indexed). If omitted, start from beginning."
    },
    "end_line": {
      "type": "integer",
      "description": "Optional: stop reading at this line (inclusive). If omitted, read to end."
    },
    "grep": {
      "type": "string",
      "description": "Optional: return lines containing this substring (simple string matching, not regex). Returns matches with context."
    },
    "context_lines": {
      "type": "integer",
      "default": 2,
      "description": "Optional: number of context lines to include before and after each grep match."
    }
  },
  "required": ["path"]
}
```

**read_file 返回格式**：

整文件/范围读取：
```json
{
  "success": true,
  "path": "relative/path/to/file",
  "size_bytes": 12345,
  "total_lines": 42,
  "content": "   1|line 1 content\n   2|line 2 content\n..."
}
```

grep 模式：
```json
{
  "success": true,
  "path": "relative/path/to/file",
  "query": "pattern",
  "total_matches": 2,
  "matches": [
    {
      "line_number": 10,
      "content": "  10|matched line content here",
      "context_before": ["   8|...", "   9|..."],
      "context_after": ["  11|...", "  12|..."]
    }
  ]
}
```

失败：
```json
{
  "success": false,
  "error": "Access denied: cannot read the specified file"
}
```

**write_file**：
```json
{
  "type": "object",
  "properties": {
    "path": {
      "type": "string",
      "description": "Path to the file to write, relative to the project root"
    },
    "content": {
      "type": "string",
      "description": "New content to write/insert. Not used for delete_range mode."
    },
    "mode": {
      "type": "string",
      "enum": ["overwrite", "append", "insert_after", "delete_range", "replace_range"],
      "default": "overwrite",
      "description": "Write mode:\n- overwrite: replace entire file (atomic)\n- append: append content to end of file (atomic)\n- insert_after: insert new content after the specified line (atomic)\n- delete_range: delete lines from start_line to end_line (atomic)\n- replace_range: replace the entire range [start_line, end_line] with new content (composite = delete + insert, one step)"
    },
    "after_line": {
      "type": "integer",
      "description": "Required for insert_after: insert after this line number (1-indexed)"
    },
    "start_line": {
      "type": "integer",
      "description": "Required for delete_range/replace_range: starting line (1-indexed)"
    },
    "end_line": {
      "type": "integer",
      "description": "Required for delete_range/replace_range: ending line (1-indexed, inclusive)"
    }
  },
  "required": ["path"]
}
```

**write_file 返回格式**：

成功：
```json
{
  "success": true,
  "path": "relative/path/to/file",
  "mode": "replace_range",
  "original_lines": 42,
  "final_lines": 45,
  "lines_changed": 3
}
```

失败：
```json
{
  "success": false,
  "error": "Access denied: path outside allowed base directory"
}
```
（错误信息脱敏，不暴露完整绝对路径）

### 设计决策总结

| 模式 | 类型 | 作用 |
|------|------|------|
| `overwrite` | 原子 | 覆盖整个文件（新建/完全重写） |
| `append` | 原子 | 追加到文件末尾 |
| `insert_after` | 原子 | 在指定行后插入新内容 |
| `delete_range` | 原子 | 删除指定行区间 |
| `replace_range` | 复合 | 替换整个区间（内部 = delete + insert，一步完成） |

设计思路：**保留底层原子操作，同时封装常用复合操作**，兼顾灵活性和便捷性。

### 安全设计

#### 1. 沙箱路径限制

- **允许范围**：只允许读写 `base_data_path` 目录及其子目录内的文件
- **相对路径解析**：用户输入 `path` 视为相对于 `base_data_path`，解析后做 canonicalize 校验
- **绝对路径处理**：如果用户输入绝对路径，必须 canonicalize 后以 `base_data_path` 为前缀，否则拒绝
- **`..` 穿透防护**：解析后 canonical path 必须仍在 `base_data_path` 内，防止 `../` 跳出沙箱
- **符号链接防护**：默认拒绝符号链接，防止链接到沙箱外文件

#### 2. 大小限制

| 限制 | 值 |
|------|-----|
| 默认读取上限 | 1MB |
| 默认写入上限 | 1MB |
| 硬上限 | 10MB |
| 超过上限 | 直接拒绝，返回错误 |

#### 3. 敏感文件拒绝

默认拒绝读写以下路径模式（大小写不敏感）：
- 包含 `.env`
- 包含 `.pem` / `.key` / `.p12` / `.pfx`
- 包含 `id_rsa` / `id_dsa` / `id_ecdsa`
- 包含 `password` / `secret` / `token` / `credential` / `auth`
- 文件名以 `.` 开头的隐藏文件（可配置允许，默认拒绝）

#### 4. 错误脱敏

错误信息只保留通用描述，不暴露：
- 完整绝对路径
- 文件系统错误原文（如 `Permission denied` 具体路径）
- 用户输入原始路径中的敏感信息

示例：
- ❌ 错误 `Failed to open /home/user/.env: Permission denied`
- ✅ 正确 `Access denied: cannot read the specified file`

#### 5. 追踪脱敏

`LoggingDecorator` 对 Builtin 文件工具默认脱敏：
- `input`：`path` 保留文件名，不完整路径；`content` 全脱敏
- `output`：`content` 全脱敏，只保留成功状态和行数变化
- `error`：保持已经脱敏的错误信息不变

---

## 2. HTTP Fetch 工具设计

### 工具定义

| 工具 ID | 工具名称 | 功能 |
|---------|----------|------|
| `http_fetch` | fetch_url | Fetch content from a URL with GET method |

### 参数 Schema

```json
{
  "type": "object",
  "properties": {
    "url": {
      "type": "string",
      "description": "URL to fetch (must be HTTPS unless explicitly allowed)"
    }
  },
  "required": ["url"]
}
```

设计决策：只支持 GET 方法，Agent 动态获取内容不需要 POST/PUT 等写操作，减少风险。

### 返回格式

**成功**：
```json
{
  "success": true,
  "url": "https://example.com/path",
  "status": 200,
  "content_type": "text/plain",
  "content_length": 12345,
  "content": "...fetched content..."
}
```

**失败**：
```json
{
  "success": false,
  "error": "Failed to fetch: access denied (local network address blocked)"
}
```

### 安全设计

完全复用 [HTTP Tool Runtime](./builtins_http_tool_design.md) 的安全策略：

1. **SSRF 防护**：
   - 默认拒绝 localhost/私网/特殊网段（127.0.0.0/8, 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16 等）
   - 解析所有 IP，任一 IP 命中风险网段则拒绝
   - DNS pinning：把校验后的地址 pin 到请求，禁用代理，防止 DNS rebinding
   - 默认不允许 HTTP，只允许 HTTPS（特殊场景后续再扩展）

2. **Redirect 策略**：
   - 默认不跟随重定向
   - 防止初始 URL 合法，重定向到内网风险地址

3. **大小限制**：
   - 默认上限：1MB
   - 硬上限：10MB
   - 超过上限直接返回错误，不完整读取

4. **默认超时**：
   - 连接超时：10s
   - 整体请求超时：30s

5. **脱敏策略**：
   - 管理面：作为 Builtin 不存在管理面修改，天然安全
   - 追踪日志：url 脱敏，content 全脱敏，error 脱敏
   - 错误信息：不暴露 `reqwest` 原始错误，不泄露 IP 地址

---

## 代码组织结构

实际实现调整为同级铺开，去掉多余一级 `builtin/` 目录：

```
src/pkg/tool_registry/
├── builtin.rs                # BuiltinToolFactory trait + 通用工具注册入口（原文件保留在这里）
├── tool_security.rs          # 通用安全校验函数（抽取自 http.rs，供 http_fetch 和后续 fs 工具复用）
├── http_fetch.rs             # http_fetch 工厂 + 核心实现
├── fs_read.rs                # read_file 工厂 + 核心实现（待实现）
├── fs_write.rs               # write_file 工厂 + 核心实现（待实现）
├── mod.rs                    # 导出所有模块，原有 registry 逻辑保留
├── builtin.rs                # 原有 BuiltinToolFactory trait 定义保留
├── http.rs                   # 原有 HTTP Tool 运行时保留（用户注册的工具）
└── mcp.rs                   # 原有 MCP 运行时保留
```

### 抽取公共安全函数

从 `http.rs` 抽取 SSRF 相关公共函数到 `tool_security.rs`：
- `is_local_network_host`
- `is_local_network_ip`
- `normalize_domain`
- `domain_matches`
- `validate_target_url`
- `validate_resolved_addresses`
- `read_limited_response_body`
- `is_sensitive_header`
- `sanitize_response_headers`
- 常量 `DEFAULT_RESPONSE_MAX_BYTES`, `HARD_RESPONSE_MAX_BYTES`, `DEFAULT_TIMEOUT_MS`

`http.rs` 改从 `tool_security.rs` 导入，不改变现有功能。

### Builtin 工厂注册

每个通用工具实现 `BuiltinToolFactory`：

```rust
pub struct HttpFetchToolFactory;
impl BuiltinToolFactory for HttpFetchToolFactory {
    fn create_po(&self) -> ToolPo {
        ToolPo {
            id: "http_fetch".to_string(),
            name: "fetch_url".to_string(),
            description: "Fetch content from an HTTPS URL with GET method".to_string(),
            protocol: ToolProtocol::Builtin,
            control_mode: ControlMode::Auto,
            parameters_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "HTTPS URL to fetch (HTTP is not allowed by default)"
                    }
                },
                "required": ["url"],
                "additionalProperties": false
            })),
            config: Value::Null,
            ..Default::default()
        }.fill_defaults_for_builtin()
    }
    fn create(&self, po: ToolPo) -> Box<dyn CoreTool> {
        Box::new(HttpFetchCoreTool { po })
    }
}

struct HttpFetchCoreTool {
    po: ToolPo,
}

#[async_trait]
impl CoreTool for HttpFetchCoreTool {
    async fn call(&self, _ctx: RequestContext, args: Value) -> Result<Value> {
        // 1. parse url from args
        // 2. validate scheme (only https allowed)
        // 3. validate target url using common::validate_target_url
        //    - default allow_local_network = false
        //    - no allowed_domains / blocked_domains (any public https allowed)
        // 4. create client with DNS pinning, no redirect, timeout
        // 5. send GET request
        // 6. read limited body
        // 7. return json result with status, headers, content_length, body
    }
    fn po(&self) -> &ToolPo { &self.po }
}
```

### 注册初始化

在 `src/pkg/tool_registry/builtin/mod.rs`：

```rust
use super::BuiltinToolFactory;
use lazy_static::lazy_static;

lazy_static! {
    /// All generic builtin tool factories to register on startup.
    pub static ref GENERIC_BUILTIN_TOOLS: Vec<Box<dyn BuiltinToolFactory>> = vec![
        Box::new(FsReadToolFactory::default()),
        Box::new(FsWriteToolFactory::default()),
        Box::new(HttpFetchToolFactory::default()),
    ];
}

/// Register all generic builtin tools to the global registry.
/// Called once on app startup.
pub fn register_all(registry: &ToolRegistry) {
    for factory in GENERIC_BUILTIN_TOOLS.iter() {
        registry.register_builtin_factory(factory.clone());
    }
}
```

然后在 `src/cli/serve.rs` 启动时调用：

```rust
pkg::tool_registry::builtin::register_all(
    pkg::tool_registry::get_registry()
);
```

---

## 权限范围设计

### 当前实现
- 默认只允许操作 `base_data_path` 目录下的文件（项目/任务工作目录、附件目录、产物目录）
- 敏感文件（`.env`, `.pem`, 私钥等）直接拒绝访问
- 符号链接直接拒绝
- **超出默认范围处理**：不直接拒绝，而是返回 `{ "success": false, "require_confirmation": true, "message": "..." }`，要求 Agent **必须停止并询问用户显式确认**后才能继续
- 工具描述中明确写入强规则，Agent 遵循该约定

### 未来拓展方向（可配置化）
- 在 `ToolPo.config` 中增加 `allowed_paths` 配置项，支持用户自定义额外允许访问的目录
- 支持 `blocked_extensions` 配置禁止写入的文件类型
- 支持 `allowed_extensions` 配置只允许特定文件类型
- 支持每个 Agent 单独配置允许范围，满足多租户隔离

这些拓展不影响当前实现，可以后续按需迭代添加。

---

## 复用现有安全代码

| 安全能力 | 复用位置 |
|----------|----------|
| SSRF 防护、DNS pinning | 抽取到 `common.rs` 共享 |
| 响应大小限制、超时配置 | 共享常量和函数 |
| 追踪脱敏 | LoggingDecorator 已经对所有外部协议工具脱敏，`Builtin` + `http_fetch` 自动享受 |

---

## 测试计划

### HTTP Fetch 测试

1. ✅ 抓取公网 HTTPS URL → 成功返回内容
2. ✅ 抓取 http URL → 默认拒绝（只允许 HTTPS）
3. ✅ 抓取 localhost → 默认拒绝
4. ✅ 抓取私网 IP → 默认拒绝
5. ✅ 抓取超过大小 → 拒绝
6. ✅ 3xx 重定向 → 不跟随，返回原始响应
7. ✅ 错误信息不包含 URL 敏感参数 → 脱敏正确
8. ✅ 域名匹配 `example.com` 不匹配 `example.com.` 绕过 → 归一化正确

### 文件读写测试

1. ✅ 读取 `base_data_path` 内存在文件 → 成功返回内容
2. ✅ 读取不存在文件 → 失败，脱敏错误
3. ✅ 读取 `../` 跳出沙箱 → 拒绝访问
4. ✅ 读取绝对路径在沙箱内 → 成功
5. ✅ 读取绝对路径跳出沙箱 → 拒绝
6. ✅ 读取 `.env` → 拒绝（敏感文件）
7. ✅ 读取超过大小限制 → 拒绝
8. ✅ 写入新文件 → 成功
9. ✅ 覆盖已有文件 → 成功
10. ✅ 追加写入 → 成功
11. ✅ 写入超过大小 → 拒绝
12. ✅ 写入敏感文件名 → 拒绝

---

## 实施进度

**✅ 全部完成！**

1. ✅ 设计文档完成
2. ✅ 从 `http.rs` 抽取公共安全函数到 `tool_security.rs`，更新 `http.rs` 导入
3. ✅ 实现 `http_fetch` + 完整安全规范（HTTPS-only + SSRF防护）
4. ✅ 实现 `fs_read` + 沙箱控制 + 越界用户确认机制
5. ✅ 实现 `fs_write` + 五种原子编辑模式 + 沙箱控制
6. ✅ 在 `builtin.rs` 汇总注册，启动时自动注册
7. ✅ 讨论文件读写关键点和设计决策 ✅ **讨论完成**
8. ✅ 补充权限范围设计，预留未来可配置化拓展点
9. ✅ 编译验证通过 **成功**


---

## 与现有设计的一致性

| 原则 | 符合性 |
|------|--------|
| Builtin 保护（DAO 禁止修改/删除） | ✅ 符合，继承 Builtin 默认保护 |
| ControlMode = Auto | ✅ 符合，Agent 运行时自动调用 |
| 严格分层 | ✅ 符合，Pkg 层基础设施，不碰 DAO/DAL 业务逻辑 |
| 安全默认拒绝 | ✅ 符合，风险操作默认拒绝，显式允许才能绕过 |
| 错误脱敏 | ✅ 符合，所有错误信息不暴露敏感细节 |
| 追踪脱敏 | ✅ 符合，input/output 敏感内容默认脱敏 |


| 错误脱敏 | ✅ 符合，所有错误信息不暴露敏感细节 |
| 追踪脱敏 | ✅ 符合，input/output 敏感内容默认脱敏 |

