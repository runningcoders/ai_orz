//! Common tool-related types.

use serde::{Deserialize, Serialize};

/// Lightweight tool call trace reference.
///
/// Points to a detailed tool execution trace stored in tool-specific storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallTraceRef {
    /// Tool ID that this call belongs to.
    pub tool_id: String,
    /// Unique call ID for this specific tool execution.
    pub call_id: String,
}

impl ToolCallTraceRef {
    /// Create a new ToolCallTraceRef.
    pub fn new(tool_id: String, call_id: String) -> Self {
        Self { tool_id, call_id }
    }
}

/// shell_exec 隔离 HOME 下支持「根目录指回真实 HOME」的工具链映射
///
/// 三元组：(工具链名, 官方环境变量, 真实 HOME 相对路径)。
/// 这些工具链都支持用环境变量指定根目录而不依赖 `HOME`，因此隔离 HOME
/// （git/gh 身份隔离）与工具链自身配置（`~/.cargo`、`~/.nvm` 等）可以兼得。
/// 后端注入与前端表单校验共用此单点；未列出的名字两侧都忽略/拒绝。
pub const SHELL_TOOLCHAIN_HOME_VARS: &[(&str, &str, &str)] = &[
    ("nvm", "NVM_DIR", ".nvm"),
    ("cargo", "CARGO_HOME", ".cargo"),
    ("rustup", "RUSTUP_HOME", ".rustup"),
    ("pyenv", "PYENV_ROOT", ".pyenv"),
    ("rbenv", "RBENV_ROOT", ".rbenv"),
    ("go", "GOPATH", "go"),
    ("npm", "NPM_CONFIG_USERCONFIG", ".npmrc"),
];

/// 工具链名是否在支持列表中（大小写不敏感）
pub fn is_supported_toolchain(name: &str) -> bool {
    let lowered = name.trim().to_lowercase();
    SHELL_TOOLCHAIN_HOME_VARS
        .iter()
        .any(|(supported, _, _)| *supported == lowered)
}

/// 受限工具包：Agent 不得自行安装（管理员经 HTTP 显式授权不受限）
///
/// `admin` 内含模型提供商 / MCP Server / 消息通道 / 组织 / 技能发布等管理面
/// 工具，默认不在 `BASE_AGENT_PACKS`；而 `install_tool_pack` 自身挂在
/// `tool_management`（Agent 出生可达），若无边界则 Agent 可自开 admin 包
/// 完成权限提升。封堵落在 domain，不能只靠 HTTP 路由门闩——内置工具是
/// handler 直调，不经过 axum 中间件。
pub const RESTRICTED_TOOL_PACKS: &[&str] = &["admin"];

/// Agent 自建工具禁止携带的标签（防通过 tag 把工具扩散给其他 Agent）
///
/// `neural` 恒在工具面 tag_filter（所有 Agent 天生拥有）、`internal` 为系统
/// 自用、`admin` 会被已装 admin 包的 Agent 继承——任一都能让 Agent 造出的
/// 工具越过自身授权边界。管理员 HTTP 创建不受此限（保留既有运维能力）。
pub const AGENT_FORBIDDEN_TOOL_TAGS: &[&str] = &["neural", "internal", "admin"];

/// Agent 创建工具时的标签校验（规则本体单点，供后端管理面复用）
pub fn validate_agent_created_tool_tags(tags: &[String]) -> Result<(), String> {
    let forbidden: Vec<String> = tags
        .iter()
        .map(|tag| tag.trim().to_ascii_lowercase())
        .filter(|tag| AGENT_FORBIDDEN_TOOL_TAGS.contains(&tag.as_str()))
        .collect();
    if forbidden.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Agent 创建的工具不允许携带保留标签：{}",
            forbidden.join(", ")
        ))
    }
}

