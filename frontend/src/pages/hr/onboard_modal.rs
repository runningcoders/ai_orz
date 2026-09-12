//! 入职弹窗：确认本次要安装的工具包 / 技能包
//!
//! 三组语义（工具包 / 技能包 两个 picker 内各自分区展示）：
//! - **组织要求**（`OrganizationConfig.agent_onboard`）：硬性要求，默认选中且置灰不可取消；
//! - **Agent 自带**：出生自带的基础包 + 职业选择时匹配到的包（即当前已安装的包），
//!   默认选中且置灰不可取消 —— 这些是 Agent 既有的能力，不该在入职时被取消；
//! - **可额外安装**：其余候选包，管理员按需勾选。
//!
//! 设计要点（同 CreateAgentModal）：
//! - 由父组件条件渲染：打开时挂载、关闭即卸载，选中态随之自动重置。
//! - props 只放稳定数据（agent_id + on_close），三组包自行加载，
//!   避免父组件重渲染打断输入。
//! - 自定义 tag 输入抽成独立子组件（`CustomPackInput`），勾选操作引起的父级重渲染
//!   不会重建输入框、打断中文输入法。
//!
//! 语义：**真正的入职流程在这条边内完成** —— 锁定组与勾选项随入职请求一起提交，
//! 后端在 `PendingOnboard → Onboarded` 这条边上安装，而不是入职之后再补装。
//! 组织未要求、Agent 也无可选包时，只做状态流转（合法，不是错误）。

use dioxus::prelude::*;

