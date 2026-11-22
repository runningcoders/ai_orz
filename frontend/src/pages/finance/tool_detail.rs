//! Tool 详情页

use crate::api::finance::{
    debug_call_tool, delete_tool, get_tool, query_tools, update_tool, update_tool_status,
};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::create_tool_http::{
    HttpToolFormState, HttpToolSubForm, http_config_from_form, http_form_from_config,
};
use crate::components::create_tool_shell::{
    ShellToolFormState, ShellToolSubForm, shell_config_from_form, shell_form_from_config,
};
use crate::components::credential_requirements::CredentialRequirementsTable;
use crate::components::hud::{HudCallout, HudPanel, PageHeader};
use crate::components::injection_strategy_editor::{
    InjectionStrategyEditor, config_layer_declared, injection_panel_visible, prepare_for_save,
};
use crate::components::markdown::MarkdownRenderer;
use crate::components::state::{EmptyState, Loading};
use crate::components::stats::ToolStatsPanel;
use crate::components::time_range_picker::{TimeRange, TimeRangePicker};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::{
    DebugCallToolRequest, GetToolRequest, RuntimeReady, ToolQueryRequest, UpdateToolRequest,
    UpdateToolStatusRequest,
};
use common::enums::{ToolProtocol, ToolStatus};
use common::models::{CredentialRequirement, CredentialRequirementScope};
use dioxus::prelude::*;
use dioxus_router::{Link, use_navigator};

/// 构造带统计信息的 GetToolRequest
///
/// `range` 为统计时间窗口（详情页时间筛选器产出，毫秒闭区间）。
/// 工具统计只有窗口内聚合值（调用次数 / 失败次数 / 平均耗时），不含时序曲线，
/// 因此没有 `stats_interval` 粒度参数 —— 工具详情页也没有趋势图。
fn build_tool_stats_request(id: String, range: TimeRange) -> GetToolRequest {
    GetToolRequest {
        id,
        with_stats: Some(true),
        stats_time_start: Some(range.start_ms),
        stats_time_end: Some(range.end_ms),
    }
}

/// 从 JSON Schema 生成参数骨架
///
/// 遍历 schema.properties，按类型生成默认值：
/// string → "", number/integer → 0, boolean → false, object → {}, array → [], null → null
fn generate_skeleton_from_schema(schema: &serde_json::Value) -> String {
    let empty = "{}".to_string();
    let Some(properties) = schema.get("properties").and_then(|v| v.as_object()) else {
        return empty;
    };
    if properties.is_empty() {
        return empty;
    }
    let mut skeleton = serde_json::Map::new();
    for (key, prop_schema) in properties {
        let default_val = generate_default_value(prop_schema);
        skeleton.insert(key.clone(), default_val);
    }
    serde_json::to_string_pretty(&serde_json::Value::Object(skeleton)).unwrap_or(empty)
}

/// 根据 JSON Schema 属性定义生成默认值
fn generate_default_value(prop_schema: &serde_json::Value) -> serde_json::Value {
    // 优先使用 schema 中的 default 字段
    if let Some(default) = prop_schema.get("default") {
        return default.clone();
    }
    let prop_type = prop_schema
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("string");
    match prop_type {
        "string" => serde_json::Value::String(String::new()),
        "number" | "integer" => serde_json::json!(0),
        "boolean" => serde_json::Value::Bool(false),
        "object" => serde_json::json!({}),
        "array" => serde_json::json!([]),
        _ => serde_json::Value::Null,
    }
}

// ==================== 内置工具配置编辑（D28：CLI 命令与行为参数进 PO config） ====================

/// 内置工具结构化配置表单清单（按工厂注册名匹配）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuiltinConfigForm {
    /// browser：command / timeout_ms / max_output_bytes / install_hint 四字段
    Browser,
    /// desktop：command / timeout_ms / max_output_bytes / install_hint / disabled_commands
    /// （`common::config::DesktopConfig`，与 browser 同族但无 env / URL / file 字段）
    Desktop,
    /// gh_cli：command + 默认超时 / 默认输出上限（`common::config::CliToolConfig`）
    GhCli,
    /// lark_cli：同 `GhCli`（共用 `CliToolConfig`）
    LarkCli,
    /// tavily_search / doubao_search：timeout_ms 单字段
    /// （`common::config::SearchToolConfig`）
    SearchTool,
    /// shell_exec：受信任目录 / PATH 补全 / 工具链 / HOME 策略 / 默认超时与输出上限
    ShellExec,
    /// fs_read / fs_write：受信任目录单字段（`common::config::FsToolConfig`）
    Fs,
}

/// 按工具名匹配内置工具结构化表单；不匹配（MCP / Http / 未知 Builtin）→ None 回退只读 JSON
fn builtin_config_form(id: &str) -> Option<BuiltinConfigForm> {
    // 按工具 id 匹配（非显示名 name）：内置工具入库时 name 为「GitHub CLI」等
    // 人类可读名，详情页必须用 t.id（"gh_cli" 等）匹配才能命中结构化表单分支。
    match id {
        "browser" => Some(BuiltinConfigForm::Browser),
        "desktop" => Some(BuiltinConfigForm::Desktop),
        "gh_cli" => Some(BuiltinConfigForm::GhCli),
        "lark_cli" => Some(BuiltinConfigForm::LarkCli),
        "tavily_search" | "doubao_search" => Some(BuiltinConfigForm::SearchTool),
        "shell_exec" => Some(BuiltinConfigForm::ShellExec),
        "fs_read" | "fs_write" => Some(BuiltinConfigForm::Fs),
        _ => None,
    }
}

/// 配置表单状态（全部文本输入，提交时统一解析）
#[derive(Debug, Clone, Default, PartialEq)]
struct BuiltinConfigFormState {
    command: String,
    timeout_ms: String,
    max_output_bytes: String,
    install_hint: String,
    /// 默认超时（毫秒）；键 `default_timeout_ms`——`gh_cli`/`lark_cli`
    /// （`common::config::CliToolConfig`）与 `shell_exec`（`ShellExecConfig`）共用同一键名
    default_timeout_ms: String,
    /// 默认输出上限（字节）；键 `default_max_output_size_bytes`——同上两个结构体的共用字段
    default_max_output_size_bytes: String,
    /// shell_exec：PATH 补全目录（一行一个，支持 ~ 与 * 通配）
    path_additions: String,
    /// shell_exec：受信任目录（additional_allowed_paths，一行一个）
    ///
    /// 与 `path_additions`（PATH 补全）语义不同：此处是工作目录白名单，落入其中
    /// 的命令不再触发「目录越界 / 身份边界」两类确认，降低授权频率。
    /// 字段名与 `common::config::ShellExecConfig` 对齐，新增字段需同步两侧。
    additional_allowed_paths: String,
    /// shell_exec：隔离 HOME 下指回真实 HOME 的工具链名单（一行一个）
    toolchain_envs: String,
    /// shell_exec：HOME 策略（isolated / inherit，留空 = 用后端默认）
    home_mode: String,
    /// browser：启动环境变量行（键从 common 目录下拉选，值按条目形态输入）
    /// 存 (key, value) 列表；键空行提交时丢弃；与 `common::config::BrowserConfig.env` 对齐
    browser_env_rows: Vec<(String, String)>,
    /// browser：是否允许 open 访问 localhost / 内网（两态："true" = 允许；"false" = 拒绝）
    /// 与 `common::config::BrowserConfig.allow_local_network` 对齐；默认放行，回填时未配置归一为 "true"
    allow_local_network: String,
    /// browser：禁止的子命令黑名单（逗号分隔，留空 = 不屏蔽，开放 CLI 全部能力）
    /// 与 `common::config::BrowserConfig.disabled_commands` 对齐；管理员级可选配置
    disabled_commands: String,
    /// browser：是否允许 open 用 file:// 打开本地 HTML（默认放行，仅 false 完全禁用）
    /// 与 `common::config::BrowserConfig.allow_file_url` 对齐
    allow_file_url: String,
    /// browser：file:// 额外允许的目录（逗号分隔，绝对路径或相对 base_data_path），对齐 fs 的 additional_allowed_paths
    /// 与 `common::config::BrowserConfig.allowed_file_dirs` 对齐
    allowed_file_dirs: String,
}

/// browser 环境变量表单的「值输入形态」：与详情接口 DTO 对齐（choices 空 = 文本框）
#[derive(Debug, Clone, PartialEq, Eq)]
enum BrowserEnvFormKind {
    Text,
    /// 可选项列表（value, 展示文案）；value 空串 = 不设置
    Choice(Vec<(String, String)>),
}

/// browser 环境变量目录条目（前端视图模型；来自动态探测 DTO 或 common 静态兜底）
#[derive(Debug, Clone, PartialEq, Eq)]
struct BrowserEnvFormSpec {
    key: String,
    label: String,
    desc: String,
    hint: String,
    kind: BrowserEnvFormKind,
    advanced: bool,
    /// 本机探测到的推荐值（value, 展示文案），点击即填入
    recommended: Vec<(String, String)>,
    /// 是否来自 CLI 动态文档
    from_cli: bool,
    /// 是否允许写入（禁用项只展示）
    configurable: bool,
}

/// 从 common 静态兜底目录构建视图列表（CLI 未安装 / 探测失败 / 旧后端时使用）
fn browser_env_catalog() -> Vec<BrowserEnvFormSpec> {
    common::models::tool::BROWSER_ENV_CATALOG
        .iter()
        .map(|spec| BrowserEnvFormSpec {
            key: spec.key.to_string(),
            label: spec.label.to_string(),
            desc: spec.desc.to_string(),
            hint: spec.hint.to_string(),
            kind: match spec.kind {
                common::models::tool::BrowserEnvKind::Text => BrowserEnvFormKind::Text,
                common::models::tool::BrowserEnvKind::Choice(options) => {
                    BrowserEnvFormKind::Choice(
                        options
                            .iter()
                            .map(|(value, label)| ((*value).to_string(), (*label).to_string()))
                            .collect(),
                    )
                }
            },
            advanced: spec.advanced,
            recommended: Vec::new(),
            from_cli: false,
            configurable: true,
        })
        .collect()
}

/// 从详情接口带回的探测 DTO 构建视图列表（有 DTO 时优先于静态兜底）
fn browser_env_catalog_from_probe(
    probe: &common::api::BrowserEnvProbeDTO,
) -> Vec<BrowserEnvFormSpec> {
    probe
        .entries
        .iter()
        .map(|entry| BrowserEnvFormSpec {
            key: entry.key.clone(),
            label: entry.label.clone(),
            desc: entry.desc.clone(),
            hint: entry.hint.clone(),
            kind: if entry.choices.is_empty() {
                BrowserEnvFormKind::Text
            } else {
                BrowserEnvFormKind::Choice(
                    entry
                        .choices
                        .iter()
                        .map(|option| (option.value.clone(), option.label.clone()))
                        .collect(),
                )
            },
            advanced: entry.advanced,
            recommended: entry
                .recommended
                .iter()
                .map(|item| (item.value.clone(), item.label.clone()))
                .collect(),
            from_cli: entry.from_cli,
            configurable: entry.configurable,
        })
        .collect()
}

/// 按 key 查当前值形态（用于渲染该行的值输入框 / 下拉）
fn browser_env_kind(catalog: &[BrowserEnvFormSpec], key: &str) -> BrowserEnvFormKind {
    catalog
        .iter()
        .find(|spec| spec.key == key)
        .map(|spec| spec.kind.clone())
        .unwrap_or(BrowserEnvFormKind::Text)
}

/// 推荐值一键填入：已存在同名行则覆盖，否则新增
fn upsert_browser_env(mut form: Signal<BuiltinConfigFormState>, key: String, value: String) {
    let mut state = form.write();
    if let Some(row) = state.browser_env_rows.iter_mut().find(|(k, _)| k == &key) {
        row.1 = value;
    } else {
        state.browser_env_rows.push((key, value));
    }
}

/// 快捷预设：一键填入一组常用环境变量（覆盖式写入同名键，保留其它行）
/// 浏览器可执行路径不放这里——由后端本机探测出的推荐值在表单顶部一键选择，
/// 避免在前端写死 macOS 路径（Linux / CI 上会误导）。
const BROWSER_ENV_PRESETS: &[(&str, &[(&str, &str)])] = &[
    (
        "走 HTTP 代理",
        &[
            ("AGENT_BROWSER_PROXY", "http://127.0.0.1:7890"),
            ("AGENT_BROWSER_PROXY_BYPASS", "localhost,127.0.0.1"),
        ],
    ),
    (
        "CI / 无沙箱",
        &[("AGENT_BROWSER_ARGS", "--no-sandbox,--disable-dev-shm-usage")],
    ),
];

