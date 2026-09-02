//! 创建本地 Agent 弹窗（独立组件）
//!
//! 设计要点：
//! - 由父组件 `show_add_modal` 条件渲染：打开时挂载、关闭（on_close）即卸载，
//!   `new_*` 信号随之自动重置，无需手动 reset，避免下次打开残留上次数据。
//! - props 仅一个稳定的 `on_close: Callback<()>`（父用 `use_callback` 提供），
//!   因此父组件列表/搜索信号变化时本组件不会被无谓重渲染，从而切断
//!   「重渲染打断中文输入法组合 / 输入框卡死」这一类问题。
//! - `model_providers` 自行加载，不依赖父组件传入的 Signal，进一步解耦。

use dioxus::prelude::*;
use dioxus_router::Link;

use crate::api::finance::list_model_providers;
use crate::api::hr::create_agent;
use crate::components::modal::Modal;
use crate::pages::Route;
use crate::store::toast::use_toast;
use common::api::{CreateAgentRequest, ListModelProvidersResponseItem};

#[derive(Props, Clone, PartialEq)]
pub struct CreateAgentModalProps {
    /// 关闭并刷新列表：由父组件用 `use_callback` 提供（关闭弹窗 + 重新加载 Agent 列表）
    pub on_close: Callback<()>,
}

