//! 认证状态管理 - 登录状态持久化、用户信息全局共享
//!
//! 认证基于 HttpOnly Cookie（JWT），前端不直接持有 token
//! 仅在 localStorage 保存登录状态标志位，用于 UI 判断

use dioxus::prelude::*;

use crate::utils::local_storage;

const LOGGED_IN_KEY: &str = "ai_orz_logged_in";
const ROLE_KEY: &str = "ai_orz_role";
const USERNAME_KEY: &str = "ai_orz_username";
const DISPLAY_NAME_KEY: &str = "ai_orz_display_name";

pub fn mark_logged_in() {
    if let Some(storage) = local_storage() {
        let _ = storage.set(LOGGED_IN_KEY, "true");
    }
}

/// 持久化用户角色到 localStorage，供页面刷新后恢复管理员菜单显示
pub fn save_role(role: i32) {
    if let Some(storage) = local_storage() {
        let role_str = role.to_string();
        let _ = storage.set(ROLE_KEY, &role_str);
    }
}

pub fn clear_login_state() {
    if let Some(storage) = local_storage() {
        let _ = storage.remove_item(LOGGED_IN_KEY);
        let _ = storage.remove_item(ROLE_KEY);
    }
}

/// 持久化用户身份（username / display_name）到 localStorage。
///
/// 用于页面刷新后「同步」恢复顶栏显示名，避免只能依赖 /user/me 异步回填、
/// 而回填的 re-render 又未能可靠反映时，顶栏长期停留在 "用户" 占位。
/// 仅在值非空时写入，避免把已保存的好数据被一次空响应覆盖。
pub fn save_user_identity(username: &str, display_name: &str) {
    if let Some(storage) = local_storage() {
        if !username.is_empty() {
            let _ = storage.set(USERNAME_KEY, username);
        }
        if !display_name.is_empty() {
            let _ = storage.set(DISPLAY_NAME_KEY, display_name);
        }
    }
}

/// 完整登出：清除 localStorage + 重置内存中的 AuthState 信号
pub fn logout(mut auth: Signal<AuthState>) {
    clear_login_state();
    if let Some(storage) = local_storage() {
        let _ = storage.remove_item(USERNAME_KEY);
        let _ = storage.remove_item(DISPLAY_NAME_KEY);
    }
    let mut state = auth.write();
    state.logged_in = false;
    state.role = 0;
    state.user_id = String::new();
    state.username = String::new();
    state.display_name = String::new();
    state.org_id = String::new();
}

pub fn is_logged_in() -> bool {
    local_storage()
        .and_then(|s| s.get(LOGGED_IN_KEY).ok()?)
        .map(|v| v == "true")
        .unwrap_or(false)
}

fn restore_role() -> i32 {
    local_storage()
        .and_then(|s| s.get(ROLE_KEY).ok().flatten())
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(0)
}

/// 从 localStorage 读取一个字符串字段（用于 username / display_name 的同步恢复）
fn restore_string(key: &str) -> String {
    local_storage()
        .and_then(|s| s.get(key).ok().flatten())
        .unwrap_or_default()
}

#[derive(Clone, Debug, Default)]
pub struct AuthState {
    pub logged_in: bool,
    pub user_id: String,
    pub username: String,
    pub display_name: String,
    pub role: i32,
    pub org_id: String,
    #[allow(dead_code)]
    pub org_name: String,
}

impl AuthState {
    /// 获取显示名：优先 display_name，空时 fallback 到 username
    pub fn display_label(&self) -> &str {
        if !self.display_name.is_empty() {
            &self.display_name
        } else if !self.username.is_empty() {
            &self.username
        } else {
            "用户"
        }
    }

    pub fn restore() -> Self {
        // 修复 HIGH #1：之前 restore 只恢复 logged_in，role/username/org_id 全部丢失，
        // 导致刷新页面后管理员菜单消失。现在持久化恢复 role。
        // username/display_name 同步从 localStorage 恢复（回填福利）：刷新后顶栏立即显示
        // 正确的显示名，不必等 /user/me 异步回填的 re-render 命中。
        Self {
            logged_in: is_logged_in(),
            role: restore_role(),
            username: restore_string(USERNAME_KEY),
            display_name: restore_string(DISPLAY_NAME_KEY),
            ..Default::default()
        }
    }

    #[allow(dead_code)]
    pub fn is_logged_in(&self) -> bool {
        self.logged_in
    }
}

pub fn use_auth_state() -> Signal<AuthState> {
    use_context()
}