/// 协议 → 工具凭据需求作用域（管理期统一校验单点）
///
/// Mcp 的声明属主是 `McpServerPo.config`（同一 server 下 N 个工具共享连接属性，
/// 工具行是 sync 派生的远端镜像），管理面不承载其声明，返回 `None`。
///
/// Http / Shell / Builtin 的声明都落在各自工具行 `config.credential_requirements`：
/// Builtin 为该键非空时优先、为空回落工厂静态声明（读取侧见
/// `pkg::tool_registry` 的 `credential_requirements`），写侧必须与读取侧同源放开，
/// 否则管理面保存必然失败。
pub fn tool_credential_scope(
    protocol: crate::enums::ToolProtocol,
) -> Option<crate::models::CredentialRequirementScope> {
    use crate::enums::ToolProtocol;
    use crate::models::CredentialRequirementScope;
    match protocol {
        ToolProtocol::Http => Some(CredentialRequirementScope::HttpTool),
        ToolProtocol::Shell => Some(CredentialRequirementScope::Shell),
        ToolProtocol::Builtin => Some(CredentialRequirementScope::Builtin),
        // MCP 的声明属主是 McpServerPo.config（工具行只是 sync 派生的远端镜像）
        ToolProtocol::Mcp => None,
    }
}

/// 工具级凭据需求声明 → `ToolPo.config` 归并（创建/更新管理面共用单一实现）
///
/// 契约层（顶层 DTO 字段）与存储层（各协议 config 内同名键）在此收口：
/// - `None` = 调用方未声明，保留 config 内既有声明（兼容 HTTP 工具旧表单）
/// - `Some([])` = 显式清空 → **落空数组 `[]`**（不是删键）：读侧把 `[]` 采信为
///   用户明确表态的「零注入」，删键则会被误判成「从未配置」而回落工厂默认
/// - 非 object 的 config（存量 Null）先归一为空对象
///
/// 声明本身非敏感（不含 credential_id / 凭据原文，D1），归并前按协议 scope
/// 走 common 六规则单点校验。
pub fn apply_credential_requirements(
    config: &mut serde_json::Value,
    requirements: Option<&[crate::models::CredentialRequirement]>,
    scope: Option<crate::models::CredentialRequirementScope>,
) -> Result<(), String> {
    let Some(requirements) = requirements else {
        return Ok(());
    };
    let Some(scope) = scope else {
        return Err("该协议的工具凭据声明不由工具行承载（MCP 请在 Server 配置）".to_string());
    };
    crate::models::identity_credentials::validate_requirements(requirements, scope)?;
    if !config.is_object() {
        *config = serde_json::Value::Object(serde_json::Map::new());
    }
    let object = config
        .as_object_mut()
        .expect("config normalized to object above");
    // 空声明是「用户显式清空 = 零注入」，不是「未配置」：必须落 `[]` 保留用户意图。
    // 删键会被读侧判成「从未配置过」而回落工厂默认（表现为"以为关了、实际仍在注入"）。
    let value =
        serde_json::to_value(requirements).map_err(|e| format!("凭据需求序列化失败: {}", e))?;
    object.insert("credential_requirements".to_string(), value);
    Ok(())
}

/// Builtin 工具 config 已知字段轻量校验（D28：CLI 命令与行为参数进 PO config）
///
/// browser 环境变量的可填值形态（前端据此决定渲染输入框还是下拉）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserEnvKind {
    /// 自由文本（路径 / URL / 逗号分隔列表）
    Text,
    /// 有限枚举：(value, 展示文案) 列表；value 为空串表示「不设置 / 用默认」
    Choice(&'static [(&'static str, &'static str)]),
}

/// browser 环境变量目录条目（前后端共享 SSOT）
///
/// 前端「工具配置」页据此生成引导式表单：变量下拉 + 含义说明 + 可填值提示；
/// 后端据此做 config.env 的键白名单校验。新增 agent-browser 变量只改此表。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrowserEnvSpec {
    /// 环境变量全名（ AGENT_BROWSER_ 前缀）
    pub key: &'static str,
    /// 中文显示名
    pub label: &'static str,
    /// 含义说明（选中时在表单下方展示）
    pub desc: &'static str,
    /// 可填值 / 示例提示
    pub hint: &'static str,
    /// 取值形态
    pub kind: BrowserEnvKind,
    /// 是否高级项（true = 有副作用风险，表单上标注「高级」）
    pub advanced: bool,
}