#[component]
pub fn CreateAgentModal(props: CreateAgentModalProps) -> Element {
    let toast = use_toast();
    let mut new_name = use_signal(String::new);
    let mut new_roles = use_signal(Vec::<String>::new);
    let mut new_roles_input = use_signal(String::new);
    let mut new_capabilities = use_signal(Vec::<String>::new);
    let mut new_capabilities_input = use_signal(String::new);
    let mut new_soul = use_signal(String::new);
    let mut new_model_provider_id = use_signal(String::new);
    let mut new_description = use_signal(String::new);
    let mut creating = use_signal(|| false);

    // 模型提供商下拉数据自行加载（仅在组件挂载时拉取一次）
    let mut model_providers = use_signal(Vec::<ListModelProvidersResponseItem>::new);
    use_effect(move || {
        spawn(async move {
            if let Ok(resp) = list_model_providers().await {
                model_providers.set(resp.providers);
            }
        });
    });

    let on_close = props.on_close;

    let handle_create = move |_| {
        spawn(async move {
            if new_name().is_empty() {
                toast.error("名称不能为空");
                return;
            }
            creating.set(true);
            let req = CreateAgentRequest {
                name: new_name(),
                roles: if new_roles().is_empty() {
                    None
                } else {
                    Some(new_roles())
                },
                description: if new_description().is_empty() {
                    None
                } else {
                    Some(new_description())
                },
                capabilities: if new_capabilities().is_empty() {
                    None
                } else {
                    Some(new_capabilities())
                },
                soul: if new_soul().is_empty() {
                    None
                } else {
                    Some(new_soul())
                },
                model_provider_id: new_model_provider_id(),
            };
            match create_agent(req).await {
                Ok(_) => {
                    toast.success("Agent 创建成功");
                    // 关闭弹窗 + 刷新列表；组件卸载后 new_* 信号自动重置
                    on_close.call(());
                }
                Err(e) => toast.error(format!("创建失败: {}", e)),
            }
            creating.set(false);
        });
    };

    rsx! {
        Modal {
            title: "创建本地 Agent".to_string(),
            show: true,
            on_close: move |_| on_close.call(()),
            footer: rsx! {
                button { class: "btn hud-btn btn-ghost", onclick: move |_| on_close.call(()), "取消" }
                button { class: "btn hud-btn btn-primary", disabled: creating(), onclick: handle_create,
                    if creating() { "创建中..." } else { "创建" }
                }
            },
            div { class: "space-y-4",
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "Agent 名称 *" }
                    }
                    input { class: "input input-bordered w-full", value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()), placeholder: "请输入 Agent 名称" }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "角色（多选）" }
                        span { class: "label-text-alt", "用于路由匹配，如前台/Web接待/代码专家 等" }
                    }
                    // 预设角色 chip
                    div { class: "flex flex-wrap gap-2 mb-2",
                        {
                            const PRESET_ROLES: &[(&str, &str)] = &[
                                ("reception", "Web前台接待"),
                                ("feishu_reception", "飞书前台接待"),
                                ("a2a_gateway", "A2A网关"),
                                ("hr_specialist", "人事专员"),
                                ("code_assistant", "代码助手"),
                            ];
                            PRESET_ROLES.iter().map(|(key, label)| {
                                let key_clone = key.to_string();
                                let selected = new_roles().iter().any(|r| r == key);
                                let cls = if selected {
                                    "btn hud-btn btn-primary btn-sm"
                                } else {
                                    "btn hud-btn btn-outline btn-sm"
                                };
                                rsx! {
                                    button { class: cls,
                                        onclick: move |_| {
                                            let mut v = new_roles();
                                            if let Some(pos) = v.iter().position(|x| x == key_clone.as_str()) {
                                                v.remove(pos);
                                            } else {
                                                v.push(key_clone.clone());
                                            }
                                            new_roles.set(v);
                                        },
                                        "{label}"
                                    }
                                }
                            })
                        }
                    }
                    // 自定义输入（回车/失焦添加）
                    div { class: "flex flex-wrap gap-2 items-center",
                        if !new_roles().is_empty() {
                            for role in new_roles() {
                                span { class: "badge orz-tag badge-lg gap-1",
                                    "{role}",
                                    button { class: "btn hud-btn btn-ghost btn-xs",
                                        onclick: move |_| {
                                            let mut v = new_roles();
                                            if let Some(pos) = v.iter().position(|x| x == &role) {
                                                v.remove(pos);
                                            }
                                            new_roles.set(v);
                                        },
                                        "✕"
                                    }
                                }
                            }
                        }
                        input { class: "input input-bordered input-sm flex-1 min-w-[180px]",
                            value: "{new_roles_input}",
                            placeholder: "自定义角色，回车/逗号添加",
                            oninput: move |e| {
                                let val = e.value();
                                if let Some(comma_pos) = val.find(',') {
                                    let (head, rest) = val.split_at(comma_pos);
                                    let v = head.trim().to_string();
                                    if !v.is_empty() && !new_roles().iter().any(|r| r == v.as_str()) {
                                        let mut arr = new_roles();
                                        arr.push(v);
                                        new_roles.set(arr);
                                    }
                                    new_roles_input.set(rest[1..].trim().to_string());
                                } else {
                                    new_roles_input.set(val);
                                }
                            },
                            onkeydown: move |e| {
                                if e.key() == Key::Enter {
                                    e.prevent_default();
                                    let v = new_roles_input().trim().to_string();
                                    if !v.is_empty() && !new_roles().iter().any(|r| r == v.as_str()) {
                                        let mut arr = new_roles();
                                        arr.push(v);
                                        new_roles.set(arr);
                                    }
                                    new_roles_input.set(String::new());
                                }
                            }
                        }
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "能力关键词（多选，用于弱匹配）" }
                        span { class: "label-text-alt", "如：chat、code_search、task、knowledge 等" }
                    }
                    div { class: "flex flex-wrap gap-2 items-center",
                        if !new_capabilities().is_empty() {
                            for cap in new_capabilities() {
                                span { class: "badge orz-tag badge-lg gap-1",
                                    "{cap}",
                                    button { class: "btn hud-btn btn-ghost btn-xs",
                                        onclick: move |_| {
                                            let mut v = new_capabilities();
                                            if let Some(pos) = v.iter().position(|x| x == &cap) {
                                                v.remove(pos);
                                            }
                                            new_capabilities.set(v);
                                        },
                                        "✕"
                                    }
                                }
                            }
                        }
                        input { class: "input input-bordered input-sm flex-1 min-w-[180px]",
                            value: "{new_capabilities_input}",
                            placeholder: "自定义能力，回车/逗号添加",
                            oninput: move |e| {
                                let val = e.value();
                                if let Some(comma_pos) = val.find(',') {
                                    let (head, rest) = val.split_at(comma_pos);
                                    let v = head.trim().to_string();
                                    if !v.is_empty() && !new_capabilities().iter().any(|r| r == v.as_str()) {
                                        let mut arr = new_capabilities();
                                        arr.push(v);
                                        new_capabilities.set(arr);
                                    }
                                    new_capabilities_input.set(rest[1..].trim().to_string());
                                } else {
                                    new_capabilities_input.set(val);
                                }
                            },
                            onkeydown: move |e| {
                                if e.key() == Key::Enter {
                                    e.prevent_default();
                                    let v = new_capabilities_input().trim().to_string();
                                    if !v.is_empty() && !new_capabilities().iter().any(|r| r == v.as_str()) {
                                        let mut arr = new_capabilities();
                                        arr.push(v);
                                        new_capabilities.set(arr);
                                    }
                                    new_capabilities_input.set(String::new());
                                }
                            }
                        }
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "灵魂 / 系统提示词" }
                        span { class: "label-text-alt", "Agent 的深层人设 / 世界观 / 行为准则" }
                    }
                    textarea { class: "textarea textarea-bordered w-full", rows: 4,
                        value: "{new_soul}",
                        oninput: move |e| new_soul.set(e.value()),
                        placeholder: "你是一位资深的代码助手，习惯先分析需求再给出结构化建议..."
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "模型提供商" }
                        span { class: "label-text-alt", "可选：暂不选择则 Agent 处于面试中状态，配置对话模型后入职即可用" }
                    }
                    if model_providers.read().iter().filter(|mp| mp.capability.is_agent()).count() == 0 {
                        div { class: "flex flex-col gap-1",
                            input {
                                class: "input input-bordered w-full opacity-60",
                                value: "{new_model_provider_id}",
                                oninput: move |e| new_model_provider_id.set(e.value()),
                                placeholder: "暂无可用对话模型，可稍后在「模型提供商管理」中配置并绑定"
                            }
                            Link {
                                class: "link link-primary link-hover text-xs",
                                to: Route::FinanceModelProviders {},
                                "前往模型提供商管理 →"
                            }
                        }
                    } else {
                        select { class: "select select-bordered w-full", value: "{new_model_provider_id}",
                            onchange: move |e| new_model_provider_id.set(e.value()),
                            option { value: "", "-- 暂不绑定（面试中）--" }
                            for mp in model_providers.read().iter().filter(|mp| mp.capability.is_agent()) {
                                option { value: "{mp.id}", "{mp.name} ({mp.model_name})" }
                            }
                        }
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "描述" }
                    }
                    textarea { class: "textarea textarea-bordered w-full", value: "{new_description}",
                        oninput: move |e| new_description.set(e.value()), placeholder: "Agent 描述（可选，用于列表展示）" }
                }
            }
        }
    }
}
