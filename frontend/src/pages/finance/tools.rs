//! 工具管理

use crate::components::hud::PageHeader;
use crate::components::hud::{HudCallout, HudPanel};
use dioxus::prelude::*;
use std::collections::{HashMap, HashSet};

use crate::api::finance::{delete_tool, list_tools, query_tools, search_tools, update_tool_status};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::create_http_tool::CreateHttpToolModal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::{
    ListToolsRequest, ListToolsResponseItem, RuntimeReady, SearchToolsRequest, ToolListItem,
    ToolQueryRequest, UpdateToolStatusRequest,
};
use common::enums::{ToolProtocol, ToolStatus};
use dioxus_router::Link;

/// 通用内置工具（CoreTool 原生实现，非 handler）：用于组内区分 handler 工具
const GENERIC_BUILTIN_TOOL_IDS: &[&str] = &[
    "http_fetch",
    "fs_read",
    "fs_write",
    "shell_exec",
    "lark_cli",
    "gh_cli",
    "tavily_search",
    "doubao_search",
    "browser",
    "mark_artifact",
];

/// 是否为通用内置工具（非 handler 的 Builtin 工具）
fn is_generic_builtin_tool(t: &ToolListItem) -> bool {
    GENERIC_BUILTIN_TOOL_IDS.contains(&t.id.as_str())
}

/// 是否为 handler 工具（Builtin 协议且非通用内置工具）
fn is_handler_tool(t: &ToolListItem) -> bool {
    t.protocol == ToolProtocol::Builtin && !is_generic_builtin_tool(t)
}

/// 分组键：neural 工具归入「neural」组（最高优先级）；其余取首个 tag，无 tag 归为「未分类」
fn tool_group_key(t: &ToolListItem) -> String {
    if t.tags.iter().any(|s| s == "neural") {
        "neural".to_string()
    } else {
        t.tags
            .first()
            .cloned()
            .unwrap_or_else(|| "未分类".to_string())
    }
}

/// 组内排序权重：普通工具优先（0），handler 工具靠后（1）
fn tool_kind_rank(t: &ToolListItem) -> i32 {
    if is_handler_tool(t) { 1 } else { 0 }
}

/// 分组排序键：neural 组优先，其余按 tag 名升序；「未分类」最后
fn group_order_key(group: &(String, Vec<ToolListItem>)) -> (i32, u8, String) {
    let neural_rank = if group.0 == "neural" { 0 } else { 1 };
    let uncategorized = u8::from(group.0 == "未分类");
    (neural_rank, uncategorized, group.0.clone())
}