const CHOICE_BOOL: &[(&str, &str)] = &[
    ("", "不设置（用默认）"),
    ("1", "1 — 开启"),
    ("0", "0 — 关闭"),
];
const CHOICE_COLOR: &[(&str, &str)] = &[
    ("", "不设置（跟随系统）"),
    ("dark", "dark — 深色"),
    ("light", "light — 浅色"),
    ("no-preference", "no-preference — 无偏好"),
];
const CHOICE_RESTORE_SAVE: &[(&str, &str)] = &[
    ("", "不设置（默认 auto）"),
    ("auto", "auto — 自动"),
    ("always", "always — 总是保存"),
    ("never", "never — 不保存"),
];
const CHOICE_ENGINE: &[(&str, &str)] = &[
    ("", "不设置（默认 chrome）"),
    ("chrome", "chrome — Chromium"),
    ("lightpanda", "lightpanda — 轻量引擎"),
    ("obscura", "obscura — Rust 引擎"),
];

/// 允许在配置页填写的 agent-browser 环境变量目录
///
/// 只收录「不破坏 headless 红线、不引入代码执行 / 外部连接面」的变量。
/// 危险项见 [`BROWSER_ENV_BLOCKED`]。前端下拉与后端键校验都以此为准。
pub const BROWSER_ENV_CATALOG: &[BrowserEnvSpec] = &[
    // —— 常规：解决安装 / 渲染 / 稳定性痛点 ——
    BrowserEnvSpec {
        key: "AGENT_BROWSER_EXECUTABLE_PATH",
        label: "浏览器可执行路径",
        desc: "当 agent-browser install 下载 Chrome 失败时，指向本机已装的 Edge / Chrome。与上方 command 不同：command 是 CLI 本体，这里是浏览器内核。",
        hint: "示例：/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge、/usr/bin/google-chrome；留空则自动发现。",
        kind: BrowserEnvKind::Text,
        advanced: false,
    },
    BrowserEnvSpec {
        key: "AGENT_BROWSER_ARGS",
        label: "浏览器启动参数",
        desc: "透传给 Chromium 的启动 flag，逗号分隔。容器内通常必须加 --no-sandbox；无 GPU 服务器可加 --disable-gpu。",
        hint: "示例：--no-sandbox,--disable-dev-shm-usage,--disable-gpu",
        kind: BrowserEnvKind::Text,
        advanced: false,
    },
    BrowserEnvSpec {
        key: "AGENT_BROWSER_USER_AGENT",
        label: "User-Agent",
        desc: "覆盖浏览器 UA。目标站按 UA 区分客户端时有用；留空用浏览器默认。",
        hint: "示例：Mozilla/5.0 (compatible; AIOrzBot/1.0)",
        kind: BrowserEnvKind::Text,
        advanced: false,
    },
    BrowserEnvSpec {
        key: "AGENT_BROWSER_COLOR_SCHEME",
        label: "配色偏好",
        desc: "影响依赖 prefers-color-scheme 的页面渲染与截图。有限枚举值，直接选无需记拼写。",
        hint: "dark / light / no-preference；默认跟随系统。",
        kind: BrowserEnvKind::Choice(CHOICE_COLOR),
        advanced: false,
    },
    BrowserEnvSpec {
        key: "AGENT_BROWSER_HIDE_SCROLLBARS",
        label: "截图隐藏滚动条",
        desc: "隐藏无头 Chromium 截图里的原生滚动条，让截图更干净。默认即为开启。",
        hint: "1 = 隐藏 / 0 = 显示；留空用默认（true）。",
        kind: BrowserEnvKind::Choice(CHOICE_BOOL),
        advanced: false,
    },
    BrowserEnvSpec {
        key: "AGENT_BROWSER_NO_VERSION_CHECK",
        label: "跳过版本检查",
        desc: "agent-browser 默认每个浏览器版本会访问一次 Chrome Releases 博客查旧版。离线 / 内网环境可关掉省一次外网请求。",
        hint: "1 = 跳过检查；留空 = 保持检查。",
        kind: BrowserEnvKind::Choice(CHOICE_BOOL),
        advanced: false,
    },
    BrowserEnvSpec {
        key: "AGENT_BROWSER_NO_WEBMCP",
        label: "关闭 WebMCP",
        desc: "关闭 agent-browser 内建的 WebMCP 支持。一般无需改动，遇到站点注入干扰时可试。",
        hint: "1 = 关闭；留空 = 默认开启。",
        kind: BrowserEnvKind::Choice(CHOICE_BOOL),
        advanced: false,
    },
    BrowserEnvSpec {
        key: "AGENT_BROWSER_DEFAULT_TIMEOUT",
        label: "内部默认超时（ms）",
        desc: "agent-browser 单次操作的内部超时。官方建议低于 30000ms 以免 IPC 超时；与本工具上方的 timeout_ms（外层进程超时）是两个不同层级。",
        hint: "示例：25000（官方默认）；留空用默认。",
        kind: BrowserEnvKind::Text,
        advanced: false,
    },
    BrowserEnvSpec {
        key: "AGENT_BROWSER_DOWNLOAD_PATH",
        label: "下载目录",
        desc: "浏览器下载文件的默认落盘目录；留空用临时目录。注意截图仍走本工具统一产物存储，不受此项影响。",
        hint: "示例：/absolute/path/to/downloads",
        kind: BrowserEnvKind::Text,
        advanced: false,
    },
    BrowserEnvSpec {
        key: "AGENT_BROWSER_STATE_EXPIRE_DAYS",
        label: "登录态过期天数",
        desc: "自动清理超过 N 天的已保存会话状态（state）。配合登录态复用方案，控制凭据留存时间。",
        hint: "示例：7（默认 30）；留空用默认。",
        kind: BrowserEnvKind::Text,
        advanced: false,
    },
    // —— 高级：代理 / 登录态 / 引擎 / 安全护栏 ——
    BrowserEnvSpec {
        key: "AGENT_BROWSER_PROXY",
        label: "代理 URL（高级）",
        desc: "浏览器走指定代理，优先级高于系统 HTTP_PROXY 变量。代理 Traffic 会经过第三方，仅填可信代理。",
        hint: "示例：http://proxy.corp.example.com:8080",
        kind: BrowserEnvKind::Text,
        advanced: true,
    },
    BrowserEnvSpec {
        key: "AGENT_BROWSER_PROXY_BYPASS",
        label: "代理绕过列表（高级）",
        desc: "不走代理的主机列表，逗号分隔。内网域名 / localhost 需绕过时常配。",
        hint: "示例：localhost,*.internal.com",
        kind: BrowserEnvKind::Text,
        advanced: true,
    },
    BrowserEnvSpec {
        key: "AGENT_BROWSER_STATE",
        label: "登录态文件（高级）",
        desc: "启动时加载的 storage state 文件路径（人工登录后导出）。凭据会随文件加载，务必放在服务端受控目录。",
        hint: "示例：/absolute/path/to/state.json",
        kind: BrowserEnvKind::Text,
        advanced: true,
    },
    BrowserEnvSpec {
        key: "AGENT_BROWSER_RESTORE",
        label: "会话状态自动保存 Key（高级）",
        desc: "按此 key 自动保存 / 加载会话状态，让登录态跨调用保留。与下方 RESTORE_SAVE 配合使用。",
        hint: "示例：shared-login-state",
        kind: BrowserEnvKind::Text,
        advanced: true,
    },
    BrowserEnvSpec {
        key: "AGENT_BROWSER_RESTORE_SAVE",
        label: "会话状态保存策略（高级）",
        desc: "控制 RESTORE 的保存时机。默认 auto；never 表示只加载不回写。",
        hint: "auto / always / never",
        kind: BrowserEnvKind::Choice(CHOICE_RESTORE_SAVE),
        advanced: true,
    },
    BrowserEnvSpec {
        key: "AGENT_BROWSER_ALLOWED_DOMAINS",
        label: "域名白名单（高级·安全）",
        desc: "仅允许访问列出的域名及其子域，是 SSRF / 数据外发的强护栏。注意：官方要求全新可控上下文，与 state / profile 恢复互斥。",
        hint: "示例：example.com,*.example.com（至少填一个；逗号分隔）",
        kind: BrowserEnvKind::Text,
        advanced: true,
    },
    BrowserEnvSpec {
        key: "AGENT_BROWSER_IGNORE_HTTPS_ERRORS",
        label: "忽略证书错误（高级）",
        desc: "忽略 TLS 证书错误。仅当代理使用自签证书时临时开启；会削弱中间人防护，生产慎用。",
        hint: "1 = 忽略；留空 = 严格校验证书。",
        kind: BrowserEnvKind::Choice(CHOICE_BOOL),
        advanced: true,
    },
    BrowserEnvSpec {
        key: "AGENT_BROWSER_ENGINE",
        label: "浏览器引擎（高级）",
        desc: "切换底层引擎。chrome 兼容性最好；lightpanda / obscura 更轻但站点兼容性弱，切换前请先在目标站实测。",
        hint: "chrome / lightpanda / obscura；默认 chrome。",
        kind: BrowserEnvKind::Choice(CHOICE_ENGINE),
        advanced: true,
    },
];

