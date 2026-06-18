# Built-in HTTP Tools 设计文档

## 概述

项目内置 `http_get` / `http_post` 工具，允许 Agent 发送 HTTP 请求获取外部资源。

## 架构设计

### 遵循现有架构约定

1. **工厂 + 实例模式**：
   - 工厂注册到 `GLOBAL_TOOL_REGISTRY`，单例生命周期
   - 每次工具调用从工厂 `create()` 新建工具实例
   - DB 中的 `ToolPo` 配置（名称/描述/开关/config）注入到实例

2. **HTTP 客户端复用**：
   - `reqwest::Client` 通过 `Arc` 保存在工厂单例
   - 工具实例仅 `clone` 一份 `Arc<Client>` 复用连接池
   - 避免重复建立 TLS 连接，性能更好

3. **权限/安全校验内聚**：
   - 每个工具自己实现权限检查
   - 不同工具可以有不同安全策略
   - 便于未来扩展其他工具

### 目录结构

```
src/pkg/builtins/
├── mod.rs           # 模块导出，注册所有内置工具工厂
└── http.rs          # http_get + http_post 实现

docs/
└── builtins_http_tool_design.md  # 本文档
```

## 安全设计

### 禁止访问内网（SSRF 防护）

禁止以下 IP 段：

| CIDR | 说明 |
|------|------|
| `127.0.0.0/8` | Localhost |
| `10.0.0.0/8` | Private network |
| `172.16.0.0/12` | Private network |
| `192.168.0.0/16` | Private network |
| `::1/128` | IPv6 localhost |
| `fc00::/7` | IPv6 unique local addr |

**检测逻辑**：
1. 从 URL 解析 host 得到域名
2. 解析域名得到 IP 地址列表
3. 检查任何一个 IP 命中私有段 → 拒绝请求

### 默认限制

| 限制 | 默认值 | 可配置 |
|------|--------|--------|
| 请求超时 | 30 秒 | ✅ 从 `ToolPo.config` 读取 |
| 最大响应大小 | 10 MB | ✅ 从 `ToolPo.config` 读取 |
| 允许方法 | GET/POST | 分开两个工具 |

## 参数 JSON Schema

### http_get

```json
{
  "type": "object",
  "properties": {
    "url": {
      "type": "string",
      "description": "Target URL to send GET request to. Must be a public, non-private address."
    },
    "headers": {
      "type": "object",
      "additionalProperties": {
        "type": "string"
      },
      "description": "Optional custom HTTP headers to send with the request."
    },
    "timeout_seconds": {
      "type": "number",
      "description": "Optional request timeout in seconds. Defaults to 30."
    }
  },
  "required": ["url"]
}
```

### http_post

```json
{
  "type": "object",
  "properties": {
    "url": {
      "type": "string",
      "description": "Target URL to send POST request to. Must be a public, non-private address."
    },
    "body": {
      "type": "string",
      "description": "Request body content to send. Can be JSON, form-data, plain text, etc."
    },
    "headers": {
      "type": "object",
      "additionalProperties": {
        "type": "string"
      },
      "description": "Optional custom HTTP headers to send with the request."
    },
    "timeout_seconds": {
      "type": "number",
      "description": "Optional request timeout in seconds. Defaults to 30."
    }
  },
  "required": ["url", "body"]
}
```

## 响应格式

成功响应：

```json
{
  "status": 200,
  "content_length": 12345,
  "headers": {
    "Content-Type": "application/json",
    "Server": "nginx"
  },
  "content": "... response text content (truncated if exceeds max size) ..."
}
```

错误响应（返回 `ToolError`，LLM 可以看到错误信息）：

- 地址不被允许："URL is not allowed: private IP address blocked"
- 超时："Request timed out after 30 seconds"
- 解析错误："Failed to parse URL: ..."
- 网络错误："Network error: ..."

## 配置格式（ToolPo.config）

`ToolPo.config` 是 JSON 对象，支持以下字段：

```json
{
  "max_response_size_mb": 10,
  "default_timeout_seconds": 30,
  "allowed_domains": ["api.github.com", "example.com"],
  "blocked_domains": ["internal.company.com"]
}
```

所有字段都是可选的，不配置则使用默认值。

## 核心代码结构

```rust
// 工厂单例
pub struct HttpGetBuiltinFactory;

impl BuiltinToolFactory for HttpGetBuiltinFactory {
    fn create_po(&self) -> ToolPo {
        ToolPo::new_builtin(
            "http_get".into(),
            "http_get".into(),
            "Send HTTP GET request to public URL".into(),
        )
        // with parameters_schema filled from JSON above
    }

    fn create(&self, po: ToolPo) -> Box<dyn CoreTool> {
        Box::new(HttpGetTool::from_po(po, HTTP_CLIENT.clone()))
    }
}

// 工具实例
pub struct HttpGetTool {
    po: ToolPo,
    client: Arc<Client>,
    max_response_size: usize,
    default_timeout_secs: u64,
}

impl HttpGetTool {
    pub fn from_po(po: ToolPo, client: Arc<Client>) -> Self {
        // parse config from po.config, use defaults if missing
        let max_response_size = ...;
        let default_timeout_secs = ...;
        Self { po, client, max_response_size, default_timeout_secs }
    }

    fn check_url_permission(&self, url: &str) -> Result<(), ToolError> {
        // parse URL → resolve IP → check private ranges
    }
}

#[async_trait]
impl CoreTool for HttpGetTool {
    async fn call(&self, ctx: RequestContext, args: Value) -> Result<Value, ToolError> {
        // 1. parse arguments from Value
        // 2. check permission (call check_url_permission)
        // 3. build request with headers and timeout
        // 4. execute request
        // 5. check response size
        // 6. read and convert to UTF-8
        // 7. return JSON result
    }

    fn po(&self) -> &ToolPo {
        &self.po
    }
}
```

## 注册流程

1. `src/pkg/builtins/mod.rs` 定义 `register_all()` 函数
2. 在 `src/pkg/mod.rs` 调用 `builtins::register_all();`
3. 程序启动自动注册所有内置工具到全局注册表
4. 系统同步时会自动同步到数据库 `tools` 表

## 待实现任务

- [ ] 创建 `src/pkg/builtins/` 目录结构
- [ ] 实现 `http_get` 工具完整逻辑
- [ ] 实现 `http_post` 工具完整逻辑
- [ ] 实现私有 IP 检测逻辑
- [ ] 添加 `reqwest` 依赖到 `Cargo.toml`
- [ ] 单元测试
