//! 认证状态管理 - 登录状态持久化、用户信息全局共享
//!
//! 认证基于 HttpOnly Cookie（JWT），前端不直接持有 token
//! 仅在 localStorage 保存登录状态标志位，用于 UI 判断

use dioxus::prelude::*;

use common::enums::UserRole;

use crate::utils::local_storage;

const LOGGED_IN_KEY: &str = "ai_orz_logged_in";
const ROLE_KEY: &str = "ai_orz_role";

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

/// 完整登出：清除 localStorage + 重置内存中的 AuthState 信号
pub fn logout() {
    clear_login_state();
    let mut auth = use_auth_state();
    let mut state = auth.write();
    state.logged_in = false;
    state.role = 0;
    state.username = String::new();
    state.org_id = String::new();
}

pub fn is_logged_in() -> bool {
    local_storage()
        .and_then(|s| s.get(LOGGED_IN_KEY).ok()?)
        .map(|v| v == "true")
        .unwrap_or(false)
}

/// localStorage 中是否已持久化过角色（登录后 /user/me 回填或历史 session）
///
/// 修复 E2E-2：之前用 role == 0 作为「未恢复」哨兵，与 SuperAdmin=0 语义正面相撞，
/// 导致超管反复请求 /user/me。改用「键是否存在」作为恢复判据。
pub fn has_saved_role() -> bool {
    local_storage()
        .and_then(|s| s.get(ROLE_KEY).ok().flatten())
        .is_some()
}

fn restore_role() -> i32 {
    local_storage()
        .and_then(|s| s.get(ROLE_KEY).ok().flatten())
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(0)
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
        // 修复 HIGH #1：之前 restore 只恢复 logged_in，role/username/org_id 全部丢失，
        // 导致刷新页面后管理员菜单消失。现在持久化恢复 role。
        // username/org_id 仅用于 UI 显示，丢失影响较小，可后续通过接口回填。
        Self {
            logged_in: is_logged_in(),
            role: restore_role(),
            ..Default::default()
        }
    }

    #[allow(dead_code)]
    pub fn is_logged_in(&self) -> bool {
        self.logged_in
    }

    pub fn is_admin(&self) -> bool {
        // 修复 E2E-3：之前用 role >= 2 数字比较（且方向与枚举相反），导致超管/管理员
        // 菜单判断失效。统一走 UserRole::has_permission（并查集继承体系）：
        // 满足 Admin 最低权限即可（SuperAdmin 或 Admin）
        UserRole::has_permission(UserRole::from_i32(self.role), UserRole::Admin)
    }
}

pub fn use_auth_state() -> Signal<AuthState> {
    use_context()
}