/// 需要风险提示的环境变量（含理由，供配置页展示，但**不拦截配置**）
///
/// 这些变量会引入代码执行面、把会话引向外部浏览器、或暴露凭据。按「配置管理而非功能封禁」
/// 的定位，它们照常可填，仅在页面标注风险由管理员自行取舍——CLI 本就支持这些能力，硬禁只会
/// 逼想用的人走 shell_exec 绕过，老实人反而用不了。前缀合法性仍由 [`validate_browser_env`] 保证。
pub const BROWSER_ENV_BLOCKED: &[(&str, &str)] = &[
    (
        "AGENT_BROWSER_HEADED",
        "固定 headless 红线：服务端无图形界面，弹窗无意义且会引入显示环境依赖",
    ),
    (
        "AGENT_BROWSER_ALLOW_FILE_ACCESS",
        "禁止 file:// 读本地文件，避免本地数据被页面读取外发",
    ),
    (
        "AGENT_BROWSER_CDP",
        "禁止附着外部调试端口，会话边界与凭据不可控",
    ),
    (
        "AGENT_BROWSER_AUTO_CONNECT",
        "禁止自动连接外部 Chrome，同上",
    ),
    (
        "AGENT_BROWSER_PROVIDER",
        "禁止改用云浏览器 provider，页面数据会离开本机",
    ),
    (
        "AGENT_BROWSER_EXTENSIONS",
        "禁止加载浏览器扩展（代码执行面）",
    ),
    (
        "AGENT_BROWSER_INIT_SCRIPTS",
        "禁止注入页面脚本（代码执行面）",
    ),
    (
        "AGENT_BROWSER_ENABLE",
        "禁止开启内置 init script 特性（代码执行面）",
    ),
    (
        "AGENT_BROWSER_ENCRYPTION_KEY",
        "加密密钥不落工具配置：请从服务进程环境注入（子进程会继承）",
    ),
    (
        "AGENT_BROWSER_CA_CERT",
        "证书信任面：请在部署环境配置，不经工具配置页",
    ),
    (
        "AGENT_BROWSER_USE_SYSTEM_CA",
        "证书信任面：请在部署环境配置，不经工具配置页",
    ),
    (
        "AGENT_BROWSER_CLEAR_CA_CERT",
        "证书信任面：请在部署环境配置，不经工具配置页",
    ),
    (
        "AGENT_BROWSER_SESSION",
        "会话隔离由工具固定 --session ai_orz_agent_{agent_id} 管理，禁止覆盖",
    ),
    (
        "AGENT_BROWSER_NAMESPACE",
        "命名空间会改变 daemon socket 与状态目录，破坏会话边界",
    ),
    (
        "AGENT_BROWSER_CONFIG",
        "自定义配置文件可覆盖安全策略与启动参数，请在部署环境配置",
    ),
    (
        "AGENT_BROWSER_PLUGINS",
        "插件是可执行代码加载面，禁止经配置页注入",
    ),
    (
        "AGENT_BROWSER_FLOWS_DIR",
        "flows 录制会向该目录写文件，禁止由配置页指定路径",
    ),
    (
        "AGENT_BROWSER_DASHBOARD_ALLOWED_ORIGINS",
        "dashboard 信任面：请在部署环境配置，不经工具配置页",
    ),
    (
        "AGENT_BROWSER_STREAM_PORT",
        "WebSocket 流端口会新开网络监听面，禁止由配置页指定",
    ),
    (
        "AGENT_BROWSER_NO_XVFB",
        "仅 headed 模式相关（headed 已被禁用），配置无意义且徒增理解成本",
    ),
];

