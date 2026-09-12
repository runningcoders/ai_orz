//! 绑定对话模型弹窗（独立组件）
//!
//! 入口：Agent 列表页「模型/执行器」列——当 local 类型 Agent 未绑定模型时，
//! 该列显示「请绑定模型」按钮，点击后弹出本组件，让用户就地选择并绑定，
//! 无需进入详情页。
//!
//! 设计要点与 `CreateAgentModal` 一致：
//! - props 仅稳定的 `on_close: Callback<()>`（父用 `use_callback` 提供）+ 目标 Agent 标识，
//!   父组件列表/搜索信号变化时本组件不会被无谓重渲染；
//! - `model_providers` 自行加载，不依赖父组件传入的 Signal，进一步解耦；
//! - 只做局部更新：`update_agent` 仅传 `model_provider_id`，其余字段留 None（= 不改）。

use dioxus::prelude::*;
use dioxus_router::Link;

use crate::api::finance::list_model_providers;
use crate::api::hr::update_agent;
use crate::components::modal::Modal;
use crate::pages::Route;
use crate::store::toast::use_toast;
use common::api::{ListModelProvidersResponseItem, UpdateAgentRequest};

#[derive(Props, Clone, PartialEq)]
pub struct BindModelModalProps {
    /// 目标 Agent ID
    pub agent_id: String,
    /// 目标 Agent 名称（仅用于弹窗文案）
    pub agent_name: String,
    /// 关闭并刷新列表：由父组件用 `use_callback` 提供
    pub on_close: Callback<()>,
}

#[component]
pub fn BindModelModal(props: BindModelModalProps) -> Element {
    let toast = use_toast();
    let mut model_providers = use_signal(Vec::<ListModelProvidersResponseItem>::new);
    let mut selected = use_signal(String::new);
    let mut saving = use_signal(|| false);

    let agent_id = props.agent_id.clone();
    let agent_name = props.agent_name.clone();
    let on_close = props.on_close;

    // 模型提供商下拉数据自行加载（仅在组件挂载时拉取一次）
    use_effect(move || {
        spawn(async move {
            if let Ok(resp) = list_model_providers().await {
                model_providers.set(resp.providers);
            }
        });
    });

    let handle_bind = move |_| {
        if selected().is_empty() {
            toast.error("请先选择要绑定的对话模型");
            return;
        }
        // 局部更新：仅带 model_provider_id，其余字段 None = 保持原值
        let req = UpdateAgentRequest {
            id: agent_id.clone(),
            name: None,
            roles: None,
            description: None,
            capabilities: None,
            soul: None,
            model_provider_id: Some(selected()),
            runtime_config: None,
        };
        saving.set(true);
        spawn(async move {
            match update_agent(req).await {
                Ok(_) => {
                    toast.success("模型绑定成功");
                    // 关闭弹窗 + 刷新列表；组件卸载后各信号自动重置
                    on_close.call(());
                }
                Err(e) => toast.error(format!("绑定失败: {}", e)),
            }
            saving.set(false);
        });
    };

    let has_agent_provider = model_providers
        .read()
        .iter()
        .any(|mp| mp.capability.is_agent());

    rsx! {
        Modal {
            title: "绑定对话模型".to_string(),
            show: true,
            on_close: move |_| on_close.call(()),
            footer: rsx! {
                button { class: "btn hud-btn btn-ghost", onclick: move |_| on_close.call(()), "取消" }
                button { class: "btn hud-btn btn-primary", disabled: saving() || !has_agent_provider, onclick: handle_bind,
                    if saving() { "绑定中..." } else { "确认绑定" }
                }
            },
            div { class: "space-y-4",
                div { class: "text-sm text-base-content/70",
                    "为「"
                    span { class: "font-medium text-base-content", "{agent_name}" }
                    "」选择要绑定的对话模型。绑定后即可点击状态栏的下一步按钮继续完成入职。"
                }
                if !has_agent_provider {
                    div { class: "flex flex-col gap-1",
                        div { class: "text-sm text-warning", "暂无可用的对话模型" }
                        Link {
                            class: "link link-primary link-hover text-xs",
                            to: Route::FinanceModelProviders {},
                            "前往模型提供商管理 →"
                        }
                    }
                } else {
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "对话模型" }
                        }
                        select { class: "select select-bordered w-full", value: "{selected}",
                            onchange: move |e| selected.set(e.value()),
                            option { value: "", "-- 请选择对话模型 --" }
                            for mp in model_providers.read().iter().filter(|mp| mp.capability.is_agent()) {
                                option { value: "{mp.id}", "{mp.name} ({mp.model_name})" }
                            }
                        }
                    }
                }
            }
        }
    }
}