#[component]
pub fn FinanceTools() -> Element {
    let mut tools = use_signal(Vec::<ListToolsResponseItem>::new);
    let mut loading = use_signal(|| true);
    let toast = use_toast();

    // 搜索状态
    let mut search_keyword = use_signal(String::new);
    let mut search_request_id = use_signal(|| 0u32);

    // 过滤条件
    let mut filter_protocol = use_signal(|| -1i32);
    let mut filter_status = use_signal(|| -1i32);

    // ===== 删除确认对话框 =====
    let mut show_delete_confirm = use_signal(|| false);
    let mut pending_delete_id = use_signal(String::new);

    // ===== 创建 HTTP 工具弹窗 =====
    let mut show_create = use_signal(|| false);

    // ===== 分组折叠状态（tag 分组视图） =====
    let mut collapsed_groups: Signal<HashSet<String>> = use_signal(HashSet::new);

    // 加载数据（三场景切换：list / query / search）
    let load_data = move || {
        spawn(async move {
            loading.set(true);
            let keyword = search_keyword();
            let protocol = filter_protocol();
            let status = filter_status();
            let my_id = search_request_id() + 1;
            search_request_id.set(my_id);

            let has_filter = protocol >= 0 || status >= 0;

            // 三场景切换：
            // 无关键词 + 无过滤 → list_tools
            // 无关键词 + 有过滤 → query_tools
            // 有关键词 → search_tools（可同时带过滤条件）
            let result = if keyword.trim().is_empty() && !has_filter {
                list_tools(ListToolsRequest::default())
                    .await
                    .map(|p| p.items)
            } else if keyword.trim().is_empty() {
                query_tools(&ToolQueryRequest {
                    protocol: if protocol >= 0 {
                        Some(ToolProtocol::from(protocol))
                    } else {
                        None
                    },
                    status: if status >= 0 {
                        Some(ToolStatus::from(status))
                    } else {
                        None
                    },
                    ..Default::default()
                })
                .await
                .map(|p| p.items)
            } else {
                search_tools(&SearchToolsRequest {
                    keyword: Some(keyword),
                    protocol: if protocol >= 0 {
                        Some(ToolProtocol::from(protocol))
                    } else {
                        None
                    },
                    status: if status >= 0 {
                        Some(ToolStatus::from(status))
                    } else {
                        None
                    },
                    ..Default::default()
                })
                .await
                .map(|p| p.items)
            };

            // 丢弃过期请求的结果
            if search_request_id() != my_id {
                return;
            }

            match result {
                Ok(v) => tools.set(v),
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    };

    // 初始加载
    use_effect(move || {
        load_data();
    });

    let tools_list = tools.read().clone();

    // 按 tag 分组：neural 组最高优先级；同 tag 内普通工具优先于 handler 工具
    let mut group_map: HashMap<String, Vec<ToolListItem>> = HashMap::new();
    for t in tools_list.iter() {
        group_map
            .entry(tool_group_key(t))
            .or_default()
            .push(t.clone());
    }
    let mut groups: Vec<(String, Vec<ToolListItem>)> = group_map.into_iter().collect();
    for (_, group_tools) in groups.iter_mut() {
        group_tools.sort_by(|a, b| {
            tool_kind_rank(a)
                .cmp(&tool_kind_rank(b))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
    }
    groups.sort_by_key(group_order_key);
    let collapsed_set = collapsed_groups.read().clone();

    rsx! {
        AppLayout {
            PageHeader {
                eyebrow: Some("FINANCE".to_string()),
                title: "工具管理".to_string(),
                actions: Some(rsx!{
                div { class: "flex gap-2",
                    Link {
                        class: "btn hud-btn btn-ghost btn-sm",
                        to: crate::pages::Route::FinanceMcpServers {},
                        "🌐 MCP 服务器"
                    }
                    button {
                        class: "btn hud-btn btn-primary btn-sm",
                        onclick: move |_| show_create.set(true),
                        "+ 创建 HTTP 工具"
                    }
                }
                }),
            },

            // 工具创建来源引导：不同协议工具在对应页面创建
            HudCallout { tone: Some("info".to_string()), extra_class: Some("mb-4".to_string()),
                div { class: "w-full text-sm space-y-1",
                    p { class: "font-medium", "工具创建指引" }
                    div { class: "flex flex-wrap gap-x-6 gap-y-1",
                        span { "· HTTP 工具：点击右上角「+ 创建 HTTP 工具」，用于封装外部 REST API" }
                        Link {
                            class: "link link-primary",
                            to: crate::pages::Route::FinanceMcpServers {},
                            "· MCP 工具：前往 MCP 服务器页创建服务器并同步工具 →"
                        }
                        span { "· 内置工具：由系统代码注册表同步，无需手动创建" }
                    }
                }
            }

            // 筛选栏（独立卡片）
            HudPanel { signal: Some(true), extra_class: Some("mb-4".to_string()),
                div { class: "card-body",
                    div { class: "flex flex-wrap gap-4 items-end",
                        div { class: "flex flex-col gap-1 min-w-[140px] flex-1",
                            label { class: "form-label", "协议" }
                            select {
                                class: "select select-bordered w-full",
                                value: "{filter_protocol}",
                                onchange: move |e| {
                                    if let Ok(v) = e.value().parse::<i32>() {
                                        filter_protocol.set(v);
                                    }
                                    load_data();
                                },
                                option { value: "-1", "全部" }
                                option { value: "0", "内置" }
                                option { value: "1", "HTTP" }
                                option { value: "2", "MCP" }
                            }
                        }
                        div { class: "flex flex-col gap-1 min-w-[140px] flex-1",
                            label { class: "form-label", "状态" }
                            select {
                                class: "select select-bordered w-full",
                                value: "{filter_status}",
                                onchange: move |e| {
                                    if let Ok(v) = e.value().parse::<i32>() {
                                        filter_status.set(v);
                                    }
                                    load_data();
                                },
                                option { value: "-1", "全部" }
                                option { value: "1", "启用" }
                                option { value: "0", "禁用" }
                            }
                        }
                        div { class: "flex flex-col gap-1 min-w-[140px] flex-1",
                            label { class: "form-label", "搜索" }
                            input {
                                class: "input input-bordered w-full",
                                placeholder: "搜索工具...",
                                value: "{search_keyword}",
                                oninput: move |e| {
                                    search_keyword.set(e.value());
                                    let my_id = search_request_id() + 1;
                                    search_request_id.set(my_id);
                                    spawn(async move {
                                        gloo_timers::future::TimeoutFuture::new(300).await;
                                        if search_request_id() != my_id {
                                            return;
                                        }
                                        load_data();
                                    });
                                }
                            }
                        }
                    }
                }
            }

            // 列表卡片
            HudPanel { signal: Some(true),
                div { class: "card-body",
                if loading() {
                    Loading {}
                } else if tools_list.is_empty() {
                    EmptyState { icon: "🔧".to_string(), message: "暂无工具".to_string() }
                } else {
                    div { class: "overflow-x-auto",
                        table { class: "table hud-table table-zebra table-pin-rows",
                            thead { tr {
                                th { "名称" }
                                th { "协议" }
                                th { "状态" }
                                th { "就绪" }
                                th { "操作" }
                            }}
                            tbody {
                                for (group_key, group_tools) in groups.iter() {
                                    {
                                        let key = group_key.clone();
                                        let group_len = group_tools.len();
                                        let is_neural_group = key == "neural";
                                        let is_uncategorized = key == "未分类";
                                        let default_collapsed = !is_neural_group;
                                        let is_collapsed =
                                            collapsed_set.contains(&key) != default_collapsed;
                                        let arrow = if is_collapsed { "▸" } else { "▾" };
                                        let toggle_key = key.clone();
                                        rsx! {
                                            tr { key: "{key}-header", class: "bg-base-200 cursor-pointer select-none",
                                                onclick: move |_| {
                                                    let toggle_key = toggle_key.clone();
                                                    let mut set = collapsed_groups.write();
                                                    if set.contains(&toggle_key) {
                                                        set.remove(&toggle_key);
                                                    } else {
                                                        set.insert(toggle_key);
                                                    }
                                                },
                                                td { colspan: "5", class: "px-2 py-2",
                                                    span { class: "font-semibold",
                                                        "{arrow} {key}"
                                                    }
                                                    span { class: "badge orz-tag badge-sm ml-2", "{group_len}" }
                                                    if is_neural_group {
                                                        span { class: "badge orz-tag badge-sm ml-2", "神经工具" }
                                                    } else if is_uncategorized {
                                                        span { class: "badge orz-tag badge-sm ml-2", "未分类" }
                                                    }
                                                }
                                            }
                                            if !is_collapsed {
                                                for t in group_tools.iter() {
                                                    {
                                                        let id = t.id.clone();
                                                        let name = t.name.clone();
                                                        let protocol = t.protocol;
                                                        let status = t.status;
                                                        let is_enabled = status == ToolStatus::Enabled;
                                                        let runtime_ready = t.runtime_ready.clone();
                                                        // 就绪 badge 三态（advisory）：未就绪悬浮显示原因与修复提示；Unknown（无探测器）弱化展示
                                                        let (ready_class, ready_title, ready_text) = match &runtime_ready {
                                                            RuntimeReady::Ready => (
                                                                "badge hud-badge badge-success badge-outline",
                                                                "运行环境就绪（CLI 已安装 / 授权可用）".to_string(),
                                                                "就绪",
                                                            ),
                                                            RuntimeReady::NotReady { reason, hint } => (
                                                                "badge hud-badge badge-warning badge-outline",
                                                                format!("未就绪（{}）：{}", reason, hint),
                                                                "未就绪",
                                                            ),
                                                            RuntimeReady::Unknown => (
                                                                "text-base-content/40 text-xs",
                                                                String::new(),
                                                                "—",
                                                            ),
                                                        };
                                                        let id_disable = id.clone();
                                                        let id_enable = id.clone();
                                                        let id_delete = id.clone();
                                                        let id_detail = id.clone();
                                                        rsx! {
                                                            tr { key: "{id}",
                                                                td { class: "font-semibold",
                                                                    Link { to: crate::pages::Route::FinanceToolDetail { id: id_detail.clone() }, "{name}" }
                                                                }
                                                                td { span { class: "badge orz-tag badge-sm", "{protocol}" } }
                                                                td {
                                                                    if is_enabled {
                                                                        span { class: "badge hud-badge badge-success", "启用" }
                                                                    } else {
                                                                        span { class: "badge hud-badge badge-error", "禁用" }
                                                                    }
                                                                }
                                                                td {
                                                                    span {
                                                                        class: "{ready_class}",
                                                                        title: "{ready_title}",
                                                                        "{ready_text}"
                                                                    }
                                                                }
                                                                td { class: "flex gap-2 items-center",
                                                                    if is_enabled {
                                                                        button { class: "btn hud-btn btn-ghost btn-sm",
                                                                            onclick: move |_| {
                                                                                let id_disable = id_disable.clone();
                                                                                spawn(async move {
                                                                                    if let Err(e) = update_tool_status(UpdateToolStatusRequest { id: id_disable, status: ToolStatus::Disabled }).await {
                                                                                        toast.error(&e);
                                                                                    } else {
                                                                                        load_data();
                                                                                    }
                                                                                });
                                                                            },
                                                                            "禁用"
                                                                        }
                                                                    } else {
                                                                        button { class: "btn hud-btn btn-ghost btn-sm",
                                                                            onclick: move |_| {
                                                                                let id_enable = id_enable.clone();
                                                                                spawn(async move {
                                                                                    if let Err(e) = update_tool_status(UpdateToolStatusRequest { id: id_enable, status: ToolStatus::Enabled }).await {
                                                                                        toast.error(&e);
                                                                                    } else {
                                                                                        load_data();
                                                                                    }
                                                                                });
                                                                            },
                                                                            "启用"
                                                                        }
                                                                    }
                                                                    button { class: "btn hud-btn btn-error btn-sm",
                                                                        onclick: move |_| {
                                                                            pending_delete_id.set(id_delete.clone());
                                                                            show_delete_confirm.set(true);
                                                                        },
                                                                        "删除"
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
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
                        load_data();
                    }
                });
            },
            on_cancel: move |_| {
                show_delete_confirm.set(false);
            }
        }

        CreateHttpToolModal {
            show: show_create(),
            on_close: move |_| show_create.set(false),
            on_created: move |_| load_data(),
        }
        }
    }
}
