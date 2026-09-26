//! 认证状态管理 - 登录状态持久化、用户信息全局共享
//!
//! 认证基于 HttpOnly Cookie（JWT），前端不直接持有 token
//! 仅在 localStorage 保存登录状态标志位，用于 UI 判断

use dioxus::prelude::*;

use crate::utils::local_store;

// 键名统一收敛至组件层 local_store::keys（旧明文键兼容读取见 local_store::legacy）

pub fn mark_logged_in() {
    let _ = local_store::set_json(local_store::keys::AUTH_LOGGED_IN, &true);
}

/// 持久化用户角色到 localStorage，供页面刷新后恢复管理员菜单显示
pub fn save_role(role: i32) {
    let _ = local_store::set_json(local_store::keys::AUTH_ROLE, &role);
}

pub fn clear_login_state() {
    let _ = local_store::remove(local_store::keys::AUTH_LOGGED_IN);
    let _ = local_store::remove(local_store::keys::AUTH_ROLE);
}

/// 持久化用户身份（username / display_name）到 localStorage。
///
/// 用于页面刷新后「同步」恢复顶栏显示名，避免只能依赖 /user/me 异步回填、
/// 而回填的 re-render 又未能可靠反映时，顶栏长期停留在 "用户" 占位。
/// 仅在值非空时写入，避免把已保存的好数据被一次空响应覆盖。
pub fn save_user_identity(username: &str, display_name: &str) {
    if !username.is_empty() {
        let _ = local_store::set_json(local_store::keys::AUTH_USERNAME, &username.to_string());
    }
    if !display_name.is_empty() {
        let _ = local_store::set_json(
            local_store::keys::AUTH_DISPLAY_NAME,
            &display_name.to_string(),
        );
    }
}

/// 完整登出：清除 localStorage + 重置内存中的 AuthState 信号
pub fn logout(mut auth: Signal<AuthState>) {
    clear_login_state();
    let _ = local_store::remove(local_store::keys::AUTH_USERNAME);
    let _ = local_store::remove(local_store::keys::AUTH_DISPLAY_NAME);
    let mut state = auth.write();
    state.logged_in = false;
    state.role = 0;
    state.user_id = String::new();
    state.username = String::new();
    state.display_name = String::new();
    state.org_id = String::new();
}

pub fn is_logged_in() -> bool {
    // 新键未命中回退旧明文键（"true" 恰为合法 JSON bool），命中即一次性迁移
    local_store::get_json_with_legacy::<bool>(
        local_store::keys::AUTH_LOGGED_IN,
        local_store::legacy::AUTH_LOGGED_IN,
    )
    .ok()
    .flatten()
    .unwrap_or(false)
}

fn restore_role() -> i32 {
    // 新键未命中回退旧明文键（"1" 恰为合法 JSON i32），命中即一次性迁移
    local_store::get_json_with_legacy::<i32>(
        local_store::keys::AUTH_ROLE,
        local_store::legacy::AUTH_ROLE,
    )
    .ok()
    .flatten()
    .unwrap_or(0)
}

/// 从 localStorage 读取一个字符串字段（用于 username / display_name 的同步恢复）
fn restore_string(key: &str, legacy_key: &str) -> String {
    // 新键未命中回退旧明文键（用户名 / 显示名旧编码为裸文本），命中即一次性迁移
    local_store::get_string_with_legacy(key, legacy_key)
        .ok()
        .flatten()
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
            username: restore_string(
                local_store::keys::AUTH_USERNAME,
                local_store::legacy::AUTH_USERNAME,
            ),
            display_name: restore_string(
                local_store::keys::AUTH_DISPLAY_NAME,
                local_store::legacy::AUTH_DISPLAY_NAME,
            ),
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