/// 从 detail config 初始化表单（config 非对象 / 字段缺失 → 空串，由占位符提示缺省值）
fn builtin_form_from_config(config: Option<&serde_json::Value>) -> BuiltinConfigFormState {
    let Some(object) = config.and_then(|c| c.as_object()) else {
        return BuiltinConfigFormState::default();
    };
    let text = |key: &str| {
        object
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    };
    let number = |key: &str| {
        object
            .get(key)
            .and_then(|v| v.as_u64())
            .map(|n| n.to_string())
            .unwrap_or_default()
    };
    let path_additions = object
        .get("path_additions")
        .and_then(|v| v.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    let toolchain_envs = object
        .get("toolchain_envs")
        .and_then(|v| v.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    let additional_allowed_paths = object
        .get("additional_allowed_paths")
        .and_then(|v| v.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    BuiltinConfigFormState {
        command: text("command"),
        timeout_ms: number("timeout_ms"),
        max_output_bytes: number("max_output_bytes"),
        install_hint: text("install_hint"),
        default_timeout_ms: number("default_timeout_ms"),
        default_max_output_size_bytes: number("default_max_output_size_bytes"),
        path_additions,
        additional_allowed_paths,
        toolchain_envs,
        home_mode: text("home_mode"),
        allow_local_network: object
            .get("allow_local_network")
            .and_then(|v| v.as_bool())
            .map(|v| v.to_string())
            .unwrap_or_else(|| "true".to_string()),
        disabled_commands: object
            .get("disabled_commands")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default(),
        allow_file_url: object
            .get("allow_file_url")
            .and_then(|v| v.as_bool())
            .map(|v| v.to_string())
            .unwrap_or_else(|| "true".to_string()),
        allowed_file_dirs: object
            .get("allowed_file_dirs")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default(),
        // browser：env 对象 → 有序行（按 key 排序，保证回填顺序稳定）
        browser_env_rows: object
            .get("env")
            .and_then(|v| v.as_object())
            .map(|env| {
                let mut rows: Vec<(String, String)> = env
                    .iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect();
                rows.sort_by(|a, b| a.0.cmp(&b.0));
                rows
            })
            .unwrap_or_default(),
    }
}

/// 以 detail config 为基底合并表单字段（防丢字段：未在表单展示的字段原样保留）
///
/// 合并后统一走 common 单点 `validate_builtin_tool_config`（与后端 update_tool 校验
/// 同一实现）：command 非空 string；timeout_ms / max_output_bytes 正整数
/// （留空 = 不覆盖，保留基底值或后端缺省兜底）。
fn merge_builtin_config(
    base: Option<&serde_json::Value>,
    form: &BuiltinConfigFormState,
    layout: BuiltinConfigForm,
) -> Result<serde_json::Value, String> {
    let mut map = base
        .and_then(|c| c.as_object())
        .cloned()
        .unwrap_or_default();
    // `credential_requirements` 在详情回显里被 redact 成 `[REDACTED]` 字符串（不可信）；
    // 真实声明走 UpdateToolRequest 顶层字段单独提交（见下方「注入策略」卡片）。
    // 此处必须剔除：否则「保存配置」会把占位串写回 config，运行时反序列化失败 →
    // 回落到工厂默认，注入策略被静默清空。
    map.remove("credential_requirements");
    match layout {
        BuiltinConfigForm::Browser => {
            insert_command(&mut map, &form.command);
            insert_positive_number(&mut map, "timeout_ms", &form.timeout_ms);
            insert_positive_number(&mut map, "max_output_bytes", &form.max_output_bytes);
            map.insert(
                "install_hint".to_string(),
                serde_json::Value::String(form.install_hint.trim().to_string()),
            );
            insert_browser_env(&mut map, &form.browser_env_rows)?;
            insert_bool_field(&mut map, "allow_local_network", &form.allow_local_network)?;
            insert_comma_list(&mut map, "disabled_commands", &form.disabled_commands);
            insert_bool_field(&mut map, "allow_file_url", &form.allow_file_url)?;
            insert_comma_list(&mut map, "allowed_file_dirs", &form.allowed_file_dirs);
        }
        BuiltinConfigForm::Desktop => {
            // 与 browser 同族的基础运行参数，但无 env / URL / file 字段
            insert_command(&mut map, &form.command);
            insert_positive_number(&mut map, "timeout_ms", &form.timeout_ms);
            insert_positive_number(&mut map, "max_output_bytes", &form.max_output_bytes);
            map.insert(
                "install_hint".to_string(),
                serde_json::Value::String(form.install_hint.trim().to_string()),
            );
            insert_comma_list(&mut map, "disabled_commands", &form.disabled_commands);
        }
        BuiltinConfigForm::GhCli | BuiltinConfigForm::LarkCli => {
            insert_command(&mut map, &form.command);
            // 键名对齐 common::config::CliToolConfig
            insert_positive_number(&mut map, "default_timeout_ms", &form.default_timeout_ms);
            insert_positive_number(
                &mut map,
                "default_max_output_size_bytes",
                &form.default_max_output_size_bytes,
            );
        }
        BuiltinConfigForm::SearchTool => {
            // 键名对齐 common::config::SearchToolConfig（tavily_search / doubao_search 共用）
            insert_positive_number(&mut map, "timeout_ms", &form.timeout_ms);
        }
        BuiltinConfigForm::ShellExec => {
            // 键名对齐 common::config::ShellExecConfig（按结构体字段顺序书写）
            insert_positive_number(&mut map, "default_timeout_ms", &form.default_timeout_ms);
            insert_positive_number(
                &mut map,
                "default_max_output_size_bytes",
                &form.default_max_output_size_bytes,
            );
            insert_additional_allowed_paths(&mut map, &form.additional_allowed_paths);
            insert_path_additions(&mut map, &form.path_additions);
            insert_toolchain_envs(&mut map, &form.toolchain_envs)?;
            insert_home_mode(&mut map, &form.home_mode)?;
        }
        BuiltinConfigForm::Fs => {
            // 键名对齐 common::config::FsToolConfig（fs_read / fs_write 共用）
            insert_additional_allowed_paths(&mut map, &form.additional_allowed_paths);
        }
    }
    let merged = serde_json::Value::Object(map);
    common::models::validate_builtin_tool_config(&merged)?;
    Ok(merged)
}

/// command 覆盖（原样放置，合法性由合并后的 common 校验统一判定）
fn insert_command(map: &mut serde_json::Map<String, serde_json::Value>, command: &str) {
    map.insert(
        "command".to_string(),
        serde_json::Value::String(command.trim().to_string()),
    );
}

/// path_additions 覆盖：按行拆分、去空行；全部为空时删除该键（回退后端内置默认）
fn insert_path_additions(map: &mut serde_json::Map<String, serde_json::Value>, text: &str) {
    let entries: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect();
    if entries.is_empty() {
        map.remove("path_additions");
    } else {
        map.insert(
            "path_additions".to_string(),
            serde_json::Value::Array(entries.into_iter().map(serde_json::Value::String).collect()),
        );
    }
}

/// additional_allowed_paths 覆盖：按行拆分、去空行；全部为空时删除该键（回退默认 = 不受信任目录白名单）
///
/// 与 `insert_path_additions`（PATH 补全，可执行性）语义不同：此处是工作目录白名单，
/// 落入其中的命令不再触发「目录越界 / 身份边界」两类确认，降低授权频率。
fn insert_additional_allowed_paths(
    map: &mut serde_json::Map<String, serde_json::Value>,
    text: &str,
) {
    let entries: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect();
    if entries.is_empty() {
        map.remove("additional_allowed_paths");
    } else {
        map.insert(
            "additional_allowed_paths".to_string(),
            serde_json::Value::Array(entries.into_iter().map(serde_json::Value::String).collect()),
        );
    }
}

/// toolchain_envs 覆盖：按行拆分、去空行、去重；全部为空时删除该键（不注入）
///
/// 合法名单取 common 单点 `SHELL_TOOLCHAIN_HOME_VARS`（与后端注入同一实现），
/// 未知名直接报错（带支持列表），避免保存一个后端静默忽略的配置。
fn insert_toolchain_envs(
    map: &mut serde_json::Map<String, serde_json::Value>,
    text: &str,
) -> Result<(), String> {
    let mut entries: Vec<String> = Vec::new();
    for line in text.lines() {
        let name = line.trim().to_lowercase();
        if name.is_empty() || entries.contains(&name) {
            continue;
        }
        if !common::models::tool::is_supported_toolchain(&name) {
            let supported = common::models::tool::SHELL_TOOLCHAIN_HOME_VARS
                .iter()
                .map(|(name, _, _)| *name)
                .collect::<Vec<_>>()
                .join(" / ");
            return Err(format!(
                "config.toolchain_envs 含不支持的工具链「{name}」，支持：{supported}"
            ));
        }
        entries.push(name);
    }
    if entries.is_empty() {
        map.remove("toolchain_envs");
    } else {
        map.insert(
            "toolchain_envs".to_string(),
            serde_json::Value::Array(entries.into_iter().map(serde_json::Value::String).collect()),
        );
    }
    Ok(())
}

/// home_mode 覆盖：留空不覆盖（保留基底 / 缺省兜底）；非法值由 common 校验拦截前先本地拦截
fn insert_home_mode(
    map: &mut serde_json::Map<String, serde_json::Value>,
    text: &str,
) -> Result<(), String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    if trimmed != "isolated" && trimmed != "inherit" {
        return Err("config.home_mode 只支持 isolated / inherit".to_string());
    }
    map.insert(
        "home_mode".to_string(),
        serde_json::Value::String(trimmed.to_string()),
    );
    Ok(())
}

/// 布尔字段覆盖：留空不覆盖（保留基底 / 后端默认）；"true"/"false" 写布尔，其余报错
fn insert_bool_field(
    map: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    text: &str,
) -> Result<(), String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let value = match trimmed {
        "true" => true,
        "false" => false,
        _ => return Err(format!("config.{key} 只能是 true / false")),
    };
    map.insert(key.to_string(), serde_json::Value::Bool(value));
    Ok(())
}

/// 逗号分隔字符串 → 字符串数组：留空则删除该键（回退「不限制」默认），去重保序
fn insert_comma_list(map: &mut serde_json::Map<String, serde_json::Value>, key: &str, text: &str) {
    let mut seen = std::collections::BTreeSet::new();
    let items: Vec<serde_json::Value> = text
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && seen.insert(s.to_string()))
        .map(|s| serde_json::Value::String(s.to_string()))
        .collect();
    if items.is_empty() {
        map.remove(key);
    } else {
        map.insert(key.to_string(), serde_json::Value::Array(items));
    }
}

/// browser env 覆盖：按行写入 env 对象；键空行丢弃，全部为空时删除该键（回退默认）。
/// 合法性（仅 `AGENT_BROWSER_` 前缀 + 字符串值）统一由 common 的
/// `validate_builtin_tool_config` → `validate_browser_env` 判定，前端不重复规则（SSOT）。
/// 风险项（headed / CDP / 凭据等）照常写入——它们在页面附风险说明，由管理员自行取舍。
fn insert_browser_env(
    map: &mut serde_json::Map<String, serde_json::Value>,
    rows: &[(String, String)],
) -> Result<(), String> {
    let mut env = serde_json::Map::new();
    for (key, value) in rows {
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        env.insert(
            key.to_string(),
            serde_json::Value::String(value.trim().to_string()),
        );
    }
    if env.is_empty() {
        map.remove("env");
    } else {
        map.insert("env".to_string(), serde_json::Value::Object(env));
    }
    Ok(())
}

