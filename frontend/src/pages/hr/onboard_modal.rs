//! 入职弹窗：选择本次要安装的组织级工具包 / 技能包
//!
//! 设计要点（同 CreateAgentModal）：
//! - 由父组件条件渲染：打开时挂载、关闭即卸载，选中态随之自动重置。
//! - props 只放稳定数据（agent_id + 组织默认包 + on_close 回调），
//!   候选包自行加载，避免父组件重渲染打断输入。
//!
//! 语义：**真正的入职流程在这条边内完成** —— 勾选的包随入职请求一起提交，
//! 后端在 `PendingOnboard → Onboarded` 这条边上安装，而不是入职之后再补装。
//! 不勾选任何包也不会阻断入职（组织未要求时合法）。

use dioxus::prelude::*;

use crate::api::finance::list_tools;
use crate::api::hr::{onboard_agent, query_skills};
use crate::api::organization::get_current_organization;
use crate::components::modal::Modal;
use crate::store::toast::use_toast;
use common::api::{AgentPackSelection, ListToolsRequest, OnboardAgentRequest, SkillQueryRequest};
use common::enums::SkillStatus;

#[derive(Props, Clone, PartialEq)]
pub struct OnboardModalProps {
    pub agent_id: String,
    /// 关闭并刷新：由父组件用 `use_callback` 提供
    pub on_close: Callback<()>,
}

