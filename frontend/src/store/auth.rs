//! 认证状态管理 - token 持久化、用户信息全局共享

use dioxus::prelude::*;
use web_sys::Storage;

const TOKEN_KEY: &str = "ai_orz_token";

/// 获取 localStorage
fn get_storage() -> Option<Storage> {
    web_sys::window()?.local_storage().ok()?
}

/// 保存 token 到 localStorage
pub fn save_token(token: &str) {
    if let Some(storage) = get_storage() {
        let _ = storage.set(TOKEN_KEY, token);
    }
}

/// 从 localStorage 读取 token
pub fn load_token() -> Option<String> {
    get_storage()?.get(TOKEN_KEY).ok()?
}

/// 清除 token
pub fn clear_token() {
    if let Some(storage) = get_storage() {
        let _ = storage.remove(TOKEN_KEY);
    }
}

/// 判断是否已登录
pub fn is_logged_in() -> bool {
    load_token().is_some()
}

/// 全局认证状态 Signal
/// 在 App 根组件中通过 use_context_provider 初始化
#[derive(Clone, Debug, Default)]
pub struct AuthState {
    pub token: Option<String>,
    pub username: String,
    pub role: i32,
    pub org_id: String,
    pub org_name: String,
}

impl AuthState {
    /// 从 localStorage 恢复状态
    pub fn restore() -> Self {
        Self {
            token: load_token(),
            ..Default::default()
        }
    }

    pub fn is_logged_in(&self) -> bool {
        self.token.is_some()
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
