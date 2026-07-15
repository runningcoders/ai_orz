//! 认证状态管理 - 登录状态持久化、用户信息全局共享
//!
//! 认证基于 HttpOnly Cookie（JWT），前端不直接持有 token
//! 仅在 localStorage 保存登录状态标志位，用于 UI 判断

use dioxus::prelude::*;
use web_sys::Storage;

const LOGGED_IN_KEY: &str = "ai_orz_logged_in";

/// 获取 localStorage
fn get_storage() -> Option<Storage> {
    web_sys::window()?.local_storage().ok()?
}

/// 标记已登录
pub fn mark_logged_in() {
    if let Some(storage) = get_storage() {
        let _ = storage.set(LOGGED_IN_KEY, "true");
    }
}

/// 清除登录状态
pub fn clear_login_state() {
    if let Some(storage) = get_storage() {
        let _ = storage.remove_item(LOGGED_IN_KEY);
    }
}

/// 判断是否已登录（基于 localStorage 标志位）
pub fn is_logged_in() -> bool {
    get_storage()
        .and_then(|s| s.get(LOGGED_IN_KEY).ok()?)
        .map(|v| v == "true")
        .unwrap_or(false)
}

/// 全局认证状态 Signal
/// 在 App 根组件中通过 use_context_provider 初始化
#[derive(Clone, Debug, Default)]
pub struct AuthState {
    pub logged_in: bool,
    pub username: String,
    pub role: i32,
    pub org_id: String,
    pub org_name: String,
}

impl AuthState {
    /// 从 localStorage 恢复状态
    pub fn restore() -> Self {
        Self {
            logged_in: is_logged_in(),
            ..Default::default()
        }
    }

    pub fn is_logged_in(&self) -> bool {
        self.logged_in
    }

    pub fn is_admin(&self) -> bool {
        self.role >= 2
    }
}

/// 在根组件初始化全局认证状态
/// 使用方式：let auth = use_context_provider(|| Signal::new(AuthState::restore()));
pub fn use_auth_state() -> Signal<AuthState> {
    use_context()
}
