//! Tool 详情页

use crate::api::finance::{
    debug_call_tool, delete_tool, get_tool, query_tools, update_tool, update_tool_status,
};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::credential_requirements::CredentialRequirementsTable;
use crate::components::markdown::MarkdownRenderer;
use crate::components::state::{EmptyState, Loading};
use crate::components::stats::ToolStatsPanel;
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::{
    DebugCallToolRequest, GetToolRequest, GetToolResponse, RuntimeReady, ToolQueryRequest,
    UpdateToolRequest, UpdateToolStatusRequest,
};
use common::enums::{ToolProtocol, ToolStatus};
use dioxus::prelude::*;
use dioxus_router::{Link, use_navigator};

/// 构造带统计信息的 GetToolRequest
fn build_tool_stats_request(id: String) -> GetToolRequest {
    GetToolRequest {
        id,
        with_stats: Some(true),
        ..Default::default()
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
    /// gh_cli：command 单字段
    GhCli,
    /// lark_cli：command 单字段
    LarkCli,
    /// tavily_search：timeout_ms 单字段
    TavilySearch,
}

/// 按工具名匹配内置工具结构化表单；不匹配（MCP / Http / 未知 Builtin）→ None 回退只读 JSON
fn builtin_config_form(name: &str) -> Option<BuiltinConfigForm> {
    match name {
        "browser" => Some(BuiltinConfigForm::Browser),
        "gh_cli" => Some(BuiltinConfigForm::GhCli),
        "lark_cli" => Some(BuiltinConfigForm::LarkCli),
        "tavily_search" => Some(BuiltinConfigForm::TavilySearch),
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
}

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
    BuiltinConfigFormState {
        command: text("command"),
        timeout_ms: number("timeout_ms"),
        max_output_bytes: number("max_output_bytes"),
        install_hint: text("install_hint"),
    }
}

/// 以 detail config 为基底合并表单字段（防丢字段：未在表单展示的字段原样保留）
///
/// 校验与后端 `validate_builtin_config` 对齐：command 非空 string；
/// timeout_ms / max_output_bytes 正整数（留空 = 不覆盖，保留基底值或后端缺省兜底）。
fn merge_builtin_config(
    base: Option<&serde_json::Value>,
    form: &BuiltinConfigFormState,
    layout: BuiltinConfigForm,
) -> Result<serde_json::Value, String> {
    let mut map = base
        .and_then(|c| c.as_object())
        .cloned()
        .unwrap_or_default();
    match layout {
        BuiltinConfigForm::Browser => {
            insert_command(&mut map, &form.command)?;
            insert_positive_number(&mut map, "timeout_ms", &form.timeout_ms)?;
            insert_positive_number(&mut map, "max_output_bytes", &form.max_output_bytes)?;
            map.insert(
                "install_hint".to_string(),
                serde_json::Value::String(form.install_hint.trim().to_string()),
            );
        }
        BuiltinConfigForm::GhCli | BuiltinConfigForm::LarkCli => {
            insert_command(&mut map, &form.command)?;
        }
        BuiltinConfigForm::TavilySearch => {
            insert_positive_number(&mut map, "timeout_ms", &form.timeout_ms)?;
        }
    }
    Ok(serde_json::Value::Object(map))
}

/// command 覆盖（非空校验与后端 validate_builtin_config 对齐）
fn insert_command(
    map: &mut serde_json::Map<String, serde_json::Value>,
    command: &str,
) -> Result<(), String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err("command 不能为空".to_string());
    }
    map.insert(
        "command".to_string(),
        serde_json::Value::String(trimmed.to_string()),
    );
    Ok(())
}

/// 数字字段覆盖：留空不覆盖（保留基底 / 缺省兜底）；非空须为正整数
fn insert_positive_number(
    map: &mut serde_json::Map<String, serde_json::Value>,
    field: &str,
    text: &str,
) -> Result<(), String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let number = trimmed
        .parse::<u64>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| format!("{field} 必须是正整数"))?;
    map.insert(field.to_string(), serde_json::json!(number));
    Ok(())
}