/// browser 环境变量单行编辑器（抽出组件以便在正常函数体里做 let 计算，
/// 避免 RSX 子节点内不能写裸 let 的限制）
#[component]
fn BrowserEnvRow(
    /// 行索引（用于对 config_form 的对应行做原地修改）
    idx: usize,
    /// 当前键
    env_key: String,
    /// 当前值
    env_value: String,
    /// 目录（下拉选项 / 说明来源）
    catalog: Vec<BrowserEnvFormSpec>,
    /// 配置表单信号（写入键 / 值 / 删除行）
    config_form: Signal<BuiltinConfigFormState>,
) -> Element {
    let spec = catalog.iter().find(|s| s.key == env_key).cloned();
    // 风险项（advanced，如 headed / CDP / 凭据）照常可填，仅在说明里标 warning；
    // 不再有只读禁用——CLI 天然支持这些能力，硬禁只会逼人走 shell_exec 绕过。
    let kind = browser_env_kind(&catalog, &env_key);
    let recommendations = spec
        .as_ref()
        .map(|entry| entry.recommended.clone())
        .unwrap_or_default();
    rsx! {
        div { class: "mb-3 p-3 rounded-lg border border-base-300/60 bg-base-200/30",
            div { class: "flex items-start gap-2",
                div { class: "flex-1 min-w-0 grid grid-cols-1 md:grid-cols-2 gap-2",
                    // 变量下拉：选项用原生 selected 表达选中态（见 agent_detail.rs 同款处理）
                    select {
                        class: "select select-bordered hud-input w-full text-sm",
                        value: "{env_key}",
                        onchange: move |e| {
                            let new_key = e.value();
                            let mut form = config_form.write();
                            if let Some(row) = form.browser_env_rows.get_mut(idx) {
                                // 切换变量时清空旧值，避免留着不匹配的枚举值
                                row.0 = new_key;
                                row.1.clear();
                            }
                        },
                        option { value: "", selected: env_key.is_empty(), "— 选择环境变量 —" }
                        for entry in &catalog {
                            option {
                                value: "{entry.key}",
                                selected: entry.key == env_key,
                                if entry.advanced {
                                    "{entry.label} · 高级 / 有风险"
                                } else {
                                    "{entry.label}"
                                }
                            }
                        }
                    }
                    // 值输入：枚举项下拉 / 其余文本框
                    match kind {
                        BrowserEnvFormKind::Choice(options) => rsx! {
                            select {
                                class: "select select-bordered hud-input w-full text-sm",
                                value: "{env_value}",
                                onchange: move |e| {
                                    if let Some(row) = config_form.write().browser_env_rows.get_mut(idx) {
                                        row.1 = e.value();
                                    }
                                },
                                for (value, label) in &options {
                                    option {
                                        value: "{value}",
                                        selected: value == &env_value,
                                        "{label}"
                                    }
                                }
                            }
                        },
                        BrowserEnvFormKind::Text => rsx! {
                            input {
                                class: "input input-bordered hud-input w-full font-mono text-sm",
                                value: "{env_value}",
                                oninput: move |e| {
                                    if let Some(row) = config_form.write().browser_env_rows.get_mut(idx) {
                                        row.1 = e.value();
                                    }
                                },
                                placeholder: spec.as_ref().map(|s| s.hint.as_str()).unwrap_or("值"),
                            }
                        },
                    }
                }
                button {
                    r#type: "button",
                    class: "btn btn-ghost btn-xs hud-btn mt-1",
                    title: "移除此变量",
                    onclick: move |_| {
                        let mut form = config_form.write();
                        if idx < form.browser_env_rows.len() {
                            form.browser_env_rows.remove(idx);
                        }
                    },
                    "✕"
                }
            }
            // 本机探测推荐值：点击直接填入，不用手敲路径
            if !recommendations.is_empty() {
                div { class: "mt-2 flex flex-wrap items-center gap-2",
                    span { class: "text-xs opacity-60", "推荐值：" }
                    for (value, label) in recommendations {
                        button {
                            r#type: "button",
                            class: "btn btn-xs btn-outline hud-btn",
                            title: "{value}",
                            onclick: move |_| {
                                let mut form = config_form.write();
                                if let Some(row) = form.browser_env_rows.get_mut(idx) {
                                    row.1 = value.clone();
                                }
                            },
                            "{label}"
                        }
                    }
                }
            }
            // 选中变量后展示含义 + 可填值引导
            if let Some(entry) = spec {
                div { class: "mt-2",
                    HudCallout {
                        tone: Some(if entry.advanced {
                            "warning".to_string()
                        } else {
                            "info".to_string()
                        }),
                        extra_class: Some("py-2 text-xs leading-relaxed".to_string()),
                        p { class: "font-medium mb-0.5",
                            "{entry.key}"
                            if entry.from_cli {
                                span { class: "badge badge-xs badge-info ml-2", "CLI 提供" }
                            }
                            if entry.advanced {
                                span { class: "badge badge-xs badge-warning ml-1", "有风险，请确认" }
                            }
                        }
                        p { class: "mb-0.5", "{entry.desc}" }
                        p { class: "opacity-70 mb-0", "{entry.hint}" }
                    }
                }
            }
        }
    }
}

/// 探测推荐值总览：把所有变量的本机推荐值提到表单顶部，一键写入对应变量
#[component]
fn BrowserDetectedOptions(
    catalog: Vec<BrowserEnvFormSpec>,
    config_form: Signal<BuiltinConfigFormState>,
) -> Element {
    let detected: Vec<(String, String, String)> = catalog
        .iter()
        .flat_map(|entry| {
            let key = entry.key.clone();
            entry
                .recommended
                .iter()
                .map(move |(value, label)| (key.clone(), value.clone(), label.clone()))
        })
        .collect();
    if detected.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "mb-4 p-3 rounded-lg border border-info/30 bg-info/5",
            p { class: "text-xs font-medium mb-1", "本机探测到的可用选项" }
            p { class: "text-xs opacity-60 mb-2",
                "来自 agent-browser 运行环境的实时探测，点击即填入对应变量（可再手改）。"
            }
            div { class: "flex flex-wrap gap-2",
                for (key, value, label) in detected {
                    button {
                        r#type: "button",
                        class: "btn btn-sm btn-outline hud-btn",
                        title: "{key} = {value}",
                        onclick: move |_| upsert_browser_env(config_form, key.clone(), value.clone()),
                        "{label}"
                    }
                }
            }
        }
    }
}

/// 数字字段覆盖：留空不覆盖（保留基底 / 缺省兜底）；非数字原样放置交由 common 校验判定
fn insert_positive_number(
    map: &mut serde_json::Map<String, serde_json::Value>,
    field: &str,
    text: &str,
) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    let value = trimmed
        .parse::<u64>()
        .map(|number| serde_json::json!(number))
        .unwrap_or_else(|_| serde_json::Value::String(trimmed.to_string()));
    map.insert(field.to_string(), value);
}