/// 风险键片段：显式清单之外的新变量命中这些片段时在配置页标注风险（但**不拦截**）。
///
/// 用途是「提示管理员这里要留意」而非「禁止配置」——与 [`BROWSER_ENV_BLOCKED`] 一样，
/// agent-browser 升级新增的变量会被动态发现，命中片段就附一条风险说明。匹配用 `contains`，
/// 片段本身须足够具体（`_PORT` 而非 `PORT`、`INIT_SCRIPT` 而非 `INIT`），以免误伤安全项。
pub const BROWSER_ENV_DENY_TOKENS: &[&str] = &[
    "HEADED",
    "HEADLESS",
    "FILE_ACCESS",
    "CDP",
    "AUTO_CONNECT",
    "PROVIDER",
    "EXTENSION",
    "INIT_SCRIPT",
    "ENABLE",
    "ENCRYPTION",
    "CA_CERT",
    "USE_SYSTEM_CA",
    "SESSION",
    "NAMESPACE",
    "_CONFIG",
    "PLUGIN",
    "FLOWS_DIR",
    "DASHBOARD",
    "STREAM",
    "_PORT",
    "XVFB",
    "TOKEN",
    "SECRET",
    "PASSWORD",
    "CREDENTIAL",
    "VAULT",
    "DEBUG",
    "REMOTE",
    "SKILL",
    "HOOK",
];