#[component]
pub fn OnboardModal(props: OnboardModalProps) -> Element {
    let toast = use_toast();
    let mut selected_tool_packs = use_signal(Vec::<String>::new);
    let mut selected_skill_packs = use_signal(Vec::<String>::new);
    let mut tool_candidates = use_signal(Vec::<String>::new);
    let mut skill_candidates = use_signal(Vec::<String>::new);
    let mut custom_tag = use_signal(String::new);
    let mut submitting = use_signal(|| false);

    // 预勾选组织级默认包（OrganizationConfig.agent_onboard）
    use_effect(move || {
        spawn(async move {
            if let Ok(resp) = get_current_organization().await {
                let onboard = resp.data.config.agent_onboard;
                selected_tool_packs.set(onboard.required_tool_packs);
                selected_skill_packs.set(onboard.required_skill_packs);
            }
        });
    });

    // 候选包：工具侧取所有工具的 tags，技能侧取已发布技能的 tags（去重排序）
    use_effect(move || {
        spawn(async move {
            if let Ok(page) = list_tools(ListToolsRequest::default()).await {
                let mut tags: Vec<String> =
                    page.items.iter().flat_map(|t| t.tags.clone()).collect();
                tags.sort();
                tags.dedup();
                tool_candidates.set(tags);
            }
            let req = SkillQueryRequest {
                status: Some(SkillStatus::Published),
                ..Default::default()
            };
            if let Ok(page) = query_skills(&req).await {
                let mut tags: Vec<String> =
                    page.items.iter().flat_map(|s| s.tags.clone()).collect();
                tags.sort();
                tags.dedup();
                skill_candidates.set(tags);
            }
        });
    });

    let on_close = props.on_close;
    let agent_id = props.agent_id.clone();

    let handle_submit = move |_| {
        let agent_id = agent_id.clone();
        spawn(async move {
            submitting.set(true);
            let req = OnboardAgentRequest {
                id: agent_id,
                packs: Some(AgentPackSelection {
                    tool_packs: selected_tool_packs(),
                    skill_packs: selected_skill_packs(),
                }),
            };
            match onboard_agent(req).await {
                Ok(_) => {
                    toast.success("Agent 已正式入职");
                    on_close.call(());
                }
                Err(e) => toast.error(format!("入职失败: {}", e)),
            }
            submitting.set(false);
        });
    };

    rsx! {
        Modal {
            title: "入职：选择组织要求的工具包与技能包".to_string(),
            show: true,
            on_close: move |_| on_close.call(()),
            width_class: Some("max-w-3xl".to_string()),
            footer: rsx! {
                button { class: "btn hud-btn btn-ghost", onclick: move |_| on_close.call(()), "取消" }
                button {
                    class: "btn hud-btn btn-primary",
                    disabled: submitting(),
                    onclick: handle_submit,
                    if submitting() { "入职中..." } else { "确认入职" }
                }
            },
            div { class: "space-y-5",
                p { class: "text-sm text-base-content/60",
                    "勾选的包会随本次入职一并安装（工具包写入授权，技能包建立技能副本）。"
                    "同名包建议两侧都勾 —— 例如 project_management 既是工具授权 tag，也是技能包 tag。"
                }

                div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                    PackPicker {
                        title: "工具包",
                        hint: "控制该 Agent 能调用哪些工具",
                        candidates: tool_candidates(),
                        selected: selected_tool_packs(),
                        on_toggle: move |tag: String| {
                            let mut cur = selected_tool_packs();
                            if let Some(pos) = cur.iter().position(|t| *t == tag) {
                                cur.remove(pos);
                            } else {
                                cur.push(tag);
                            }
                            selected_tool_packs.set(cur);
                        },
                    }
                    PackPicker {
                        title: "技能包",
                        hint: "控制该 Agent 有哪些方法论可用",
                        candidates: skill_candidates(),
                        selected: selected_skill_packs(),
                        on_toggle: move |tag: String| {
                            let mut cur = selected_skill_packs();
                            if let Some(pos) = cur.iter().position(|t| *t == tag) {
                                cur.remove(pos);
                            } else {
                                cur.push(tag);
                            }
                            selected_skill_packs.set(cur);
                        },
                    }
                }

                // 自定义 tag：候选来自库里已有资源，组织新增的自定义包可在此补录
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text", "补充自定义包 tag" }
                    }
                    div { class: "flex gap-2",
                        input {
                            class: "input hud-input flex-1",
                            placeholder: "输入 tag 后回车，同时加入工具包与技能包",
                            value: "{custom_tag}",
                            oninput: move |e| custom_tag.set(e.value()),
                            onkeydown: move |e| {
                                if e.key() == Key::Enter {
                                    e.prevent_default();
                                    let tag = custom_tag().trim().to_string();
                                    if tag.is_empty() {
                                        return;
                                    }
                                    let mut tools = selected_tool_packs();
                                    if !tools.contains(&tag) {
                                        tools.push(tag.clone());
                                        selected_tool_packs.set(tools);
                                    }
                                    let mut skills = selected_skill_packs();
                                    if !skills.contains(&tag) {
                                        skills.push(tag);
                                        selected_skill_packs.set(skills);
                                    }
                                    custom_tag.set(String::new());
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}

/// 单个包选择区（候选 checkbox 列表 + 已选摘要）
#[component]
fn PackPicker(
    title: String,
    hint: String,
    candidates: Vec<String>,
    selected: Vec<String>,
    on_toggle: Callback<String>,
) -> Element {
    rsx! {
        div { class: "border border-base-300 rounded-lg p-3",
            div { class: "font-medium mb-1", "{title}" }
            div { class: "text-xs text-base-content/50 mb-3", "{hint}" }
            div { class: "max-h-64 overflow-y-auto space-y-1",
                if candidates.is_empty() {
                    p { class: "text-sm text-base-content/50", "暂无候选包" }
                }
                for tag in candidates {
                    {
                        let checked = selected.contains(&tag);
                        let tag_for_toggle = tag.clone();
                        rsx! {
                            label { class: "flex items-center gap-2 py-1 cursor-pointer",
                                input {
                                    r#type: "checkbox",
                                    class: "checkbox checkbox-sm",
                                    checked: checked,
                                    onclick: move |_| on_toggle.call(tag_for_toggle.clone()),
                                }
                                span { class: "text-sm font-mono", "{tag}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
