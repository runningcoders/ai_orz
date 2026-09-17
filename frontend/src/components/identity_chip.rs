//! 身份 chip（IdentityChip）：头像 + 展示名，点头像展开信息卡
//!
//! ## 为什么需要
//! `TaskListItem.assignee_id` / `GetProjectResponse.owner_agent_id` 这类字段**只带 ID**，
//! 直接插进 rsx 就是把一串 ulid 打到界面上（「负责人：0a3f9c21-…」）—— 既读不懂，
//! 也点不开。本组件把 ID 换成可读形态：圆头像 + 展示名，点头像展开信息卡
//! （[`AvatarBubble`]，**与聊天页消息气泡同一实现**）。
//!
//! ## 复用边界
//! 本组件只管「ID → 展示名 + 行内布局」。浮层定位、卡片懒加载、卡片内容全部由
//! [`AvatarBubble`] 承担 —— 改展示口径只改那一处，这里不复制任何卡片逻辑。
//!
//! ## 展示名来源
//! 取自全局名称目录（`store::directory`，App 根部预载 Agent + 用户全量）。
//! 目录未命中时回退**短 ID**（前 6 后 4），绝不回退成完整 ID：目录拉取失败 /
//! 实体已删除 / 联邦组织外的实体都可能走到这条分支，界面至少要可读。

use dioxus::prelude::*;

use crate::components::avatar_bubble::{AvatarBubble, AvatarSize, AvatarTone, BubbleAlign};
use crate::store::directory::use_directory;
use crate::utils::message::NameMap;
use crate::utils::status::short_id;

/// 按归属类型把 ID 解析成展示名（纯函数，便于 host 单测）
///
/// `Agent` 查 Agent 名表、`User` 查用户表；未命中回退短 ID。
pub fn resolve_identity_name(
    id: &str,
    tone: AvatarTone,
    agents: &NameMap,
    users: &NameMap,
) -> String {
    let hit = match tone {
        AvatarTone::Agent => agents.get(id),
        AvatarTone::User => users.get(id),
    };
    hit.cloned().unwrap_or_else(|| short_id(id))
}

/// 用户卡的副标题：侧栏/列表里的用户是「组织内其他成员」的语境，
/// 不能沿用聊天气泡那套「当前用户」（那是「我发的消息」语境）。
const USER_SUBTITLE: &str = "组织成员";

/// 展示某个实体（Agent / 用户）的头像 + 名称，点头像展开信息卡。
///
/// - `id`：实体 ID（`tone` 决定查哪张名称表、弹哪种卡片）
/// - `tone`：归属类型。任务分配对象可直接由 `AvatarTone::from(AssigneeType::from_i32(t))` 得到
/// - `align`：浮层水平对齐，默认左对齐（头像在行首、卡片向右展开）
#[component]
pub fn IdentityChip(id: String, tone: AvatarTone, #[props(default)] align: BubbleAlign) -> Element {
    // 订阅全局目录：目录预载完成时 chip 自行重渲染，不需要宿主传名字
    let directory = use_directory();
    let name = {
        let dir = directory.read();
        resolve_identity_name(&id, tone, &dir.agents, &dir.users)
    };
    let is_agent = tone == AvatarTone::Agent;
    let agent_id = is_agent.then(|| id.clone());
    let user_id = (!is_agent).then(|| id.clone());
    let user_subtitle = (!is_agent).then(|| USER_SUBTITLE.to_string());
    rsx! {
        div { class: "flex items-center gap-1 min-w-0",
            AvatarBubble {
                name: name.clone(),
                tone,
                agent_id,
                user_id,
                user_subtitle,
                align,
                size: AvatarSize::Sm,
            }
            // ⚠️ flex 子项默认 `min-width:auto`，只给 `truncate` 不会截断长名 →
            // 必须同时给 `min-w-0`，否则窄侧栏会被长名字撑破
            span { class: "text-xs min-w-0 truncate", title: "{name}", "{name}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn maps() -> (NameMap, NameMap) {
        let mut agents = NameMap::new();
        agents.insert("agt_1".to_string(), "李雷".to_string());
        let mut users = NameMap::new();
        users.insert("usr_1".to_string(), "韩梅梅".to_string());
        (agents, users)
    }

    #[test]
    fn resolves_name_by_tone() {
        let (agents, users) = maps();
        assert_eq!(
            resolve_identity_name("agt_1", AvatarTone::Agent, &agents, &users),
            "李雷"
        );
        assert_eq!(
            resolve_identity_name("usr_1", AvatarTone::User, &agents, &users),
            "韩梅梅"
        );
    }

    /// 目录没命中的 ID 也必须可读：回退短 ID（前 6 后 4），**不是**整串 ID
    #[test]
    fn falls_back_to_short_id() {
        let (agents, users) = maps();
        let long_id = "01J8ZK4Q0P7X9Y2M3N5V6W8T4B";
        let name = resolve_identity_name(long_id, AvatarTone::Agent, &agents, &users);
        assert_eq!(name, "01J8ZK…8T4B");
        assert!(name.len() < long_id.len());
        // 短 ID 原样返回（`short_id` 对 <= 10 位不截断）
        assert_eq!(
            resolve_identity_name("agt_x", AvatarTone::Agent, &agents, &users),
            "agt_x"
        );
    }

    /// 同名 ID 落在另一张表上不串：Agent 表里的名字不该被当成用户命中
    #[test]
    fn tables_do_not_cross_hit() {
        let (agents, users) = maps();
        let mut same_id_agents = NameMap::new();
        same_id_agents.insert("u1".to_string(), "Agent 侧".to_string());
        let mut same_id_users = NameMap::new();
        same_id_users.insert("u1".to_string(), "用户侧".to_string());
        assert_eq!(
            resolve_identity_name("u1", AvatarTone::Agent, &same_id_agents, &same_id_users),
            "Agent 侧"
        );
        assert_eq!(
            resolve_identity_name("u1", AvatarTone::User, &same_id_agents, &same_id_users),
            "用户侧"
        );
        let _ = (agents, users);
    }
}