#[component]
pub fn FinanceToolDetail(id: String) -> Element {
    // 方案 B：响应式 rid + use_resource，拉取仅在 :id 变化时触发
    let route = dioxus_router::use_route::<crate::pages::Route>();
    let mut rid = use_signal(String::new);
    if let crate::pages::Route::FinanceToolDetail { id: route_id } = &route
        && *rid.peek() != *route_id
    {
        rid.set(route_id.clone());
    }
    // 统计时间窗口（时间筛选器产出，默认最近 7 天）：变化时 use_resource 重拉
    let mut stats_range = use_signal(TimeRange::default);
    // 稳定回调（hook 必须在组件顶层无条件调用，不能写在 rsx 条件分支里）
    let on_stats_range = use_callback(move |r: TimeRange| stats_range.set(r));
    let mut tool_res = use_resource(move || {
        let id = rid();
        let range = stats_range();
        async move { get_tool(build_tool_stats_request(id, range)).await }
    });
    let toast = use_toast();
    let navigator = use_navigator();

    // ===== 删除确认对话框 =====
    let mut show_delete_confirm = use_signal(|| false);
    let mut pending_delete_id = use_signal(String::new);

    // 调试面板状态
    let mut debug_args = use_signal(|| "{}".to_string());
    let mut debug_result = use_signal(|| None::<String>);
    let mut debug_calling = use_signal(|| false);

    // 工具配置编辑状态（Builtin 结构化字段；文本输入，提交时统一解析）
    let mut config_form = use_signal(BuiltinConfigFormState::default);
    // browser env 目录：详情接口的动态探测优先，探测失败 / 旧后端回落 common 静态兜底
    let env_catalog = tool_res
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .and_then(|tool| tool.browser_env_probe.as_ref())
        .map(browser_env_catalog_from_probe)
        .unwrap_or_else(browser_env_catalog);
    // 探测来源版本（仅用于表单标注；None = 静态兜底）
    let browser_probe_version = tool_res
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .and_then(|tool| tool.browser_env_probe.as_ref())
        .and_then(|probe| probe.cli_version.clone());
    let env_catalog_hint = match &browser_probe_version {
        Some(version) => {
            // CLI 自带文档只有一张 16 项的环境变量表，内置精选里其余项并非 CLI 实时
            // 上报——如实标注两个来源，避免让用户误以为整张表都来自当前版本。
            let cli_count = env_catalog.iter().filter(|entry| entry.from_cli).count();
            let builtin_count = env_catalog.len().saturating_sub(cli_count);
            format!(
                "agent-browser {version} 实时探测 {} 项（标「CLI 提供」）+ 内置精选 {} 项；标「只读」的是 CLI 有但安全策略禁用的项。点击推荐值或预设即可填入，无需手敲。",
                cli_count, builtin_count
            )
        }
        None => {
            "目录来自内置兜底（未探测到 agent-browser）；变量名从下方选择，无需手敲。".to_string()
        }
    };
    // 当前 env 行快照（signal 读取建立响应式依赖；clone 避免遍历时持有 borrow 与写入冲突）
    let browser_env_rows = config_form.read().browser_env_rows.clone();
    let mut config_saving = use_signal(|| false);
    // 用户自建工具（HTTP / Shell）配置编辑状态：直接复用创建页的协议子表单
    // （`HttpToolSubForm` / `ShellToolSubForm`），避免详情页另写一套字段与校验
    let mut http_form = use_signal(HttpToolFormState::default);
    let mut shell_form = use_signal(ShellToolFormState::default);
    let mut custom_config_saving = use_signal(|| false);
    // 注入策略编辑（Builtin：两 Tab —— 「凭据」/「通用（身份）」共享同一个
    // credential_requirements 数组；草稿以服务端回读为准，保存后重拉覆盖）
    let mut requirements_draft = use_signal(Vec::<CredentialRequirement>::new);
    let mut requirements_saving = use_signal(|| false);
    // 运行时就绪（详情响应无此字段，经 query_tools 与列表 badge 同源探测）
    let runtime_ready = use_resource(move || {
        let id = rid();
        async move {
            query_tools(&ToolQueryRequest {
                ids: Some(vec![id]),
                ..Default::default()
            })
            .await
            .ok()
            .and_then(|page| page.items.into_iter().next().map(|item| item.runtime_ready))
            .unwrap_or_default()
        }
    });

    // tool_data / debug_args / config_form 同步（方案 B）：从 tool_res 派生，
    // 运行时就绪由上方独立 runtime_ready resource 负责
    use_effect(move || {
        if let Some(tool) = tool_res.read().as_ref().and_then(|r| r.as_ref().ok()) {
            if let Some(ref schema) = tool.parameters_schema {
                debug_args.set(generate_skeleton_from_schema(schema));
            }
            config_form.set(builtin_form_from_config(tool.config.as_ref()));
            // 注入策略草稿：详情回读（含保存后重拉）以服务端为准 —— 两 Tab 复用同一回填
            requirements_draft.set(tool.credential_requirements.clone());
            // 用户自建工具（HTTP / Shell）：config 反填协议子表单（与创建页同一字段集）
            if matches!(tool.protocol, ToolProtocol::Http | ToolProtocol::Shell) {
                let mut http = http_form_from_config(tool.config.as_ref());
                let mut shell = shell_form_from_config(tool.config.as_ref());
                // config 回显会把 credential_requirements 打成 [REDACTED]，
                // 凭据声明以顶层字段为准，否则保存时会被静默清空
                let requirements = tool.credential_requirements.clone();
                http.credential_requirements = requirements.clone();
                shell.credential_requirements = requirements;
                http_form.set(http);
                shell_form.set(shell);
            }
        }
    });

    rsx! {
        AppLayout {
            PageHeader {
                eyebrow: "FINANCE".to_string(),
                title: "工具详情".to_string(),
                actions: Some(rsx! {
                    Link { class: "btn hud-btn btn-ghost", to: crate::pages::Route::FinanceTools {},
                        "← 返回列表"
                    }
                }),
            }

            match tool_res.read().as_ref() {
                None => rsx! { Loading {} },
                Some(Ok(t)) => {
                    let t = t.clone();
                    rsx! {
                HudPanel {
                    title: t.name.clone(),
                    eyebrow: "TOOL".to_string(),
                    signal: true,
                    div { class: "card-body",
                        div { class: "flex justify-end mb-4",
                            div { class: "flex gap-2",
                                if t.enabled {
                                    button { class: "btn hud-btn btn-outline btn-sm",
                                        onclick: {
                                            let id = t.id.clone();
                                            move |_| {
                                                let id = id.clone();
                                                spawn(async move {
                                                    if let Err(e) = update_tool_status(UpdateToolStatusRequest { id: id.clone(), status: ToolStatus::Disabled }).await {
                                                        toast.error(&e);
                                                    } else {
                                                        toast.success("已禁用");
                                                        if let Ok(tool) = get_tool(build_tool_stats_request(id, stats_range())).await {
                                                            tool_res.set(Some(Ok(tool)));
                                                        }
                                                    }
                                                });
                                            }
                                        },
                                        "禁用"
                                    }
                                } else {
                                    button { class: "btn hud-btn btn-primary btn-sm",
                                        onclick: {
                                            let id = t.id.clone();
                                            move |_| {
                                                let id = id.clone();
                                                spawn(async move {
                                                    if let Err(e) = update_tool_status(UpdateToolStatusRequest { id: id.clone(), status: ToolStatus::Enabled }).await {
                                                        toast.error(&e);
                                                    } else {
                                                        toast.success("已启用");
                                                        if let Ok(tool) = get_tool(build_tool_stats_request(id, stats_range())).await {
                                                            tool_res.set(Some(Ok(tool)));
                                                        }
                                                    }
                                                });
                                            }
                                        },
                                        "启用"
                                    }
                                }
                                button { class: "btn hud-btn btn-error btn-sm",
                                    onclick: {
                                        let id = t.id.clone();
                                        move |_| {
                                            pending_delete_id.set(id.clone());
                                            show_delete_confirm.set(true);
                                        }
                                    },
                                    "删除"
                                }
                            }
                        }
                        div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                            div { class: "md:col-span-2",
                                label { class: "label",
                                    span { class: "label-text font-medium", "描述" }
                                }
                                MarkdownRenderer { content: t.description.clone(), compact: true }
                            }
                            div {
                                label { class: "label",
                                    span { class: "label-text font-medium", "协议" }
                                }
                                div { span { class: "badge orz-tag badge-sm", "{t.protocol}" } }
                            }
                            div {
                                label { class: "label",
                                    span { class: "label-text font-medium", "状态" }
                                }
                                div {
                                    if t.enabled {
                                        span { class: "badge hud-badge badge-success", "启用" }
                                    } else {
                                        span { class: "badge hud-badge badge-error", "禁用" }
                                    }
                                }
                            }
                            div {
                                label { class: "label",
                                    span { class: "label-text font-medium", "控制模式" }
                                }
                                div { "{t.control_mode}" }
                            }
                            if !t.tags.is_empty() {
                                div { class: "md:col-span-2",
                                    label { class: "label",
                                        span { class: "label-text font-medium", "标签" }
                                    }
                                    div { class: "flex flex-wrap gap-2",
                                        for tag in t.tags.iter() {
                                            span { class: "badge orz-tag badge-sm", "{tag}" }
                                        }
                                    }
                                }
                            }
                            div {
                                label { class: "label",
                                    span { class: "label-text font-medium", "工具 ID" }
                                }
                                div { class: "font-mono text-sm", "{t.id}" }
                            }
                        }
                    }
                }

                // ===== 工具配置卡片（Builtin 结构化编辑；HTTP/Shell 可编辑 JSON；MCP/其它只读）=====
                if builtin_config_form(&t.id).is_some()
                    || t.has_config
                    || matches!(t.protocol, ToolProtocol::Http | ToolProtocol::Shell)
                {
                    HudPanel {
                        title: "工具配置".to_string(),
                        eyebrow: "CONFIG".to_string(),
                        div { class: "card-body",
                            if let Some(layout) = builtin_config_form(&t.id) {
                                // —— Builtin 结构化表单（D28：CLI 命令与行为参数）——
                                if layout == BuiltinConfigForm::Browser {
                                    div { class: "flex flex-col gap-5",
                                        // —— 基础运行参数 ——
                                        div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                                            div {
                                                label { class: "label",
                                                    span { class: "label-text font-medium", "命令 (command)" }
                                                    span { class: "label-text-alt text-base-content/60", "CLI 本体路径" }
                                                }
                                                input {
                                                    class: "input input-bordered hud-input w-full font-mono text-sm",
                                                    value: "{config_form.read().command}",
                                                    oninput: move |e| config_form.write().command = e.value(),
                                                    placeholder: "agent-browser",
                                                }
                                            }
                                            div {
                                                label { class: "label",
                                                    span { class: "label-text font-medium", "安装引导 (install_hint)" }
                                                }
                                                input {
                                                    class: "input input-bordered hud-input w-full text-sm",
                                                    value: "{config_form.read().install_hint}",
                                                    oninput: move |e| config_form.write().install_hint = e.value(),
                                                    placeholder: "brew install agent-browser 或 cargo install agent-browser",
                                                }
                                            }
                                            div {
                                                label { class: "label",
                                                    span { class: "label-text font-medium", "超时毫秒 (timeout_ms)" }
                                                    span { class: "label-text-alt text-base-content/60", "外层进程" }
                                                }
                                                input {
                                                    class: "input input-bordered hud-input w-full text-sm",
                                                    value: "{config_form.read().timeout_ms}",
                                                    oninput: move |e| config_form.write().timeout_ms = e.value(),
                                                    placeholder: "60000",
                                                }
                                            }
                                            div {
                                                label { class: "label",
                                                    span { class: "label-text font-medium", "输出上限字节 (max_output_bytes)" }
                                                }
                                                input {
                                                    class: "input input-bordered hud-input w-full text-sm",
                                                    value: "{config_form.read().max_output_bytes}",
                                                    oninput: move |e| config_form.write().max_output_bytes = e.value(),
                                                    placeholder: "262144",
                                                }
                                            }
                                        }

                                        // —— 启动环境变量：探测推荐 + 快捷预设 ——
                                        div {
                                            p { class: "text-sm font-medium mb-1", "启动环境变量" }
                                            p { class: "text-xs opacity-60 mb-2", "{env_catalog_hint}" }
                                            BrowserDetectedOptions {
                                                catalog: env_catalog.clone(),
                                                config_form,
                                            }
                                            div { class: "flex flex-wrap gap-2",
                                                for (preset_name, preset_pairs) in BROWSER_ENV_PRESETS {
                                                    button {
                                                        r#type: "button",
                                                        class: "btn btn-sm btn-outline hud-btn",
                                                        onclick: {
                                                            let pairs = *preset_pairs;
                                                            move |_| {
                                                                let mut form = config_form.write();
                                                                for (key, value) in pairs {
                                                                    if let Some(row) = form.browser_env_rows.iter_mut().find(|(k, _)| k == key) {
                                                                        row.1 = value.to_string();
                                                                    } else {
                                                                        form.browser_env_rows.push((key.to_string(), value.to_string()));
                                                                    }
                                                                }
                                                            }
                                                        },
                                                        "{preset_name}"
                                                    }
                                                }
                                            }
                                        }

                                        // —— 启动环境变量：动态行 ——
                                        div {
                                            for (idx, (env_key, env_value)) in browser_env_rows.iter().enumerate() {
                                                BrowserEnvRow {
                                                    idx,
                                                    env_key: env_key.clone(),
                                                    env_value: env_value.clone(),
                                                    catalog: env_catalog.clone(),
                                                    config_form,
                                                }
                                            }
                                            button {
                                                r#type: "button",
                                                class: "btn btn-sm btn-ghost hud-btn mt-1",
                                                onclick: move |_| {
                                                    config_form.write().browser_env_rows.push((String::new(), String::new()));
                                                },
                                                "+ 添加环境变量"
                                            }
                                        }

                                        // —— 可选配置：禁用的子命令黑名单（留空 = 不屏蔽） ——
                                        div { class: "border-t border-base-300/60 pt-4",
                                            label { class: "label",
                                                span { class: "label-text font-medium", "禁用的子命令 (disabled_commands)" }
                                                span { class: "label-text-alt text-base-content/60", "留空 = 开放全部" }
                                            }
                                            input {
                                                class: "input input-bordered hud-input w-full font-mono text-sm",
                                                value: "{config_form.read().disabled_commands}",
                                                oninput: move |e| config_form.write().disabled_commands = e.value(),
                                                placeholder: "留空不屏蔽；如需拉黑填 eval, cookies, ...",
                                            }
                                            p { class: "text-xs opacity-60 mt-1",
                                                "默认开放 agent-browser 的全部命令（含 eval / batch / tab / cookies 等）。只有明确有风险的命令才填进这个逗号分隔的黑名单——不必枚举其余命令，CLI 升级新增命令也默认可用。用 browser 工具而非 shell 的价值在于自动 session 隔离、URL 入口检查与截图产物归档——不是靠禁命令。"
                                            }
                                        }

                                        // —— 内网访问开关（默认放行，管理员可收紧） ——
                                        div { class: "border-t border-base-300/60 pt-4",
                                            label { class: "label",
                                                span { class: "label-text font-medium", "本地 / 内网页面访问 (allow_local_network)" }
                                            }
                                            select {
                                                class: "select select-bordered hud-input w-full text-sm",
                                                value: "{config_form.read().allow_local_network}",
                                                onchange: move |e| config_form.write().allow_local_network = e.value(),
                                                option { value: "true", selected: config_form.read().allow_local_network != "false", "允许本地 / 内网（测试场景开箱可用）" }
                                                option { value: "false", selected: config_form.read().allow_local_network == "false", "明确拒绝本地 / 内网" }
                                            }
                                            p { class: "text-xs opacity-60 mt-1",
                                                "默认允许 Agent 用 open 打开 localhost / 内网地址，方便测试本机 dev server。仅当部署环境需要收紧时选第二项。云元数据端点（169.254.169.254 等）在任何选项下都会被拒绝。此开关只能在此配置，Agent 无法自行打开。"
                                            }
                                        }

                                        // —— 本地 HTML 文件访问（允许/不允许两种；允许时可配额外目录白名单） ——
                                        div { class: "border-t border-base-300/60 pt-4",
                                            label { class: "label",
                                                span { class: "label-text font-medium", "本地 HTML 文件访问 (file://)" }
                                            }
                                            select {
                                                class: "select select-bordered hud-input w-full text-sm",
                                                value: "{config_form.read().allow_file_url}",
                                                onchange: move |e| config_form.write().allow_file_url = e.value(),
                                                option { value: "true", selected: config_form.read().allow_file_url != "false", "允许" }
                                                option { value: "false", selected: config_form.read().allow_file_url == "false", "完全禁用 file://" }
                                            }
                                            label { class: "label mt-2",
                                                span { class: "label-text font-medium", "额外允许的目录 (allowed_file_dirs)" }
                                                span { class: "label-text-alt text-base-content/60", "绝对路径或相对数据目录" }
                                            }
                                            input {
                                                class: "input input-bordered hud-input w-full font-mono text-sm",
                                                value: "{config_form.read().allowed_file_dirs}",
                                                oninput: move |e| config_form.write().allowed_file_dirs = e.value(),
                                                placeholder: "如 /data/html, /tmp/preview；留空 = 仅数据目录内",
                                            }
                                            p { class: "text-xs opacity-60 mt-1",
                                                "选择允许时，Agent 可用 file:// 打开本地 HTML：数据目录内的文件直接打开；上方白名单里的目录也视为可信、直接打开。其他目录不会硬拒，而是让 Agent 停下来向你请求确认（require_confirmation），你同意后可把目录加进白名单再让它重试。路径校验会拦截符号链接逃逸、../ 穿越和敏感文件。选择完全禁用则 file:// 一律不可用。此开关 Agent 无法自行打开。"
                                            }
                                        }

                                        // —— 边界说明 ——
                                        HudCallout {
                                            tone: Some("info".to_string()),
                                            extra_class: Some("py-2 text-xs leading-relaxed".to_string()),
                                            p { class: "font-medium mb-0.5", "边界说明" }
                                            p { class: "mb-0.5",
                                                "URL 检查只对 open 入口做字面过滤（协议 / 云元数据），本地内网默认放行；不做 DNS 解析，302 跳转、页面内 fetch、点击跳转不受此检查约束。完整边界靠浏览器级域名白名单（AGENT_BROWSER_ALLOWED_DOMAINS）与部署层网络隔离。"
                                            }
                                            p { class: "mb-0.5",
                                                "仅允许 AGENT_BROWSER_* 前缀变量——这是防 PATH / LD_PRELOAD 劫持子进程的必要边界。前缀内变量全部可配（目录是引导不是白名单）；headed / session / CDP / 云浏览器 / 加密密钥等标为「有风险」，由你自行判断是否启用。"
                                            }
                                            p { class: "opacity-70 mb-0",
                                                "标记为「CLI 提供」的项是 agent-browser 运行时自报的能力，无需我们维护清单；带凭据的代理推荐不会回显。变量值仅作用于本机启动的 browser 会话。"
                                            }
                                        }
                                    }
                                } else if layout == BuiltinConfigForm::Desktop {
                                    // desktop：与 browser 同族的基础运行参数 + 子命令黑名单，
                                    // 无 env / URL / file 字段（agent-desktop 无这些入口）
                                    div { class: "flex flex-col gap-5",
                                        div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                                            div {
                                                label { class: "label",
                                                    span { class: "label-text font-medium", "命令 (command)" }
                                                    span { class: "label-text-alt text-base-content/60", "CLI 本体路径" }
                                                }
                                                input {
                                                    class: "input input-bordered hud-input w-full font-mono text-sm",
                                                    value: "{config_form.read().command}",
                                                    oninput: move |e| config_form.write().command = e.value(),
                                                    placeholder: "agent-desktop",
                                                }
                                            }
                                            div {
                                                label { class: "label",
                                                    span { class: "label-text font-medium", "安装引导 (install_hint)" }
                                                }
                                                input {
                                                    class: "input input-bordered hud-input w-full text-sm",
                                                    value: "{config_form.read().install_hint}",
                                                    oninput: move |e| config_form.write().install_hint = e.value(),
                                                    placeholder: "npm install -g agent-desktop（macOS 需授予辅助功能 / 屏幕录制权限）",
                                                }
                                            }
                                            div {
                                                label { class: "label",
                                                    span { class: "label-text font-medium", "超时毫秒 (timeout_ms)" }
                                                    span { class: "label-text-alt text-base-content/60", "外层进程" }
                                                }
                                                input {
                                                    class: "input input-bordered hud-input w-full text-sm",
                                                    value: "{config_form.read().timeout_ms}",
                                                    oninput: move |e| config_form.write().timeout_ms = e.value(),
                                                    placeholder: "60000",
                                                }
                                            }
                                            div {
                                                label { class: "label",
                                                    span { class: "label-text font-medium", "输出上限字节 (max_output_bytes)" }
                                                }
                                                input {
                                                    class: "input input-bordered hud-input w-full text-sm",
                                                    value: "{config_form.read().max_output_bytes}",
                                                    oninput: move |e| config_form.write().max_output_bytes = e.value(),
                                                    placeholder: "262144",
                                                }
                                            }
                                        }
                                        div { class: "border-t border-base-300/60 pt-4",
                                            label { class: "label",
                                                span { class: "label-text font-medium", "禁用的子命令 (disabled_commands)" }
                                                span { class: "label-text-alt text-base-content/60", "留空 = 开放全部" }
                                            }
                                            input {
                                                class: "input input-bordered hud-input w-full font-mono text-sm",
                                                value: "{config_form.read().disabled_commands}",
                                                oninput: move |e| config_form.write().disabled_commands = e.value(),
                                                placeholder: "留空不屏蔽；如需拉黑填 sandbox, ...",
                                            }
                                            p { class: "text-xs opacity-60 mt-1",
                                                "默认开放 agent-desktop 的全部命令。只有明确有风险的命令才填进这个逗号分隔的黑名单，CLI 升级新增命令也默认可用。用 desktop 工具而非 shell 的价值在于自动 session 隔离与截图产物归档——不是靠禁命令。"
                                            }
                                        }
                                        HudCallout {
                                            tone: Some("info".to_string()),
                                            extra_class: Some("py-2 text-xs leading-relaxed".to_string()),
                                            p { class: "font-medium mb-0.5", "边界说明" }
                                            p { class: "mb-0.5",
                                                "agent-desktop 走 OS 无障碍树（macOS AX / Windows UIA / Linux AT-SPI）读界面，默认 headless 不抢光标/焦点；需物理输入时由 Agent 自行加 --headed。截图需 macOS「屏幕录制」权限。"
                                            }
                                            p { class: "opacity-70 mb-0",
                                                "与 browser 工具同属 agent-browser.dev 家族、在工具页归入同一分组。命令引导用 skills get desktop / skills get platform 从 CLI 自带文档实时获取。"
                                            }
                                        }
                                    }
                                } else if layout == BuiltinConfigForm::SearchTool {
                                    // tavily_search / doubao_search：超时单字段
                                    // （字段名对齐 common::config::SearchToolConfig）
                                    div {
                                        label { class: "label",
                                            span { class: "label-text font-medium", "超时毫秒 (timeout_ms)" }
                                        }
                                        input {
                                            class: "input input-bordered hud-input w-full text-sm",
                                            value: "{config_form.read().timeout_ms}",
                                            oninput: move |e| config_form.write().timeout_ms = e.value(),
                                            placeholder: if t.id == "tavily_search" { "15000" } else { "20000" },
                                        }
                                    }
                                } else if layout == BuiltinConfigForm::ShellExec {
                                    div { class: "grid grid-cols-1 gap-4",
                                        div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                                            div {
                                                label { class: "label",
                                                    span { class: "label-text font-medium", "默认超时毫秒 (default_timeout_ms)" }
                                                }
                                                input {
                                                    class: "input input-bordered hud-input w-full text-sm",
                                                    value: "{config_form.read().default_timeout_ms}",
                                                    oninput: move |e| config_form.write().default_timeout_ms = e.value(),
                                                    placeholder: "300000",
                                                }
                                            }
                                            div {
                                                label { class: "label",
                                                    span { class: "label-text font-medium", "默认输出上限 (default_max_output_size_bytes)" }
                                                }
                                                input {
                                                    class: "input input-bordered hud-input w-full text-sm",
                                                    value: "{config_form.read().default_max_output_size_bytes}",
                                                    oninput: move |e| config_form.write().default_max_output_size_bytes = e.value(),
                                                    placeholder: "10485760",
                                                }
                                            }
                                        }
                                        div {
                                            label { class: "label",
                                                span { class: "label-text font-medium", "受信任目录 (additional_allowed_paths)" }
                                            }
                                            textarea {
                                                class: "textarea textarea-bordered hud-input w-full font-mono text-sm h-24",
                                                value: "{config_form.read().additional_allowed_paths}",
                                                oninput: move |e| config_form.write().additional_allowed_paths = e.value(),
                                                placeholder: "/Users/aman/Projects\n/Users/aman/Technology",
                                            }
                                            p { class: "text-xs opacity-60 mt-1",
                                                "一行一个绝对路径。落入这些目录的命令不再触发「目录越界 / 身份边界」两类确认，可降低授权频率；不影响命令级确认（如 git push / rm -rf）。全部清空 = 不设白名单。"
                                            }
                                        }
                                        div {
                                            label { class: "label",
                                                span { class: "label-text font-medium", "PATH 补全目录 (path_additions)" }
                                            }
                                            textarea {
                                                class: "textarea textarea-bordered hud-input w-full font-mono text-sm h-24",
                                                value: "{config_form.read().path_additions}",
                                                oninput: move |e| config_form.write().path_additions = e.value(),
                                                placeholder: "/opt/homebrew/bin\n~/.cargo/bin\n~/.nvm/versions/node/*/bin",
                                            }
                                            p { class: "text-xs opacity-60 mt-1",
                                                "一行一个目录，支持 ~ 前缀与单段 * 通配；仅追加「存在且未包含」的目录到 PATH 尾部。全部清空 = 回退后端内置默认。"
                                            }
                                        }
                                        div {
                                            label { class: "label",
                                                span { class: "label-text font-medium", "工具链指回真实 HOME (toolchain_envs)" }
                                            }
                                            textarea {
                                                class: "textarea textarea-bordered hud-input w-full font-mono text-sm h-20",
                                                value: "{config_form.read().toolchain_envs}",
                                                oninput: move |e| config_form.write().toolchain_envs = e.value(),
                                                placeholder: "cargo\nnvm\npyenv",
                                            }
                                            p { class: "text-xs opacity-60 mt-1",
                                                "仅隔离 HOME 下生效：把列出的工具链根目录经官方变量（CARGO_HOME / NVM_DIR 等）指回真实 HOME，与 git 身份隔离兼得；对应路径不存在则跳过。支持：cargo / rustup / nvm / pyenv / rbenv / go / npm"
                                            }
                                        }
                                        div {
                                            label { class: "label",
                                                span { class: "label-text font-medium", "HOME 策略 (home_mode)" }
                                            }
                                            select {
                                                class: "select select-bordered hud-input w-full text-sm",
                                                value: "{config_form.read().home_mode}",
                                                onchange: move |e| config_form.write().home_mode = e.value(),
                                                option { value: "", selected: config_form.read().home_mode.is_empty(), "留空（后端默认 isolated）" }
                                                option { value: "isolated", selected: config_form.read().home_mode == "isolated", "isolated — 用户隔离 HOME（git/gh 身份确定）" }
                                                option { value: "inherit", selected: config_form.read().home_mode == "inherit", "inherit — 继承服务进程 HOME（nvm/cargo/ssh 可用）" }
                                            }
                                            // 说明随选中值联动：帮用户在「身份确定」与「工具链可用」之间做取舍
                                            HudCallout {
                                                tone: Some("info".to_string()),
                                                extra_class: Some("mt-2 py-2 text-xs leading-relaxed".to_string()),
                                                {match config_form.read().home_mode.as_str() {
                                                    "inherit" => "inherit：子进程直接使用服务进程的 HOME，nvm / pyenv / cargo / ssh 等可读取真实 HOME 下的配置与缓存；代价是 git 会读到服务进程 OS 用户的 .gitconfig，commit author 可能不是预期身份。适合单机自用 / 受信环境。",
                                                    "isolated" => "isolated：HOME 指向本平台用户的隔离目录，git / gh 使用确定的平台身份、凭据互不串扰；代价是 nvm / pyenv / cargo / ssh 读不到真实 HOME 的配置（命令本体仍可经 PATH 补全找到）。适合多用户部署。",
                                                    _ => "留空 = 使用后端默认 isolated。若命令经 PATH 补全仍失败（如 nvm use、cargo login 这类依赖 HOME 配置的命令），可切到 inherit 试验。",
                                                }}
                                            }
                                        }
                                    }
                                } else if layout == BuiltinConfigForm::Fs {
                                    // fs_read / fs_write：受信任目录单字段
                                    // （字段名对齐 common::config::FsToolConfig）
                                    div {
                                        label { class: "label",
                                            span { class: "label-text font-medium", "受信任目录 (additional_allowed_paths)" }
                                        }
                                        textarea {
                                            class: "textarea textarea-bordered hud-input w-full font-mono text-sm h-24",
                                            value: "{config_form.read().additional_allowed_paths}",
                                            oninput: move |e| config_form.write().additional_allowed_paths = e.value(),
                                            placeholder: "/Users/aman/Projects\n/Users/aman/Technology",
                                        }
                                        p { class: "text-xs opacity-60 mt-1",
                                            "一行一个绝对路径（锚定到项目根 / base data path）。列出的目录在默认工作区之外仍可读写，且不再触发「目录越界 / 身份边界」确认。全部清空 = 仅限默认工作区。"
                                        }
                                    }
                                } else {
                                    // gh_cli / lark_cli：command + 默认超时 / 默认输出上限
                                    // （字段名对齐 common::config::CliToolConfig）
                                    div { class: "grid grid-cols-1 md:grid-cols-3 gap-4",
                                        div {
                                            label { class: "label",
                                                span { class: "label-text font-medium", "命令 (command)" }
                                            }
                                            input {
                                                class: "input input-bordered hud-input w-full font-mono text-sm",
                                                value: "{config_form.read().command}",
                                                oninput: move |e| config_form.write().command = e.value(),
                                                placeholder: if t.id == "gh_cli" { "gh" } else { "lark-cli" },
                                            }
                                        }
                                        div {
                                            label { class: "label",
                                                span { class: "label-text font-medium", "默认超时毫秒 (default_timeout_ms)" }
                                            }
                                            input {
                                                class: "input input-bordered hud-input w-full text-sm",
                                                value: "{config_form.read().default_timeout_ms}",
                                                oninput: move |e| config_form.write().default_timeout_ms = e.value(),
                                                placeholder: "60000",
                                            }
                                        }
                                        div {
                                            label { class: "label",
                                                span { class: "label-text font-medium", "默认输出上限 (default_max_output_size_bytes)" }
                                            }
                                            input {
                                                class: "input input-bordered hud-input w-full text-sm",
                                                value: "{config_form.read().default_max_output_size_bytes}",
                                                oninput: move |e| config_form.write().default_max_output_size_bytes = e.value(),
                                                placeholder: "1048576",
                                            }
                                        }
                                    }
                                }
                                p { class: "text-xs opacity-60 mt-2",
                                    "数字字段留空表示不修改；仅覆盖表单展示字段，config 其余字段原样保留"
                                }
                                div { class: "flex items-center gap-3 mt-3",
                                    button {
                                        class: "btn hud-btn btn-primary btn-sm",
                                        disabled: config_saving(),
                                        onclick: {
                                            let id = t.id.clone();
                                            move |_| {
                                                let id = id.clone();
                                                // 以 detail config 为基底合并（防丢字段），校验失败 toast 不提交
                                                let base = tool_res.read().as_ref().and_then(|r| r.as_ref().ok()).and_then(|tool| tool.config.clone());
                                                let form = config_form.read().clone();
                                                let config = match merge_builtin_config(base.as_ref(), &form, layout) {
                                                    Ok(config) => config,
                                                    Err(e) => {
                                                        toast.error(&e);
                                                        return;
                                                    }
                                                };
                                                config_saving.set(true);
                                                spawn(async move {
                                                    // Builtin 仅 config 可改：其余字段不传（后端工厂字段 guard）
                                                    match update_tool(UpdateToolRequest {
                                                        id: id.clone(),
                                                        config: Some(config),
                                                        ..Default::default()
                                                    })
                                                    .await
                                                    {
                                                        Ok(_) => {
                                                            toast.success("工具配置已保存");
                                                            // 刷新详情（含统计）并回填表单基底
                                                            match get_tool(build_tool_stats_request(id, stats_range())).await {
                                                                Ok(tool) => {
                                                                    config_form.set(builtin_form_from_config(tool.config.as_ref()));
                                                                    tool_res.set(Some(Ok(tool)));
                                                                }
                                                                Err(e) => toast.error(&e),
                                                            }
                                                        }
                                                        Err(e) => toast.error(&e),
                                                    }
                                                    config_saving.set(false);
                                                });
                                            }
                                        },
                                        if config_saving() {
                                            Loading { size: "sm" }
                                            "保存中..."
                                        } else {
                                            "保存配置"
                                        }
                                    }
                                }
                            } else if matches!(t.protocol, ToolProtocol::Http | ToolProtocol::Shell) {
                                // —— 用户自建工具（HTTP / Shell）：复用创建页协议子表单 ——
                                // 同一组件 + 同一 common 结构体构造点，详情页不再另写一套字段。
                                div { class: "grid grid-cols-1 gap-4",
                                    if t.protocol == ToolProtocol::Http {
                                        HttpToolSubForm { form: http_form }
                                    } else {
                                        ShellToolSubForm { form: shell_form }
                                    }
                                }
                                p { class: "text-xs opacity-60 mt-2",
                                    "字段集与创建页一致；保存时按 common 的 HttpToolConfig / ShellToolConfig 整体覆盖 config（含凭据需求）。MCP 工具配置在其 Server 管理页维护，此处只读。"
                                }
                                div { class: "flex items-center gap-3 mt-3",
                                    button {
                                        class: "btn hud-btn btn-primary btn-sm",
                                        disabled: custom_config_saving(),
                                        onclick: {
                                            let id = t.id.clone();
                                            let is_http = t.protocol == ToolProtocol::Http;
                                            move |_| {
                                                let id = id.clone();
                                                // 构造 common SSOT 结构体 → 序列化（与创建页同一函数，
                                                // 校验同源：URL/方法/占位符/凭据需求都在此拦截）
                                                let config = if is_http {
                                                    http_config_from_form(&http_form.read()).and_then(|c| {
                                                        serde_json::to_value(c)
                                                            .map_err(|e| format!("HTTP 配置序列化失败: {}", e))
                                                    })
                                                } else {
                                                    shell_config_from_form(&shell_form.read()).and_then(|c| {
                                                        serde_json::to_value(c)
                                                            .map_err(|e| format!("Shell 配置序列化失败: {}", e))
                                                    })
                                                };
                                                let config = match config {
                                                    Ok(config) => config,
                                                    Err(e) => {
                                                        toast.error(&e);
                                                        return;
                                                    }
                                                };
                                                let requirements = if is_http {
                                                    http_form.read().credential_requirements.clone()
                                                } else {
                                                    shell_form.read().credential_requirements.clone()
                                                };
                                                custom_config_saving.set(true);
                                                spawn(async move {
                                                    match update_tool(UpdateToolRequest {
                                                        id: id.clone(),
                                                        config: Some(config),
                                                        credential_requirements: Some(requirements),
                                                        ..Default::default()
                                                    }).await {
                                                        Ok(_) => {
                                                            toast.success("工具配置已保存");
                                                            match get_tool(build_tool_stats_request(id, stats_range())).await {
                                                                Ok(tool) => {
                                                                    // 与上方 use_effect 同源：凭据声明以顶层字段为准
                                                                    let mut http = http_form_from_config(tool.config.as_ref());
                                                                    let mut shell = shell_form_from_config(tool.config.as_ref());
                                                                    let requirements = tool.credential_requirements.clone();
                                                                    http.credential_requirements = requirements.clone();
                                                                    shell.credential_requirements = requirements;
                                                                    http_form.set(http);
                                                                    shell_form.set(shell);
                                                                    tool_res.set(Some(Ok(tool)));
                                                                }
                                                                Err(e) => toast.error(&e),
                                                            }
                                                        }
                                                        Err(e) => toast.error(&e),
                                                    }
                                                    custom_config_saving.set(false);
                                                });
                                            }
                                        },
                                        if custom_config_saving() {
                                            Loading { size: "sm" }
                                            "保存中..."
                                        } else {
                                            "保存配置"
                                        }
                                    }
                                }
                            } else {
                                // —— MCP / 未匹配内置工具：只读 JSON（MCP server 配置在 MCP 管理页维护）——
                                if let Some(ref config) = t.config {
                                    pre { class: "bg-base-200 rounded-lg p-3 text-xs overflow-x-auto max-h-96 font-mono",
                                        {serde_json::to_string_pretty(config).unwrap_or_default()}
                                    }
                                }
                            }
                        }
                    }
                }

                // ===== 凭据需求卡片 =====
                // 工具自身无声明时，MCP 工具改展示继承来源标识（声明属主在 Server 行），
                // 避免凭据区域恒空白；不复制声明内容、不改变声明属主。
                // Builtin 即使零声明也必须给编辑入口（shell_exec 现状为空声明）；
                // 其余协议保持「有声明或有继承才渲染」的既有形态（零变化）。
                if injection_panel_visible(
                    t.protocol,
                    !t.credential_requirements.is_empty(),
                    t.credential_source.is_some(),
                ) {
                    HudPanel {
                        title: if t.protocol == ToolProtocol::Builtin {
                            "注入策略".to_string()
                        } else {
                            "凭据需求".to_string()
                        },
                        eyebrow: if t.protocol == ToolProtocol::Builtin {
                            "INJECTION".to_string()
                        } else {
                            "CREDENTIALS".to_string()
                        },
                        div { class: "card-body",
                            if let Some(ref source) = t.credential_source {
                                p { class: "text-sm opacity-60",
                                    "此工具自身不声明凭据需求，凭据继承自其 MCP Server；声明在 Server 管理页维护，运行时按调用者身份解析后注入。"
                                }
                                div { class: "flex flex-wrap items-center gap-2 mt-2",
                                    span { class: "text-xs opacity-60", "来源 Server：" }
                                    Link {
                                        class: "btn hud-btn btn-ghost btn-sm",
                                        to: crate::pages::Route::FinanceMcpServerDetail { id: source.id.clone() },
                                        "{source.name}"
                                    }
                                    code { class: "text-xs bg-base-200 rounded px-1 font-mono", "{source.id}" }
                                }
                            } else if t.protocol == ToolProtocol::Builtin {
                                // —— 内置工具：注入策略两 Tab 编辑器 ——
                                // config 优先 / 工厂兜底（P2 分派），保存只写声明不写凭据原文；
                                // config 其余字段由上方「保存配置」经 merge_builtin_config 维护，互不覆盖。
                                div { class: "space-y-2",
                                    p { class: "text-sm opacity-60",
                                        if config_layer_declared(t.config.as_ref()) {
                                            "注入需求由该工具的 config 声明（已固化）；config 属运维所有权，工厂 sync 不会覆盖。"
                                        } else {
                                            "当前注入需求来自工厂静态声明（默认值）。保存后写入工具 config，此后工厂声明变更不再自动生效。"
                                        }
                                    }
                                    if !config_layer_declared(t.config.as_ref()) && !t.credential_requirements.is_empty() {
                                        HudCallout { tone: Some("warning".to_string()), extra_class: Some("py-2 text-sm".to_string()),
                                            div {
                                                p { "上表是工厂默认值，不是配置值。" }
                                                p { class: "text-xs opacity-80",
                                                    "点「保存注入策略」即把当前列表写入工具 config（此后以 config 为准）；不改动则运行时行为零变化。"
                                                }
                                            }
                                        }
                                    }
                                    InjectionStrategyEditor { requirements: requirements_draft }
                                    div { class: "flex items-center gap-3 mt-2",
                                        button {
                                            class: "btn hud-btn btn-primary btn-sm",
                                            disabled: requirements_saving(),
                                            onclick: {
                                                let id = t.id.clone();
                                                move |_| {
                                                    let id = id.clone();
                                                    let draft = requirements_draft.read().clone();
                                                    let prepared = match prepare_for_save(
                                                        draft,
                                                        CredentialRequirementScope::Builtin,
                                                    ) {
                                                        Ok(list) => list,
                                                        Err(e) => {
                                                            // 与后端同一套规则（common 单点），错误就地提示不提交
                                                            toast.error(&e);
                                                            return;
                                                        }
                                                    };
                                                    requirements_saving.set(true);
                                                    spawn(async move {
                                                        match update_tool(UpdateToolRequest {
                                                            id: id.clone(),
                                                            credential_requirements: Some(prepared),
                                                            ..Default::default()
                                                        })
                                                        .await
                                                        {
                                                            Ok(_) => {
                                                                toast.success("注入策略已保存");
                                                                match get_tool(build_tool_stats_request(id, stats_range())).await {
                                                                    Ok(tool) => {
                                                                        // 回读覆盖草稿：验收 A6「保存 → 刷新回读一致」
                                                                        requirements_draft.set(tool.credential_requirements.clone());
                                                                        tool_res.set(Some(Ok(tool)));
                                                                    }
                                                                    Err(e) => toast.error(&e),
                                                                }
                                                            }
                                                            Err(e) => toast.error(&e),
                                                        }
                                                        requirements_saving.set(false);
                                                    });
                                                }
                                            },
                                            if requirements_saving() {
                                                Loading { size: "sm" }
                                                "保存中..."
                                            } else {
                                                "保存注入策略"
                                            }
                                        }
                                        span { class: "text-xs opacity-60",
                                            "只写声明（类型 / 字段 / 注入点），不写任何凭据原文；清空后回落到工厂默认"
                                        }
                                    }
                                }
                            } else {
                                p { class: "text-sm opacity-60", "凭据需求由工具配置声明，只读展示" }
                                CredentialRequirementsTable { requirements: t.credential_requirements.clone() }
                            }
                            // 运行时就绪绑定引导（与列表 badge 同源；NotReady 时提示）
                            if let RuntimeReady::NotReady { reason, hint } = runtime_ready.read().as_ref().cloned().unwrap_or_default() {
                                HudCallout { tone: Some("warning".to_string()), extra_class: Some("mt-3 py-2 text-sm".to_string()),
                                    div {
                                        p {
                                            if reason == "api_key_missing" {
                                                "当前用户凭据未绑定，工具将无法执行"
                                            } else {
                                                "工具运行环境未就绪，工具将无法执行"
                                            }
                                        }
                                        p { class: "text-xs opacity-80", "原因：{reason}｜提示：{hint}" }
                                    }
                                }
                            }
                        }
                    }
                }

                if t.stats.is_some() {
                    // 时间筛选：切换即重拉（use_resource 订阅 stats_range）
                    div { class: "mb-3",
                        TimeRangePicker { value: stats_range(), on_change: on_stats_range }
                    }
                    ToolStatsPanel { stats: t.stats.clone() }
                }

                // 工具调试调用面板
                HudPanel {
                    title: "调试调用".to_string(),
                    eyebrow: "DEBUG".to_string(),
                    div { class: "card-body",

                        // 参数 Schema 展示（如果有）
                        if let Some(ref schema) = t.parameters_schema {
                            div { class: "mb-3",
                                label { class: "label",
                                    span { class: "label-text font-medium text-sm opacity-60", "参数 Schema（自动生成骨架）" }
                                }
                                pre { class: "bg-base-200 rounded-lg p-3 text-xs overflow-x-auto max-h-48 font-mono",
                                    {serde_json::to_string_pretty(schema).unwrap_or_default()}
                                }
                            }
                        }

                        // JSON 参数编辑器
                        label { class: "label",
                            span { class: "label-text font-medium", "调用参数 (JSON)" }
                        }
                        textarea {
                            class: "textarea textarea-bordered hud-input w-full font-mono text-sm h-40",
                            value: "{debug_args()}",
                            oninput: move |e| debug_args.set(e.value()),
                            placeholder: "输入 JSON 参数",
                        }

                        // 调用按钮
                        div { class: "flex items-center gap-3 mt-3",
                            button {
                                class: "btn hud-btn btn-primary",
                                disabled: debug_calling() || !t.enabled,
                                onclick: {
                                    let id = t.id.clone();
                                    move |_| {
                                        let id = id.clone();
                                        let args_text = debug_args();
                                        debug_calling.set(true);
                                        debug_result.set(None);
                                        spawn(async move {
                                            // 解析 JSON 参数
                                            let parsed = match serde_json::from_str::<serde_json::Value>(&args_text) {
                                                Ok(v) => v,
                                                Err(e) => {
                                                    toast.error(format!("JSON 解析失败: {}", e));
                                                    debug_calling.set(false);
                                                    return;
                                                }
                                            };
                                            match debug_call_tool(DebugCallToolRequest { id: id.clone(), args: parsed }).await {
                                                Ok(resp) => {
                                                    let formatted = serde_json::to_string_pretty(&resp.result)
                                                        .unwrap_or_else(|_| resp.result.to_string());
                                                    debug_result.set(Some(format!(
                                                        "✅ 调用成功\ncall_id: {}\n\n结果:\n{}",
                                                        resp.tool_call_id, formatted
                                                    )));
                                                    toast.success("调用成功");
                                                }
                                                Err(e) => {
                                                    debug_result.set(Some(format!("❌ 调用失败:\n{}", e)));
                                                    toast.error(&e);
                                                }
                                            }
                                            debug_calling.set(false);
                                        });
                                    }
                                },
                                if debug_calling() {
                                    Loading { size: "sm" }
                                    "调用中..."
                                } else {
                                    "调用"
                                }
                            }
                            if !t.enabled {
                                span { class: "text-sm opacity-60", "工具未启用，无法调用" }
                            }
                        }

                        // 调用结果展示
                        if let Some(result) = debug_result() {
                            div { class: "mt-4",
                                label { class: "label",
                                    span { class: "label-text font-medium", "调用结果" }
                                }
                                pre { class: "bg-base-200 rounded-lg p-3 text-xs overflow-x-auto max-h-96 font-mono whitespace-pre-wrap break-all",
                                    {result}
                                }
                            }
                        }
                    }
                }
                    }
                }
                Some(Err(e)) => rsx! {
                    EmptyState { icon: "❓".to_string(), message: format!("加载失败: {}", e) }
                },
            }

            ConfirmDialog {
                show: show_delete_confirm(),
                title: "确认删除".to_string(),
                message: "确定删除此工具？此操作不可撤销。".to_string(),
                on_confirm: move |_| {
                    let id = pending_delete_id();
                    show_delete_confirm.set(false);
                    spawn(async move {
                        if let Err(e) = delete_tool(&id).await {
                            toast.error(format!("删除失败: {}", e));
                        } else {
                            toast.success("已删除");
                            let _ = navigator.push("/finance/tools".to_string());
                        }
                    });
                },
                on_cancel: move |_| {
                    show_delete_confirm.set(false);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_env_catalog_prefers_probe_and_falls_back_to_static() {
        let fallback = browser_env_catalog();
        assert!(fallback.len() >= 18);
        assert!(
            fallback
                .iter()
                .all(|entry| !entry.from_cli && entry.recommended.is_empty())
        );

        let probe = common::api::BrowserEnvProbeDTO {
            cli_version: Some("0.43.0".to_string()),
            entries: vec![common::api::BrowserEnvEntryDTO {
                key: "AGENT_BROWSER_EXECUTABLE_PATH".to_string(),
                label: "浏览器可执行路径".to_string(),
                desc: "动态说明".to_string(),
                hint: "动态提示".to_string(),
                choices: vec![],
                recommended: vec![common::api::BrowserEnvOptionDTO {
                    value: "/bin/edge".to_string(),
                    label: "Edge".to_string(),
                }],
                advanced: false,
                from_cli: true,
                configurable: true,
            }],
        };
        let dynamic = browser_env_catalog_from_probe(&probe);
        assert_eq!(dynamic.len(), 1);
        assert_eq!(dynamic[0].desc, "动态说明");
        assert_eq!(dynamic[0].recommended[0].1, "Edge");
        assert!(dynamic[0].from_cli);
        assert!(matches!(
            browser_env_kind(&dynamic, "AGENT_BROWSER_EXECUTABLE_PATH"),
            BrowserEnvFormKind::Text
        ));

        let with_choice = common::api::BrowserEnvProbeDTO {
            entries: vec![common::api::BrowserEnvEntryDTO {
                key: "AGENT_BROWSER_COLOR_SCHEME".to_string(),
                choices: vec![common::api::BrowserEnvOptionDTO {
                    value: "dark".to_string(),
                    label: "深色".to_string(),
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(matches!(
            browser_env_kind(
                &browser_env_catalog_from_probe(&with_choice),
                "AGENT_BROWSER_COLOR_SCHEME"
            ),
            BrowserEnvFormKind::Choice(_)
        ));
    }

    #[test]
    fn browser_env_keeps_risk_rows_drops_only_empty_key() {
        // 风险项（HEADED / STREAM_PORT 等 CLI 原生能力）照常可配，不能丢弃——
        // 硬禁只会逼人走 shell 绕过。只有空键行（用户开了没填）才跳过。
        let mut map = serde_json::Map::new();
        insert_browser_env(
            &mut map,
            &[
                (
                    "AGENT_BROWSER_EXECUTABLE_PATH".to_string(),
                    "/bin/edge".to_string(),
                ),
                ("AGENT_BROWSER_HEADED".to_string(), "1".to_string()),
                ("AGENT_BROWSER_STREAM_PORT".to_string(), "9223".to_string()),
                ("  ".to_string(), "x".to_string()),
            ],
        )
        .unwrap();
        let env = map.get("env").unwrap().clone();
        let object = env.as_object().unwrap();
        assert_eq!(object.len(), 3, "空键丢弃，其余前缀内项全保留");
        assert_eq!(
            object.get("AGENT_BROWSER_HEADED").and_then(|v| v.as_str()),
            Some("1")
        );
        assert_eq!(
            object
                .get("AGENT_BROWSER_STREAM_PORT")
                .and_then(|v| v.as_str()),
            Some("9223")
        );
        // 风险项写入后整体 config 仍合法（前缀约束满足）
        let config = serde_json::json!({ "env": env });
        assert!(common::models::validate_builtin_tool_config(&config).is_ok());
    }

    #[test]
    fn merge_builtin_config_never_writes_back_redacted_requirements() {
        // 详情 config 的 credential_requirements 是 redact 占位串；
        // 「保存配置」必须剔除该键（真实声明走顶层字段单独提交）。
        let base = serde_json::json!({
            "command": "gh",
            "credential_requirements": "[REDACTED]",
            "unknown_field": 1
        });
        let form = BuiltinConfigFormState {
            command: "gh".to_string(),
            ..Default::default()
        };
        let merged = merge_builtin_config(Some(&base), &form, BuiltinConfigForm::GhCli)
            .expect("合法 config 应通过校验");
        assert!(
            merged.get("credential_requirements").is_none(),
            "redact 占位串不得写回 config"
        );
        assert_eq!(merged["command"], "gh");
        assert_eq!(merged["unknown_field"], 1, "未展示字段必须原样保留");
    }

    #[test]
    fn merge_desktop_config_writes_only_its_own_fields() {
        // desktop 配置只写 5 个自有字段，不写入 browser 专属的 env / URL / file 字段
        let form = BuiltinConfigFormState {
            command: "agent-desktop".to_string(),
            timeout_ms: "30000".to_string(),
            max_output_bytes: "65536".to_string(),
            install_hint: "npm i -g agent-desktop".to_string(),
            disabled_commands: "sandbox".to_string(),
            ..Default::default()
        };
        let merged = merge_builtin_config(None, &form, BuiltinConfigForm::Desktop)
            .expect("合法 config 应通过校验");
        assert_eq!(merged["command"], "agent-desktop");
        assert_eq!(merged["timeout_ms"], 30_000);
        assert_eq!(merged["max_output_bytes"], 65_536);
        assert_eq!(merged["disabled_commands"], serde_json::json!(["sandbox"]));
        for browser_only in [
            "env",
            "allow_local_network",
            "allow_file_url",
            "allowed_file_dirs",
        ] {
            assert!(
                merged.get(browser_only).is_none(),
                "desktop config 不应写入 browser 专属字段 {browser_only}"
            );
        }
    }

    #[test]
    fn builtin_config_form_matches_factory_ids_not_display_names() {
        // 按工具 id 匹配（非显示名）：详情页据此渲染结构化配置表单，
        // 调用方必须传 t.id（"gh_cli" 等），不能用 t.name（"GitHub CLI" 等）。
        assert_eq!(
            builtin_config_form("browser"),
            Some(BuiltinConfigForm::Browser)
        );
        assert_eq!(
            builtin_config_form("desktop"),
            Some(BuiltinConfigForm::Desktop)
        );
        assert_eq!(
            builtin_config_form("gh_cli"),
            Some(BuiltinConfigForm::GhCli)
        );
        assert_eq!(
            builtin_config_form("lark_cli"),
            Some(BuiltinConfigForm::LarkCli)
        );
        // tavily_search / doubao_search 共用同一表单（超时单字段，SearchToolConfig）
        assert_eq!(
            builtin_config_form("tavily_search"),
            Some(BuiltinConfigForm::SearchTool)
        );
        assert_eq!(
            builtin_config_form("doubao_search"),
            Some(BuiltinConfigForm::SearchTool)
        );
        assert_eq!(
            builtin_config_form("shell_exec"),
            Some(BuiltinConfigForm::ShellExec)
        );
        // fs_read / fs_write 共用同一表单（受信任目录单字段，FsToolConfig）
        assert_eq!(builtin_config_form("fs_read"), Some(BuiltinConfigForm::Fs));
        assert_eq!(builtin_config_form("fs_write"), Some(BuiltinConfigForm::Fs));
        // 显示名（入库 name 字段）不匹配 —— 这是历史 bug 的根因，必须恒为 None
        assert_eq!(builtin_config_form("GitHub CLI"), None);
        assert_eq!(builtin_config_form("Browser Automation"), None);
        assert_eq!(builtin_config_form("Feishu/Lark CLI"), None);
        assert_eq!(builtin_config_form("Search Web (Tavily)"), None);
        assert_eq!(builtin_config_form("Search Chinese Web (Doubao)"), None);
        assert_eq!(builtin_config_form("Execute Shell Command"), None);
        assert_eq!(builtin_config_form("Read File from Workspace"), None);
        assert_eq!(builtin_config_form("Write File to Workspace"), None);
        // MCP / Http / 未知内置工具名 → None（回退只读 JSON）
        assert_eq!(builtin_config_form("mcp_tool"), None);
        assert_eq!(builtin_config_form("http_tool"), None);
    }

    #[test]
    fn form_from_config_reads_shell_exec_fields() {
        let config = serde_json::json!({
            "default_timeout_ms": 120000,
            "default_max_output_size_bytes": 5242880,
            "path_additions": ["/opt/homebrew/bin", "~/.cargo/bin"],
            "additional_allowed_paths": ["/Users/aman/Projects", "/Users/aman/Tech"],
            "toolchain_envs": ["cargo", "nvm"],
            "home_mode": "inherit"
        });
        let form = builtin_form_from_config(Some(&config));
        assert_eq!(form.default_timeout_ms, "120000");
        assert_eq!(form.default_max_output_size_bytes, "5242880");
        assert_eq!(form.path_additions, "/opt/homebrew/bin\n~/.cargo/bin");
        assert_eq!(
            form.additional_allowed_paths,
            "/Users/aman/Projects\n/Users/aman/Tech"
        );
        assert_eq!(form.toolchain_envs, "cargo\nnvm");
        assert_eq!(form.home_mode, "inherit");
    }

    #[test]
    fn merge_shell_exec_roundtrip_and_reset() {
        // 回写：多行文本 → JSON 数组 + home_mode 校验
        let form = BuiltinConfigFormState {
            default_timeout_ms: "120000".to_string(),
            default_max_output_size_bytes: "5242880".to_string(),
            path_additions: " /opt/homebrew/bin \n\n~/.cargo/bin\n".to_string(),
            additional_allowed_paths: "/p1\n\n/p2\n".to_string(),
            toolchain_envs: "Cargo\n\nnvm\nCARGO\n".to_string(),
            home_mode: "inherit".to_string(),
            ..Default::default()
        };
        let merged = merge_builtin_config(None, &form, BuiltinConfigForm::ShellExec).unwrap();
        assert_eq!(merged["default_timeout_ms"], 120000);
        assert_eq!(merged["default_max_output_size_bytes"], 5242880);
        assert_eq!(
            merged["path_additions"],
            serde_json::json!(["/opt/homebrew/bin", "~/.cargo/bin"])
        );
        assert_eq!(
            merged["additional_allowed_paths"],
            serde_json::json!(["/p1", "/p2"])
        );
        // 小写归一 + 去重
        assert_eq!(
            merged["toolchain_envs"],
            serde_json::json!(["cargo", "nvm"])
        );
        assert_eq!(merged["home_mode"], "inherit");

        // 全部清空 → 删除键（回退后端内置默认），不影响基底其他字段
        let base = serde_json::json!({
            "default_timeout_ms": 111,
            "default_max_output_size_bytes": 222,
            "path_additions": ["/x"],
            "additional_allowed_paths": ["/keep"],
            "toolchain_envs": ["cargo"],
            "keep": true
        });
        let form = BuiltinConfigFormState::default();
        let merged =
            merge_builtin_config(Some(&base), &form, BuiltinConfigForm::ShellExec).unwrap();
        // 数字字段留空 = 不修改（保留基底值）——与 browser / tavily 表单语义一致
        assert_eq!(merged["default_timeout_ms"], 111);
        assert_eq!(merged["default_max_output_size_bytes"], 222);
        assert!(merged.get("path_additions").is_none());
        assert!(merged.get("additional_allowed_paths").is_none());
        assert!(merged.get("toolchain_envs").is_none());
        assert_eq!(merged["keep"], true);

        // 非法 home_mode / 未知名工具链本地拦截
        let form = BuiltinConfigFormState {
            home_mode: "none".to_string(),
            ..Default::default()
        };
        let err = merge_builtin_config(None, &form, BuiltinConfigForm::ShellExec).unwrap_err();
        assert!(err.contains("home_mode"));

        let form = BuiltinConfigFormState {
            toolchain_envs: "conda".to_string(),
            ..Default::default()
        };
        let err = merge_builtin_config(None, &form, BuiltinConfigForm::ShellExec).unwrap_err();
        assert!(err.contains("toolchain_envs") && err.contains("cargo"));
    }

    #[test]
    fn form_from_config_reads_known_fields_and_tolerates_missing() {
        let config = serde_json::json!({
            "command": "/usr/local/bin/gh",
            "timeout_ms": 30000,
            "unknown": "未展示字段"
        });
        let form = builtin_form_from_config(Some(&config));
        assert_eq!(form.command, "/usr/local/bin/gh");
        assert_eq!(form.timeout_ms, "30000");
        assert_eq!(form.max_output_bytes, "");
        // config 缺失 / Null（存量 DB 零迁移兼容）→ 全空表单；
        // 两个布尔开关空串在下拉里由 `!= "false"` 选中「允许」，提交时留空不写入 = 走后端默认 true
        assert_eq!(
            builtin_form_from_config(None),
            BuiltinConfigFormState::default()
        );
        assert_eq!(
            builtin_form_from_config(Some(&serde_json::Value::Null)),
            BuiltinConfigFormState::default()
        );
    }

    #[test]
    fn merge_preserves_fields_not_shown_in_form() {
        let base = serde_json::json!({
            "command": "gh",
            "timeout_ms": 60000,
            "extra_field": "保留我"
        });
        let form = BuiltinConfigFormState {
            command: "  /opt/homebrew/bin/gh  ".to_string(),
            timeout_ms: String::new(), // 留空 → 不覆盖
            ..Default::default()
        };
        let merged = merge_builtin_config(Some(&base), &form, BuiltinConfigForm::GhCli).unwrap();
        assert_eq!(merged["command"], "/opt/homebrew/bin/gh"); // trim 后写入
        assert_eq!(merged["timeout_ms"], 60000); // 留空保留基底
        assert_eq!(merged["extra_field"], "保留我"); // 未展示字段保留
    }

    #[test]
    fn merge_browser_overwrites_all_shown_fields() {
        let base = serde_json::json!({ "command": "agent-browser", "timeout_ms": 60000 });
        let form = BuiltinConfigFormState {
            command: "agent-browser2".to_string(),
            timeout_ms: "120000".to_string(),
            max_output_bytes: "524288".to_string(),
            install_hint: "brew install agent-browser".to_string(),
            ..Default::default()
        };
        let merged = merge_builtin_config(Some(&base), &form, BuiltinConfigForm::Browser).unwrap();
        assert_eq!(merged["command"], "agent-browser2");
        assert_eq!(merged["timeout_ms"], 120000);
        assert_eq!(merged["max_output_bytes"], 524288);
        assert_eq!(merged["install_hint"], "brew install agent-browser");
    }

    // 搜索结果/表单产物必须能被 common 的共享结构体解析（键名漂移即在此暴露）
    #[test]
    fn merge_search_tool_only_touches_timeout() {
        let base = serde_json::json!({ "timeout_ms": 15000, "keep": 1 });
        let form = BuiltinConfigFormState {
            timeout_ms: "30000".to_string(),
            command: "不应写入".to_string(), // 搜索工具表单无 command 字段
            ..Default::default()
        };
        let merged =
            merge_builtin_config(Some(&base), &form, BuiltinConfigForm::SearchTool).unwrap();
        assert_eq!(merged["timeout_ms"], 30000);
        assert_eq!(merged["keep"], 1);
        assert!(merged.get("command").is_none());
        // SSOT 保险：产物必须能被 common::config::SearchToolConfig 解析
        let parsed: common::config::SearchToolConfig =
            serde_json::from_value(merged.clone()).unwrap();
        assert_eq!(parsed.timeout_ms, Some(30000));
    }

    #[test]
    fn merge_rejects_empty_command_and_invalid_numbers() {
        // command 空白
        let form = BuiltinConfigFormState {
            command: "   ".to_string(),
            ..Default::default()
        };
        let err = merge_builtin_config(None, &form, BuiltinConfigForm::GhCli).unwrap_err();
        assert!(err.contains("command"));

        // timeout_ms 非数字
        let form = BuiltinConfigFormState {
            timeout_ms: "abc".to_string(),
            ..Default::default()
        };
        let err = merge_builtin_config(None, &form, BuiltinConfigForm::SearchTool).unwrap_err();
        assert!(err.contains("timeout_ms"));

        // timeout_ms 非正整数（0）
        let form = BuiltinConfigFormState {
            timeout_ms: "0".to_string(),
            ..Default::default()
        };
        assert!(
            merge_builtin_config(None, &form, BuiltinConfigForm::SearchTool)
                .unwrap_err()
                .contains("timeout_ms")
        );
    }

    #[test]
    fn merge_from_null_base_builds_fresh_object() {
        // 存量 DB builtin config = Null（D28 零迁移兼容）：空基底 + 表单值 → 新对象
        let form = BuiltinConfigFormState {
            command: "gh".to_string(),
            ..Default::default()
        };
        let merged = merge_builtin_config(None, &form, BuiltinConfigForm::GhCli).unwrap();
        assert_eq!(merged["command"], "gh");
    }

    #[test]
    fn merge_gh_cli_writes_cli_default_fields_matching_common_struct() {
        // 键名必须与 common::config::CliToolConfig 字段一致（前后端共享 SSOT）
        let form = BuiltinConfigFormState {
            command: "gh".to_string(),
            default_timeout_ms: "45000".to_string(),
            default_max_output_size_bytes: "2097152".to_string(),
            ..Default::default()
        };
        let merged = merge_builtin_config(None, &form, BuiltinConfigForm::GhCli).unwrap();
        // 反序列化回 common SSOT 结构体：字段命名漂移即编译期/断言期暴露
        let parsed: common::config::CliToolConfig = serde_json::from_value(merged).unwrap();
        assert_eq!(parsed.command.as_deref(), Some("gh"));
        assert_eq!(parsed.default_timeout_ms, Some(45_000));
        assert_eq!(parsed.default_max_output_size_bytes, Some(2_097_152));
    }

    #[test]
    fn merged_configs_deserialize_into_common_ssot_structs() {
        // 前端构造的 config JSON 必须能被 common 的共享结构体解析（键名一致性保险）
        let browser = merge_builtin_config(
            None,
            &BuiltinConfigFormState {
                command: "agent-browser".to_string(),
                timeout_ms: "60000".to_string(),
                max_output_bytes: "262144".to_string(),
                install_hint: "brew install agent-browser".to_string(),
                browser_env_rows: vec![
                    (
                        "AGENT_BROWSER_EXECUTABLE_PATH".to_string(),
                        "/usr/bin/chromium".to_string(),
                    ),
                    ("AGENT_BROWSER_COLOR_SCHEME".to_string(), "dark".to_string()),
                ],
                allow_local_network: "true".to_string(),
                disabled_commands: "eval, cookies".to_string(),
                allow_file_url: "true".to_string(),
                allowed_file_dirs: "/data/html, /tmp/preview".to_string(),
                ..Default::default()
            },
            BuiltinConfigForm::Browser,
        )
        .unwrap();
        let parsed_browser: common::config::BrowserConfig =
            serde_json::from_value(browser).unwrap();
        assert_eq!(parsed_browser.command.as_deref(), Some("agent-browser"));
        assert_eq!(parsed_browser.max_output_bytes, Some(262_144));
        assert_eq!(parsed_browser.allow_local_network, Some(true));
        assert_eq!(
            parsed_browser.disabled_commands.as_deref(),
            Some(&["eval".to_string(), "cookies".to_string()][..])
        );
        assert_eq!(parsed_browser.allow_file_url, Some(true));
        assert_eq!(
            parsed_browser.allowed_file_dirs.as_deref(),
            Some(&["/data/html".to_string(), "/tmp/preview".to_string()][..])
        );
        let env = parsed_browser.env.expect("browser env 应写入");
        assert_eq!(
            env.get("AGENT_BROWSER_EXECUTABLE_PATH").map(String::as_str),
            Some("/usr/bin/chromium")
        );
        assert_eq!(
            env.get("AGENT_BROWSER_COLOR_SCHEME").map(String::as_str),
            Some("dark")
        );

        let shell = merge_builtin_config(
            None,
            &BuiltinConfigFormState {
                additional_allowed_paths: "/Users/aman/Projects".to_string(),
                path_additions: "/opt/homebrew/bin".to_string(),
                ..Default::default()
            },
            BuiltinConfigForm::ShellExec,
        )
        .unwrap();
        let parsed_shell: common::config::ShellExecConfig = serde_json::from_value(shell).unwrap();
        assert_eq!(
            parsed_shell.additional_allowed_paths.as_deref(),
            Some(&["/Users/aman/Projects".to_string()][..])
        );

        // fs_read / fs_write 共用 FsToolConfig：受信任目录键名必须一致
        let fs = merge_builtin_config(
            None,
            &BuiltinConfigFormState {
                additional_allowed_paths: "/Users/aman/Projects\n/srv/data".to_string(),
                ..Default::default()
            },
            BuiltinConfigForm::Fs,
        )
        .unwrap();
        let parsed_fs: common::config::FsToolConfig = serde_json::from_value(fs).unwrap();
        assert_eq!(
            parsed_fs.additional_allowed_paths.as_deref(),
            Some(&["/Users/aman/Projects".to_string(), "/srv/data".to_string()][..])
        );
    }

    #[test]
    fn merge_fs_only_touches_additional_allowed_paths() {
        // Fs 表单只覆盖受信任目录；基底其它字段原样保留（防丢字段）
        let base = serde_json::json!({ "additional_allowed_paths": ["/old"], "keep": true });
        let form = BuiltinConfigFormState {
            additional_allowed_paths: "/new\n/second".to_string(),
            ..Default::default()
        };
        let merged = merge_builtin_config(Some(&base), &form, BuiltinConfigForm::Fs).unwrap();
        assert_eq!(
            merged["additional_allowed_paths"],
            serde_json::json!(["/new", "/second"])
        );
        assert_eq!(merged["keep"], true);

        // 清空 → 删除该键（回退默认：仅限工作区）
        let form = BuiltinConfigFormState::default();
        let merged = merge_builtin_config(Some(&base), &form, BuiltinConfigForm::Fs).unwrap();
        assert!(merged.get("additional_allowed_paths").is_none());
        assert_eq!(merged["keep"], true);
    }
}
