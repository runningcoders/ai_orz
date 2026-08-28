//! 全局名称目录（Agent 名 / 用户名）
//!
//! ## 为什么需要
//! 消息气泡要显示「发送者是谁」，但 `MessageListItem` 只带 `from_id` / `from_role`，
//! 不含名字。与其让每个页面各查各的，这里统一预载一份「ID → 展示名」映射并全局共享：
//! - Agent：`list_agents` 全量（组织内 Agent 数量有限，一次拉完）
//! - 用户：`list_users` 全量（组织内成员，一次拉完）
//!
//! ## 加载策略
//! 懒加载 + 单次：首个需要名字的页面触发加载，`loaded` 置位后不再请求。
//! 加载失败只记日志不弹 toast —— 名字是增强信息，不该因为目录拉取失败打断主流程
//! （气泡会回退到 `角色 + 短 ID`，见 `utils::message::resolve_sender_name`）。

use dioxus::prelude::*;

use crate::api::hr::list_agents;
use crate::api::organization::list_users;
use crate::utils::message::{NameMap, resolve_sender_name};
use common::api::{ListAgentsRequest, MessageListItem, PaginationParams};

/// 目录单次拉取上限（组织规模上限保护，超出部分回退为短 ID 展示）
const DIRECTORY_FETCH_LIMIT: usize = 500;

/// 全局名称目录
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Directory {
    /// Agent ID → Agent 名
    pub agents: NameMap,
    /// 用户 ID → 展示名（display_name 优先，回退 username）
    pub users: NameMap,
    /// 是否已尝试加载过（无论成功失败，避免反复重试打爆接口）
    pub loaded: bool,
}

impl Directory {
    /// 解析消息发送者展示名
    pub fn sender_name(&self, msg: &MessageListItem) -> String {
        resolve_sender_name(msg, &self.agents, &self.users)
    }
}

/// 提供全局名称目录，并触发一次懒加载
pub fn use_provide_directory() -> Signal<Directory> {
    let mut directory = use_context_provider(|| Signal::new(Directory::default()));

    use_effect(move || {
        if directory.read().loaded {
            return;
        }
        directory.write().loaded = true;

        spawn(async move {
            let mut agents = NameMap::new();
            if let Ok(resp) = list_agents(ListAgentsRequest {
                pagination: PaginationParams {
                    limit: Some(DIRECTORY_FETCH_LIMIT),
                    offset: None,
                },
            })
            .await
            {
                for a in resp.items {
                    agents.insert(a.id, a.name);
                }
            }

            let mut users = NameMap::new();
            if let Ok(resp) = list_users().await {
                for u in resp.data {
                    let name = u
                        .display_name
                        .filter(|n| !n.trim().is_empty())
                        .unwrap_or(u.username);
                    users.insert(u.user_id, name);
                }
            }

            directory.write().agents = agents;
            directory.write().users = users;
        });
    });

    directory
}

/// 读取全局名称目录
pub fn use_directory() -> Signal<Directory> {
    use_context::<Signal<Directory>>()
}