use crate::api::finance::list_tools;
use crate::api::hr::{
    list_installed_skill_packs, list_installed_tool_packs, onboard_agent, query_skills,
};
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

    // 锁定组：组织要求 / Agent 自带
    let mut required_tool = use_signal(Vec::<String>::new);
    let mut required_skill = use_signal(Vec::<String>::new);
    let mut inherent_tool = use_signal(Vec::<String>::new);
    let mut inherent_skill = use_signal(Vec::<String>::new);
    // 可额外安装
    let mut optional_tool = use_signal(Vec::<String>::new);
    let mut optional_skill = use_signal(Vec::<String>::new);
    // 勾选集合：锁定组预先填入，保证提交时必含
    let mut selected_tool_packs = use_signal(Vec::<String>::new);
    let mut selected_skill_packs = use_signal(Vec::<String>::new);
    let mut loading = use_signal(|| true);
    let mut submitting = use_signal(|| false);

    // 一次性加载三组包：组织要求 = 组织配置；Agent 自带 = 当前已安装（去掉组织要求）；
    // 可额外安装 = 全量候选 tags 里除上述两者外的剩余部分。
    let agent_id_for_load = props.agent_id.clone();
    use_effect(move || {
        let agent_id = agent_id_for_load.clone();
        spawn(async move {
            // ① 组织要求（OrganizationConfig.agent_onboard）
            let (req_tool, req_skill) = match get_current_organization().await {
                Ok(resp) => {
                    let onboard = resp.data.config.agent_onboard;
                    (onboard.required_tool_packs, onboard.required_skill_packs)
                }
                Err(_) => (Vec::new(), Vec::new()),
            };

            // ② Agent 自带（当前已安装的包；出生自带 + 职业匹配所得）
            let installed_tool = list_installed_tool_packs(&agent_id)
                .await
                .map(|r| r.installed_tags)
                .unwrap_or_default();
            let installed_skill = list_installed_skill_packs(&agent_id)
                .await
                .map(|r| r.skill_packs)
                .unwrap_or_default();

            // ③ 全量候选：工具侧取所有工具的 tags，技能侧取已发布技能的 tags
            let all_tool = list_tools(ListToolsRequest::default())
                .await
                .map(|page| {
                    page.items
                        .iter()
                        .flat_map(|t| t.tags.clone())
                        .collect::<Vec<String>>()
                })
                .unwrap_or_default();
            let all_skill = query_skills(&SkillQueryRequest {
                status: Some(SkillStatus::Published),
                ..Default::default()
            })
            .await
            .map(|page| {
                page.items
                    .iter()
                    .flat_map(|s| s.tags.clone())
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();

            let (inh_tool, opt_tool) = split_pack_groups(installed_tool, &req_tool, all_tool);
            let (inh_skill, opt_skill) = split_pack_groups(installed_skill, &req_skill, all_skill);

            // 锁定组默认选中
            selected_tool_packs.set(merge_packs(vec![req_tool.clone(), inh_tool.clone()]));
            selected_skill_packs.set(merge_packs(vec![req_skill.clone(), inh_skill.clone()]));

            required_tool.set(req_tool);
            required_skill.set(req_skill);
            inherent_tool.set(inh_tool);
            inherent_skill.set(inh_skill);
            optional_tool.set(opt_tool);
            optional_skill.set(opt_skill);
            loading.set(false);
        });
    });

    // 自定义 tag：补录进「可额外安装」候选并立即选中（工具包与技能包两侧同时登记）
    let on_add_custom = use_callback(move |tag: String| {
        let mut tools = optional_tool();
        if !tools.contains(&tag) {
            tools.push(tag.clone());
            optional_tool.set(dedup_sorted(tools));
        }
        let mut skills = optional_skill();
        if !skills.contains(&tag) {
            skills.push(tag.clone());
            optional_skill.set(dedup_sorted(skills));
        }
        let mut sel_tools = selected_tool_packs();
        if !sel_tools.contains(&tag) {
            sel_tools.push(tag.clone());
            selected_tool_packs.set(sel_tools);
        }
        let mut sel_skills = selected_skill_packs();
        if !sel_skills.contains(&tag) {
            sel_skills.push(tag);
            selected_skill_packs.set(sel_skills);
        }
    });

    let on_close = props.on_close;
    let agent_id = props.agent_id.clone();

    let handle_submit = move |_| {
        let agent_id = agent_id.clone();
        spawn(async move {
            submitting.set(true);
            // 锁定组必须随包送出（组织要求 + Agent 自带，均不可取消）
            let tool_packs = merge_packs(vec![
                required_tool(),
                inherent_tool(),
                selected_tool_packs(),
            ]);
            let skill_packs = merge_packs(vec![
                required_skill(),
                inherent_skill(),
                selected_skill_packs(),
            ]);
            let req = OnboardAgentRequest {
                id: agent_id,
                packs: Some(AgentPackSelection {
                    tool_packs,
                    skill_packs,
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
            title: "入职：确认工具包与技能包".to_string(),
            show: true,
            on_close: move |_| on_close.call(()),
            width_class: Some("max-w-3xl".to_string()),
            footer: rsx! {
                button { class: "btn hud-btn btn-ghost", onclick: move |_| on_close.call(()), "取消" }
                button {
                    class: "btn hud-btn btn-primary",
                    disabled: loading() || submitting(),
                    onclick: handle_submit,
                    if submitting() { "入职中..." } else { "确认入职" }
                }
            },
            div { class: "space-y-5",
                p { class: "text-sm text-base-content/60",
                    "组织要求与 Agent 自带的包默认选中且不可取消：前者是组织硬性要求，"
                    "后者是 Agent 出生自带 + 职业匹配所得的既有能力（当前已安装）。"
                    "其余包可按需勾选，随本次入职一并安装：工具包写入授权，技能包建立技能副本。"
                }

                if loading() {
                    p { class: "text-sm text-base-content/50 py-6 text-center", "正在加载可用包..." }
                } else {
                    div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                        PackPicker {
                            title: "工具包",
                            hint: "控制该 Agent 能调用哪些工具",
                            required: required_tool(),
                            inherent: inherent_tool(),
                            optional: optional_tool(),
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
                            required: required_skill(),
                            inherent: inherent_skill(),
                            optional: optional_skill(),
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

                    CustomPackInput { on_add: on_add_custom }
                }
            }
        }
    }
}

/// 去重 + 排序（包 tag 集合统一收敛到这里，保证展示顺序稳定）
fn dedup_sorted(mut tags: Vec<String>) -> Vec<String> {
    tags.sort();
    tags.dedup();
    tags
}

/// 多组包合并为一个去重有序集合
fn merge_packs(groups: Vec<Vec<String>>) -> Vec<String> {
    dedup_sorted(groups.into_iter().flatten().collect())
}

/// 拆分「Agent 自带」与「可额外安装」
///
/// - 自带 = 已安装的包去掉组织要求（组织要求单独成组，避免重复展示）；
/// - 可额外安装 = 候选包去掉上面两组。
fn split_pack_groups(
    installed: Vec<String>,
    required: &[String],
    all_candidates: Vec<String>,
) -> (Vec<String>, Vec<String>) {
    let inherent: Vec<String> = merge_packs(vec![installed])
        .into_iter()
        .filter(|tag| !required.contains(tag))
        .collect();
    let optional: Vec<String> = merge_packs(vec![all_candidates])
        .into_iter()
        .filter(|tag| !required.contains(tag) && !inherent.contains(tag))
        .collect();
    (inherent, optional)
}

/// 单个包选择区：组织要求（锁定）/ Agent 自带（锁定）/ 可额外安装（可勾选）
#[component]
fn PackPicker(
    title: String,
    hint: String,
    /// 组织要求：硬性，固定选中且不可取消
    required: Vec<String>,
    /// Agent 自带：固定选中且不可取消
    inherent: Vec<String>,
    /// 可额外安装：可自由勾选
    optional: Vec<String>,
    selected: Vec<String>,
    on_toggle: Callback<String>,
) -> Element {
    rsx! {
        div { class: "border border-base-300 rounded-lg p-3",
            div { class: "font-medium mb-1", "{title}" }
            div { class: "text-xs text-base-content/50 mb-3", "{hint}" }

            if !required.is_empty() {
                PackGroup {
                    label: "组织要求",
                    tags: required,
                    locked: true,
                    selected: selected.clone(),
                    on_toggle,
                }
            }
            if !inherent.is_empty() {
                PackGroup {
                    label: "Agent 自带",
                    tags: inherent,
                    locked: true,
                    selected: selected.clone(),
                    on_toggle,
                }
            }
            PackGroup {
                label: "可额外安装",
                tags: optional,
                locked: false,
                selected,
                on_toggle,
            }
        }
    }
}

/// 一组包：`locked` 时固定选中并置灰（不可取消勾选）
#[component]
fn PackGroup(
    label: String,
    tags: Vec<String>,
    locked: bool,
    selected: Vec<String>,
    on_toggle: Callback<String>,
) -> Element {
    rsx! {
        div { class: "mb-3",
            div { class: "flex items-center gap-2 mb-1",
                span { class: "text-xs font-medium text-base-content/70", "{label}" }
                if locked {
                    span { class: "badge orz-tag badge-xs", "锁定" }
                }
            }
            div { class: "max-h-40 overflow-y-auto space-y-1",
                if tags.is_empty() {
                    p { class: "text-sm text-base-content/40", "无" }
                }
                for tag in tags {
                    {
                        let is_locked = locked;
                        let is_checked = locked || selected.contains(&tag);
                        let tag_for_toggle = tag.clone();
                        let row_class = if is_locked {
                            "flex items-center gap-2 py-1 opacity-60 cursor-not-allowed"
                        } else {
                            "flex items-center gap-2 py-1 cursor-pointer"
                        };
                        let text_class = if is_locked {
                            "text-sm font-mono text-base-content/50"
                        } else {
                            "text-sm font-mono"
                        };
                        rsx! {
                            label { class: "{row_class}",
                                input {
                                    r#type: "checkbox",
                                    class: "checkbox checkbox-sm",
                                    checked: is_checked,
                                    disabled: is_locked,
                                    onclick: move |_| {
                                        if !is_locked {
                                            on_toggle.call(tag_for_toggle.clone());
                                        }
                                    },
                                }
                                span { class: "{text_class}", "{tag}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 自定义包 tag 输入
///
/// 独立子组件：输入态自有 signal，props 只有 Copy 的 `Callback`，
/// 父级因勾选变化重渲染时不会重建本输入框（避免打断中文输入）。
#[component]
fn CustomPackInput(on_add: Callback<String>) -> Element {
    let mut text = use_signal(String::new);

    rsx! {
        div { class: "form-control w-full",
            label { class: "label",
                span { class: "label-text", "补充自定义包 tag" }
            }
            input {
                class: "input hud-input w-full",
                placeholder: "输入 tag 后回车，同时加入工具包与技能包",
                value: "{text}",
                oninput: move |e| text.set(e.value()),
                onkeydown: move |e| {
                    if e.key() == Key::Enter {
                        e.prevent_default();
                        let tag = text().trim().to_string();
                        if tag.is_empty() {
                            return;
                        }
                        on_add.call(tag);
                        text.set(String::new());
                    }
                },
            }
        }
    }
}