#[component]
pub fn FinanceToolDetail(id: String) -> Element {
    let mut tool_data = use_signal(|| None::<GetToolResponse>);
    let mut loading = use_signal(|| true);
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
    let mut config_saving = use_signal(|| false);
    // 运行时就绪（详情响应无此字段，经 query_tools 与列表 badge 同源探测）
    let mut runtime_ready = use_signal(RuntimeReady::default);

    use_effect(move || {
        loading.set(true);
        let id = id.clone();
        let readiness_id = id.clone();
        spawn(async move {
            match get_tool(build_tool_stats_request(id)).await {
                Ok(tool) => {
                    // 从 parameters_schema 生成参数骨架
                    if let Some(ref schema) = tool.parameters_schema {
                        debug_args.set(generate_skeleton_from_schema(schema));
                    }
                    // 初始化配置编辑表单（以 detail config 为基底）
                    config_form.set(builtin_form_from_config(tool.config.as_ref()));
                    tool_data.set(Some(tool));
                }
                Err(e) => toast.error(&e),
            }
            // 运行时就绪探测（best-effort；与列表 badge 同源：query_tools → probe_runtime_ready）
            if let Ok(page) = query_tools(&ToolQueryRequest {
                ids: Some(vec![readiness_id]),
                ..Default::default()
            })
            .await
                && let Some(item) = page.items.into_iter().next()
            {
                runtime_ready.set(item.runtime_ready);
            }
            loading.set(false);
        });
    });

    rsx! {
        AppLayout {
            div { class: "mb-6 flex items-center justify-between",
                div {
                    h1 { class: "text-2xl font-bold", "工具详情" }
                }
                Link { class: "btn btn-ghost", to: crate::pages::Route::FinanceTools {},
                    "← 返回列表"
                }
            }

            if loading() {
                Loading {}
            } else if let Some(t) = tool_data.read().clone() {
                div { class: "card bg-base-100 shadow-md",
                    div { class: "card-body",
                        div { class: "flex justify-between items-center mb-4",
                            h2 { class: "card-title", "{t.name}" }
                            div { class: "flex gap-2",
                                if t.enabled {
                                    button { class: "btn btn-outline btn-sm",
                                        onclick: {
                                            let id = t.id.clone();
                                            move |_| {
                                                let id = id.clone();
                                                spawn(async move {
                                                    if let Err(e) = update_tool_status(UpdateToolStatusRequest { id: id.clone(), status: ToolStatus::Disabled }).await {
                                                        toast.error(&e);
                                                    } else {
                                                        toast.success("已禁用");
                                                        if let Ok(tool) = get_tool(build_tool_stats_request(id)).await {
                                                            tool_data.set(Some(tool));
                                                        }
                                                    }
                                                });
                                            }
                                        },
                                        "禁用"
                                    }
                                } else {
                                    button { class: "btn btn-primary btn-sm",
                                        onclick: {
                                            let id = t.id.clone();
                                            move |_| {
                                                let id = id.clone();
                                                spawn(async move {
                                                    if let Err(e) = update_tool_status(UpdateToolStatusRequest { id: id.clone(), status: ToolStatus::Enabled }).await {
                                                        toast.error(&e);
                                                    } else {
                                                        toast.success("已启用");
                                                        if let Ok(tool) = get_tool(build_tool_stats_request(id)).await {
                                                            tool_data.set(Some(tool));
                                                        }
                                                    }
                                                });
                                            }
                                        },
                                        "启用"
                                    }
                                }
                                button { class: "btn btn-error btn-sm",
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
                                div { span { class: "badge badge-neutral", "{t.protocol}" } }
                            }
                            div {
                                label { class: "label",
                                    span { class: "label-text font-medium", "状态" }
                                }
                                div {
                                    if t.enabled {
                                        span { class: "badge badge-success", "启用" }
                                    } else {
                                        span { class: "badge badge-error", "禁用" }
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
                                            span { class: "badge badge-neutral", "{tag}" }
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

                // ===== 工具配置卡片（Builtin 按工厂名结构化编辑；MCP/Http/未匹配只读 JSON）=====
                if builtin_config_form(&t.name).is_some() || t.has_config {
                    div { class: "card bg-base-100 shadow-md mt-4",
                        div { class: "card-body",
                            h3 { class: "card-title text-lg", "工具配置" }
                            if let Some(layout) = builtin_config_form(&t.name) {
                                // —— Builtin 结构化表单（D28：CLI 命令与行为参数）——
                                if layout == BuiltinConfigForm::Browser {
                                    div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                                        div {
                                            label { class: "label",
                                                span { class: "label-text font-medium", "命令 (command)" }
                                            }
                                            input {
                                                class: "input input-bordered w-full font-mono text-sm",
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
                                                class: "input input-bordered w-full text-sm",
                                                value: "{config_form.read().install_hint}",
                                                oninput: move |e| config_form.write().install_hint = e.value(),
                                                placeholder: "brew install agent-browser 或 cargo install agent-browser",
                                            }
                                        }
                                        div {
                                            label { class: "label",
                                                span { class: "label-text font-medium", "超时毫秒 (timeout_ms)" }
                                            }
                                            input {
                                                class: "input input-bordered w-full text-sm",
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
                                                class: "input input-bordered w-full text-sm",
                                                value: "{config_form.read().max_output_bytes}",
                                                oninput: move |e| config_form.write().max_output_bytes = e.value(),
                                                placeholder: "262144",
                                            }
                                        }
                                    }
                                } else if layout == BuiltinConfigForm::TavilySearch {
                                    div {
                                        label { class: "label",
                                            span { class: "label-text font-medium", "超时毫秒 (timeout_ms)" }
                                        }
                                        input {
                                            class: "input input-bordered w-full text-sm",
                                            value: "{config_form.read().timeout_ms}",
                                            oninput: move |e| config_form.write().timeout_ms = e.value(),
                                            placeholder: "15000",
                                        }
                                    }
                                } else {
                                    // gh_cli / lark_cli：command 单字段
                                    div {
                                        label { class: "label",
                                            span { class: "label-text font-medium", "命令 (command)" }
                                        }
                                        input {
                                            class: "input input-bordered w-full font-mono text-sm",
                                            value: "{config_form.read().command}",
                                            oninput: move |e| config_form.write().command = e.value(),
                                            placeholder: if t.name == "gh_cli" { "gh" } else { "lark-cli" },
                                        }
                                    }
                                }
                                p { class: "text-xs opacity-60 mt-2",
                                    "数字字段留空表示不修改；仅覆盖表单展示字段，config 其余字段原样保留"
                                }
                                div { class: "flex items-center gap-3 mt-3",
                                    button {
                                        class: "btn btn-primary btn-sm",
                                        disabled: config_saving(),
                                        onclick: {
                                            let id = t.id.clone();
                                            move |_| {
                                                let id = id.clone();
                                                // 以 detail config 为基底合并（防丢字段），校验失败 toast 不提交
                                                let base = tool_data.read().as_ref().and_then(|tool| tool.config.clone());
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
                                                            match get_tool(build_tool_stats_request(id)).await {
                                                                Ok(tool) => {
                                                                    config_form.set(builtin_form_from_config(tool.config.as_ref()));
                                                                    tool_data.set(Some(tool));
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
                                            span { class: "loading loading-spinner loading-sm" }
                                            "保存中..."
                                        } else {
                                            "保存配置"
                                        }
                                    }
                                }
                            } else {
                                // —— MCP / Http / 未匹配内置工具：只读 JSON（MCP server 配置在 MCP 管理页维护）——
                                if let Some(ref config) = t.config {
                                    pre { class: "bg-base-200 rounded-lg p-3 text-xs overflow-x-auto max-h-96 font-mono",
                                        {serde_json::to_string_pretty(config).unwrap_or_default()}
                                    }
                                }
                            }
                        }
                    }
                }

                // ===== 凭据需求只读卡片（空数组不渲染）=====
                if !t.credential_requirements.is_empty() {
                    div { class: "card bg-base-100 shadow-md mt-4",
                        div { class: "card-body",
                            h3 { class: "card-title text-lg", "凭据需求" }
                            p { class: "text-sm opacity-60",
                                if t.protocol == ToolProtocol::Builtin {
                                    "内置工具凭据需求由工厂声明，只读展示"
                                } else {
                                    "凭据需求由工具配置声明，只读展示"
                                }
                            }
                            CredentialRequirementsTable { requirements: t.credential_requirements.clone() }
                            // 运行时就绪绑定引导（与列表 badge 同源；NotReady 时提示）
                            if let RuntimeReady::NotReady { reason, hint } = runtime_ready() {
                                div { class: "alert alert-warning mt-3 py-2 text-sm",
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
                    ToolStatsPanel { stats: t.stats.clone() }
                }

                // 工具调试调用面板
                div { class: "card bg-base-100 shadow-md mt-4",
                    div { class: "card-body",
                        h3 { class: "card-title text-lg", "调试调用" }

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
                            class: "textarea textarea-bordered w-full font-mono text-sm h-40",
                            value: "{debug_args()}",
                            oninput: move |e| debug_args.set(e.value()),
                            placeholder: "输入 JSON 参数",
                        }

                        // 调用按钮
                        div { class: "flex items-center gap-3 mt-3",
                            button {
                                class: "btn btn-primary",
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
                                    span { class: "loading loading-spinner loading-sm" }
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
            } else {
                EmptyState { icon: "🔧".to_string(), message: "工具不存在或已被删除".to_string() }
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
    fn builtin_config_form_matches_factory_names() {
        assert_eq!(builtin_config_form("browser"), Some(BuiltinConfigForm::Browser));
        assert_eq!(builtin_config_form("gh_cli"), Some(BuiltinConfigForm::GhCli));
        assert_eq!(builtin_config_form("lark_cli"), Some(BuiltinConfigForm::LarkCli));
        assert_eq!(
            builtin_config_form("tavily_search"),
            Some(BuiltinConfigForm::TavilySearch)
        );
        // MCP / Http / 未知内置工具名 → None（回退只读 JSON）
        assert_eq!(builtin_config_form("mcp_tool"), None);
        assert_eq!(builtin_config_form("http_tool"), None);
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
        // config 缺失 / Null（存量 DB 零迁移兼容）→ 全空表单
        assert_eq!(builtin_form_from_config(None), BuiltinConfigFormState::default());
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
        };
        let merged =
            merge_builtin_config(Some(&base), &form, BuiltinConfigForm::Browser).unwrap();
        assert_eq!(merged["command"], "agent-browser2");
        assert_eq!(merged["timeout_ms"], 120000);
        assert_eq!(merged["max_output_bytes"], 524288);
        assert_eq!(merged["install_hint"], "brew install agent-browser");
    }

    #[test]
    fn merge_tavily_only_touches_timeout() {
        let base = serde_json::json!({ "timeout_ms": 15000, "keep": 1 });
        let form = BuiltinConfigFormState {
            timeout_ms: "30000".to_string(),
            command: "不应写入".to_string(), // tavily 表单无 command 字段
            ..Default::default()
        };
        let merged =
            merge_builtin_config(Some(&base), &form, BuiltinConfigForm::TavilySearch).unwrap();
        assert_eq!(merged["timeout_ms"], 30000);
        assert_eq!(merged["keep"], 1);
        assert!(merged.get("command").is_none());
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
        let err =
            merge_builtin_config(None, &form, BuiltinConfigForm::TavilySearch).unwrap_err();
        assert!(err.contains("timeout_ms"));

        // timeout_ms 非正整数（0）
        let form = BuiltinConfigFormState {
            timeout_ms: "0".to_string(),
            ..Default::default()
        };
        assert!(
            merge_builtin_config(None, &form, BuiltinConfigForm::TavilySearch)
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
}