const BROWSER_ENV_DENY_REASON: &str =
    "命中风险能力片段（外连 / 代码执行 / 凭据 / 会话边界 / 监听端口），请确认后再启用";

/// 按 key 查目录条目（前端选中变量后取说明 / 可填值形态）
pub fn browser_env_spec(key: &str) -> Option<&'static BrowserEnvSpec> {
    BROWSER_ENV_CATALOG.iter().find(|spec| spec.key == key)
}

/// 查询变量的风险提示（显式清单优先，危险片段兜底；前端据此在配置页标注提示文字，但照常可填）
pub fn browser_env_blocked_reason(key: &str) -> Option<&'static str> {
    if let Some((_, reason)) = BROWSER_ENV_BLOCKED
        .iter()
        .find(|(blocked, _)| *blocked == key)
    {
        return Some(*reason);
    }
    BROWSER_ENV_DENY_TOKENS
        .iter()
        .find(|token| key.contains(**token))
        .map(|_| BROWSER_ENV_DENY_REASON)
}

/// 校验 browser config.env：`AGENT_BROWSER_` 前缀 + 字符串值。
///
/// **唯一硬约束是前缀**：本字段经 `exec` 叠加注入子进程，放开非前缀键等于允许覆盖
/// `PATH` / `LD_PRELOAD` 等系统变量劫持进程——这是字段语义边界，不是功能封禁。
///
/// 前缀之内的具体变量（headed / CDP / 云浏览器 / 凭据等）一律放行，由管理员在配置页
/// 自行取舍——这些是 agent-browser CLI 天然具备的能力，硬禁只会让老实走配置页的人用不了，
/// 而想绕的人走 shell_exec 一样能设。风险通过 [`browser_env_blocked_reason`] 在页面标注，
/// 不在此拦截。前后端共用（前端提交前 + 后端 update_tool / spawn 前兜底）。
pub fn validate_browser_env(value: &serde_json::Value) -> Result<(), String> {
    let Some(object) = value.as_object() else {
        return Err("config.env 必须为对象".to_string());
    };
    for (key, val) in object {
        if !key.starts_with("AGENT_BROWSER_") {
            return Err(format!(
                "config.env 的键「{key}」必须以 AGENT_BROWSER_ 开头（禁止注入系统环境变量）"
            ));
        }
        if val.as_str().is_none() {
            return Err(format!("config.env.{key} 必须为字符串"));
        }
    }
    Ok(())
}

