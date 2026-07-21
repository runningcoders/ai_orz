//! 认证状态管理 - 登录状态持久化、用户信息全局共享
//!
//! 认证基于 HttpOnly Cookie（JWT），前端不直接持有 token
//! 仅在 localStorage 保存登录状态标志位，用于 UI 判断

use dioxus::prelude::*;

use crate::utils::local_storage;

const LOGGED_IN_KEY: &str = "ai_orz_logged_in";

pub fn mark_logged_in() {
    if let Some(storage) = local_storage() {
        let _ = storage.set(LOGGED_IN_KEY, "true");
    }
}

pub fn clear_login_state() {
    if let Some(storage) = local_storage() {
        let _ = storage.remove_item(LOGGED_IN_KEY);
    }
}

pub fn is_logged_in() -> bool {
    local_storage()
        .and_then(|s| s.get(LOGGED_IN_KEY).ok()?)
        .map(|v| v == "true")
        .unwrap_or(false)
}

#[derive(Clone, Debug, Default)]
pub struct AuthState {
    pub logged_in: bool,
    pub username: String,
    pub role: i32,
    pub org_id: String,
    #[allow(dead_code)]
    pub org_name: String,
}

impl AuthState {
    pub fn restore() -> Self {
        Self {
            logged_in: is_logged_in(),
            ..Default::default()
        }
    }

    #[allow(dead_code)]
    pub fn is_logged_in(&self) -> bool {
        self.logged_in
    }

    pub fn is_admin(&self) -> bool {
        self.role >= 2
    }
}

pub fn use_auth_state() -> Signal<AuthState> {
    use_context()
}
