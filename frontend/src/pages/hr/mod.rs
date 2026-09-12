pub mod agent_detail;
pub mod agent_memory_panel;
pub mod agents;
pub mod create_agent_modal;
pub mod knowledge_graph;
pub mod memory_search;
pub mod onboard_modal;
pub mod skill_detail;
pub mod skills;

/// 角色标签预置项（SSOT，创建弹窗与详情页编辑共用）。
///
/// 只保留带系统语义的「接待入口」角色：走完入职流程后，`get_reception_agent`
/// 会优先选 `reception` 角色的已入职 Agent 作为对话入口。其余角色不预置 ——
/// 角色/能力标签会与工具包、技能包的 tag 关联（职业选择边按
/// `roles ∪ capabilities` 自动装配），由用户自行掌控。
pub const PRESET_ROLES: &[(&str, &str)] = &[
    ("reception", "Web前台接待"),
    ("feishu_reception", "飞书前台接待"),
];

/// 角色输入区的 label 提示（短）
pub const ROLES_LABEL_HINT: &str = "与工具/技能包 tag 关联，职业选择时按角色+能力自动装配能力";

/// 角色输入区的详细提示（长，chips 下方）
pub const ROLES_HINT: &str = "「前台接待」为系统对话入口角色：走完入职流程后会被对应对话渠道用作接待 Agent，并自动匹配 reception 工具/技能包。其余标签请自行输入，角色与能力关键词会关联工具包/技能包的 tag（职业选择时自动装配）。";
