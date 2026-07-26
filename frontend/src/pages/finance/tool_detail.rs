//! Tool 详情页

use crate::api::StatsOptions;
use crate::api::finance::{debug_call_tool, delete_tool, get_tool, update_tool_status};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::state::{EmptyState, Loading};
use crate::components::stats::ToolStatsPanel;
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::GetToolResponse;
use dioxus::prelude::*;
use dioxus_router::{Link, use_navigator};

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

#[component]
pub fn FinanceToolDetail(id: String) -> Element {
    let mut tool_data = use_signal(|| None::<GetToolResponse>);
    let mut loading = use_signal(|| true);
    let toast = use_toast();
    let navigator = use_navigator();

    // ===== 删除确认对话框 =====
    let mut show_delete_confirm = use_signal(|| false);
    let mut pending_delete_id = use_signal(|| String::new());

    // 调试面板状态
    let mut debug_args = use_signal(|| "{}".to_string());
    let mut debug_result = use_signal(|| None::<String>);
    let mut debug_calling = use_signal(|| false);

    use_effect(move || {
        loading.set(true);
        let id = id.clone();
        spawn(async move {
            let stats_options = StatsOptions {
                with_stats: true,
                with_model_call_stats: false,
                stats_interval: None,
            };
            match get_tool(&id, Some(&stats_options)).await {
                Ok(tool) => {
                    // 从 parameters_schema 生成参数骨架
                    if let Some(ref schema) = tool.parameters_schema {
                        debug_args.set(generate_skeleton_from_schema(schema));
                    }
                    tool_data.set(Some(tool));
                }
                Err(e) => toast.error(&e),
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
                                                    if let Err(e) = update_tool_status(&id, 0).await {
                                                        toast.error(&e);
                                                    } else {
                                                        toast.success("已禁用");
                                                        let stats_options = StatsOptions {
                                                            with_stats: true,
                                                            with_model_call_stats: false,
                                                            stats_interval: None,
                                                        };
                                                        if let Ok(tool) = get_tool(&id, Some(&stats_options)).await {
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
                                                    if let Err(e) = update_tool_status(&id, 1).await {
                                                        toast.error(&e);
                                                    } else {
                                                        toast.success("已启用");
                                                        let stats_options = StatsOptions {
                                                            with_stats: true,
                                                            with_model_call_stats: false,
                                                            stats_interval: None,
                                                        };
                                                        if let Ok(tool) = get_tool(&id, Some(&stats_options)).await {
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
                                div { "{t.description}" }
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
                                                    toast.error(&format!("JSON 解析失败: {}", e));
                                                    debug_calling.set(false);
                                                    return;
                                                }
                                            };
                                            match debug_call_tool(&id, &parsed).await {
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
                            toast.error(&format!("删除失败: {}", e));
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
