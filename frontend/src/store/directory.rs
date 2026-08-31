//! 全局名称目录（Agent 名 / 用户名）
//!
//! ## 为什么需要
//! 消息气泡要显示「发送者是谁」，但 `MessageListItem` 只带 `from_id` / `from_role`，
//! 不含名字。与其让每个页面各查各的，这里统一预载一份「ID → 展示名」映射并全局共享：
//! - Agent：`list_agents` 全量（组织内 Agent 数量有限，一次拉完）
//! - 用户：`list_users` 全量（组织内成员，一次拉完）
//!
//! ## 加载策略
//! 等待登录态就绪后触发一次加载（`started` 门闩保证只发一次）。
//! 加载失败会打到浏览器控制台（`console.error` / `console.warn`）而不是静默吞掉 ——
//! 名字是增强信息，不该因为目录拉取失败打断主流程（气泡会回退到 `角色 + 短 ID`，
//! 见 `utils::message::resolve_sender_name`），但失败必须可见，否则只能靠聊天气泡的
//! 按需 `get_agent` 兜底，排查困难。

use dioxus::prelude::*;
use wasm_bindgen::JsValue;
use web_sys::console;

use crate::api::hr::list_agents;
use crate::api::organization::list_users;
use crate::store::auth::use_auth_state;
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
    // 进程级门闩：保证整页生命周期只发一次预载请求
    let mut started = use_signal(|| false);
    // 依赖登录态：未登录（token 未就绪）时不要抢跑请求，否则会收到 401 导致目录永久为空
    let auth = use_auth_state();

    use_effect(move || {
        if started() || !auth.read().logged_in {
            return;
        }
        started.set(true);

        spawn(async move {
            // —— Agent 名 ——
            match list_agents(ListAgentsRequest {
                pagination: PaginationParams {
                    limit: Some(DIRECTORY_FETCH_LIMIT),
                    offset: None,
                },
            })
            .await
            {
                Ok(resp) => {
                    let mut agents = NameMap::new();
                    for a in resp.items {
                        agents.insert(a.id, a.name);
                    }
                    directory.write().agents = agents;
                }
                Err(e) => {
                    console::error_1(&JsValue::from_str(&format!(
                        "[directory] list_agents 预载失败：{e}，Agent 消息将回退为短 ID 展示"
                    )));
                }
            }

            // —— 用户名 ——
            match list_users().await {
                Ok(resp) => {
                    let mut users = NameMap::new();
                    for u in resp.data {
                        let name = u
                            .display_name
                            .filter(|n| !n.trim().is_empty())
                            .unwrap_or(u.username);
                        users.insert(u.user_id, name);
                    }
                    directory.write().users = users;
                }
                Err(e) => {
                    console::warn_1(&JsValue::from_str(&format!(
                        "[directory] list_users 预载失败：{e}，用户消息将回退为短 ID 展示"
                    )));
                }
            }

            directory.write().loaded = true;
        });
    });

    directory
}

/// 读取全局名称目录
pub fn use_directory() -> Signal<Directory> {
    use_context::<Signal<Directory>>()
}