/// 仅校验已知字段的类型与取值，未知字段宽松保留（不做白名单封闭，保持 config 扩展性）；
/// `config` 非对象（含 Null，存量 DB 兼容）时无已知字段可校验，直接通过。
/// 后端 update_tool 校验与前端表单提交前校验共用此单点。
pub fn validate_builtin_tool_config(config: &serde_json::Value) -> Result<(), String> {
    let Some(object) = config.as_object() else {
        return Ok(());
    };
    for (key, value) in object {
        match key.as_str() {
            "command" if !value.as_str().is_some_and(|command| !command.is_empty()) => {
                return Err("config.command 必须为非空字符串".to_string());
            }
            "timeout_ms" | "max_output_bytes"
                if !value.as_u64().is_some_and(|number| number > 0) =>
            {
                return Err(format!("config.{key} 必须为正整数"));
            }
            "env" => validate_browser_env(value)?,
            "allow_local_network" if !value.is_boolean() => {
                return Err("config.allow_local_network 必须为布尔值".to_string());
            }
            "disabled_commands" => {
                let arr = value
                    .as_array()
                    .ok_or_else(|| "config.disabled_commands 必须为字符串数组".to_string())?;
                if arr
                    .iter()
                    .any(|v| v.as_str().is_none_or(|s| s.trim().is_empty()))
                {
                    return Err("config.disabled_commands 不得含空命令".to_string());
                }
            }
            "allow_file_url" if !value.is_boolean() => {
                return Err("config.allow_file_url 必须为布尔值".to_string());
            }
            "allowed_file_dirs" => {
                let arr = value
                    .as_array()
                    .ok_or_else(|| "config.allowed_file_dirs 必须为字符串数组".to_string())?;
                if arr
                    .iter()
                    .any(|v| v.as_str().is_none_or(|s| s.trim().is_empty()))
                {
                    return Err("config.allowed_file_dirs 不得含空路径".to_string());
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// HTTP 工具支持的请求方法白名单（GET / POST；大小写不敏感，单点）
pub fn is_supported_http_method(method: &str) -> bool {
    matches!(method.to_ascii_uppercase().as_str(), "GET" | "POST")
}

/// 工具 `parameters` JSON Schema 的「跨 provider 安全」校验
///
/// 我们支持的 provider（OpenAI / DeepSeek / Qwen / Doubao / DoubaoVision / Ollama /
/// Custom）全部走同一个 OpenAI 兼容协议（`service/dao/cortex/native/mod.rs`），
/// 但各家对 JSON Schema 的接受度并不一致，且失败方式分两类：
///
/// - **硬拒**：整个 `chat/completions` 请求被回 400，报错还指不出是哪个工具；
/// - **静默忽略**：请求成功，但该关键字从未参与约束，schema 退化成纯文本提示。
///
/// 因此用户经 `create_tool` / `update_tool` 写入的 schema 必须收敛到最小公共子集
/// （LCD）—— 否则一个工具就能毒死持有它的 Agent 的**每一轮**模型调用。
///
/// 拒绝规则（每条都对应真实的 provider 行为，不是风格偏好）：
///
/// - 整体必须是 JSON 对象，且根部 `type` 显式为 `"object"`：工具参数契约即一个对象；
///   无参工具应写 `{"type":"object","properties":{}}`
/// - 任何 `type` 只能是字符串或字符串数组（`null` / 数字会被 DeepSeek 拒整包）
/// - 任何位置 `items` 都不得是数组（draft-07 的元组校验）：火山方舟与 OpenAI 均回
///   400 `[...] is not of type 'object', 'boolean'`
/// - 声明为数组的位置必须带**对象**形式的 `items`：OpenAI 回 400
///   `array schema missing items`
/// - 不得使用 `prefixItems`：OpenAI 未实现 2020-12 的元组关键字
///
/// 错误信息以 JSON Pointer（`/properties/env/items`）定位，便于用户直接改。
/// 后端 create/update 写入口与前端表单提交前校验共用此单点。
pub fn validate_tool_parameters_schema(schema: &serde_json::Value) -> Result<(), String> {
    let Some(root) = schema.as_object() else {
        return Err("parameters_schema 必须是 JSON 对象".to_string());
    };

    let mut violations: Vec<String> = Vec::new();
    match root.get("type") {
        Some(serde_json::Value::String(kind)) if kind == "object" => {}
        Some(other) => violations.push(format!(
            "/type 必须为 \"object\"，当前为 {other}；无参工具请写 {{\"type\":\"object\",\"properties\":{{}}}}"
        )),
        None => violations.push(
            "/type 缺失，必须显式声明 \"object\"（无参工具请写 {\"type\":\"object\",\"properties\":{}}）"
                .to_string(),
        ),
    }
    collect_schema_violations(schema, "", &mut violations);

    if violations.is_empty() {
        return Ok(());
    }
    const MAX_REPORTED: usize = 5;
    let shown = violations
        .iter()
        .take(MAX_REPORTED)
        .cloned()
        .collect::<Vec<String>>()
        .join("；");
    let more = if violations.len() > MAX_REPORTED {
        format!("（另有 {} 处未列出）", violations.len() - MAX_REPORTED)
    } else {
        String::new()
    };
    Err(format!(
        "parameters_schema 不符合跨 provider 安全子集：{shown}{more}"
    ))
}

/// 递归收集「会让网关拒绝或静默忽略」的 schema 写法，路径为 JSON Pointer 片段。
///
/// ⚠️ 必须沿**已知的 schema 承载关键字**下钻，绝不能把 `properties` 里的键名当关键字：
/// 用户完全可能有一个**字段就叫 `type`**（如 `{"type":"env"}`），若按 key 名泛化匹配，
/// `properties.type` 会被误判成 JSON Schema 的 `type` 关键字 → 假阳性直接挡住合法工具
/// （`create_mcp_server` / `update_mcp_server` 的 `CredentialBinding` 就是这种结构）。
fn collect_schema_violations(node: &serde_json::Value, path: &str, out: &mut Vec<String>) {
    let serde_json::Value::Object(map) = node else {
        return;
    };

    // ① `type` 关键字的合法性与「array 必须带对象 items」的配套约束
    if let Some(kind) = map.get("type") {
        match schema_type_names(kind) {
            Some(names) => {
                if names.contains(&"array") {
                    match map.get("items") {
                        // 元组写法由 ② 报；对象形式合法
                        Some(serde_json::Value::Array(_)) | Some(serde_json::Value::Object(_)) => {}
                        Some(other) => {
                            out.push(format!("{path}/items 必须为对象，当前为 {other}"));
                        }
                        None => out.push(format!(
                            "{path}/items 缺失：声明为 array 的 schema 必须提供对象形式的 items（OpenAI 会回 400 array schema missing items）"
                        )),
                    }
                }
            }
            None => out.push(format!(
                "{path}/type 必须为字符串或字符串数组，当前为 {kind}"
            )),
        }
    }

    // ② 元组写法：`items` 为数组
    if let Some(serde_json::Value::Array(_)) = map.get("items") {
        out.push(format!(
            "{path}/items 不能是数组：数组形式的 items 是元组校验，火山方舟与 OpenAI 会回 400"
        ));
    }

    // ③ 2020-12 元组关键字，OpenAI 未实现
    if map.contains_key("prefixItems") {
        out.push(format!(
            "{path}/prefixItems 不受支持（2020-12 元组关键字，OpenAI 会回 400）"
        ));
    }

    // ④ 沿 schema 承载关键字下钻
    for (key, value) in map {
        let child = format!("{path}/{key}");
        match key.as_str() {
            // 名字 → schema 的映射（键名是字段名，不是关键字）
            "properties" | "patternProperties" | "definitions" | "$defs" | "dependentSchemas" => {
                if let serde_json::Value::Object(entries) = value {
                    for (name, sub) in entries {
                        collect_schema_violations(sub, &format!("{child}/{name}"), out);
                    }
                }
            }
            // 单个 schema 或（元组/2020-12 形态的）schema 数组
            "items" | "prefixItems" => match value {
                serde_json::Value::Object(_) => collect_schema_violations(value, &child, out),
                serde_json::Value::Array(elements) => {
                    for (idx, element) in elements.iter().enumerate() {
                        collect_schema_violations(element, &format!("{child}[{idx}]"), out);
                    }
                }
                _ => {}
            },
            // schema 数组（组合子）
            "anyOf" | "oneOf" | "allOf" => {
                if let serde_json::Value::Array(branches) = value {
                    for (idx, branch) in branches.iter().enumerate() {
                        collect_schema_violations(branch, &format!("{child}[{idx}]"), out);
                    }
                }
            }
            // 单 schema 位置
            "additionalProperties"
            | "propertyNames"
            | "contains"
            | "not"
            | "if"
            | "then"
            | "else" => collect_schema_violations(value, &child, out),
            // 其余关键字（required / enum / description / $ref / minItems …）不承载子 schema
            _ => {}
        }
    }
}

/// 解析 `type` 的取值：字符串 → 单元素；字符串数组 → 多元素；其余（含 null）→ None
fn schema_type_names(kind: &serde_json::Value) -> Option<Vec<&str>> {
    match kind {
        serde_json::Value::String(name) => Some(vec![name.as_str()]),
        serde_json::Value::Array(names) => names
            .iter()
            .map(serde_json::Value::as_str)
            .collect::<Option<Vec<&str>>>(),
        _ => None,
    }
}
#[cfg(test)]
#[path = "tool_tests.rs"]
mod tests;
