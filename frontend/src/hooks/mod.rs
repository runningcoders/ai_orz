use dioxus::prelude::*;
use dioxus_router::use_navigator;

pub mod use_resource;
pub mod use_workspace_data;

use crate::api::organization::get_current_user_info;
use crate::pages::Route;
use crate::store::auth::{has_saved_role, logout, save_role, use_auth_state};
use crate::utils::local_storage;

#[allow(unused_imports)]
pub use use_resource::{ResourceState, use_resource};

pub fn use_breakpoint() -> Signal<bool> {
    use_context::<Signal<bool>>()
}

pub fn use_require_auth() -> bool {
    let mut auth = use_auth_state();
    let navigator = use_navigator();
    // 修复 E2E-2：门闩 + 持久化判据双保险，回填最多一次
    let mut role_restore_started = use_signal(|| false);

    use_effect(move || {
        if !auth.read().logged_in {
            navigator.replace(Route::Reception {});
        } else {
            // 修复 HIGH #1 + R-M1：刷新页面后 AuthState.restore() 仅恢复 logged_in 和
            // 持久化的 role；若 localStorage 从未存过 role（新浏览器/被清理），调用
            // /user/me 回填一次。修复 E2E-2：不再用 role == 0 判断（与 SuperAdmin=0 冲突）。
            let needs_role_restore = !has_saved_role();
            if needs_role_restore && !role_restore_started() {
                role_restore_started.set(true);
                spawn(async move {
                    match get_current_user_info().await {
                        Ok(resp) => {
                            let mut state = auth.write();
                            state.role = resp.data.role;
                            state.user_id = resp.data.user_id.clone();
                            state.username = resp.data.username.clone();
                            state.display_name = resp.data.display_name.clone().unwrap_or_default();
                            state.org_id = resp.data.organization_id.clone();
                            save_role(resp.data.role);
                        }
                        Err(e) => {
                            // 修 bug：后端清空数据/用户被删后 JWT 仍被浏览器携带，
                            // /user/me 返回 401（修复后端）或其他失败。
                            // 前端必须主动清登录态并跳接待页，否则进入"已登录但信息为空"
                            // 的假登录态。注意：网络层 handle_unauthorized 会对状态码
                            // 401 清 localStorage + 做 location 跳转，这里对非 401 的失败
                            // 做兜底（例：后端旧版返回 404、或网络层重入失败）。
                            let is_401 = e.http_status == 401;
                            logout(auth);
                            if !is_401 {
                                navigator.replace(Route::Reception {});
                            }
                        }
                    }
                });
            }
        }
    });

    auth.read().logged_in
}

pub const AVAILABLE_THEMES: &[(&str, &str)] = &[
    ("orz-light", "Orz 默认"),
    ("light", "Light"),
    ("dark", "Dark"),
    ("cupcake", "Cupcake"),
    ("emerald", "Emerald"),
    ("corporate", "Corporate"),
    ("nord", "Nord"),
    ("synthwave", "Synthwave"),
];

fn get_saved_theme() -> String {
    local_storage()
        .and_then(|s| s.get_item("ai_orz_theme").ok().flatten())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "orz-light".to_string())
}

fn set_html_theme(theme: &str) {
    if let Some(doc) = web_sys::window().and_then(|w| w.document())
        && let Some(html) = doc.document_element()
    {
        let _ = html.set_attribute("data-theme", theme);
    }
}

#[derive(Clone, Copy)]
pub struct ThemeController {
    theme: Signal<String>,
}

impl ThemeController {
    pub fn current(&self) -> String {
        (self.theme)()
    }

    pub fn set(&mut self, new_theme: String) {
        if let Some(storage) = local_storage() {
            let _ = storage.set_item("ai_orz_theme", &new_theme);
        }
        set_html_theme(&new_theme);
        self.theme.set(new_theme);
    }
}

/// 在根组件初始化全局主题状态（与 use_toast 模式一致）
///
/// 必须在 App 顶层调用一次，后续子组件通过 `use_theme()` 共享同一份 Signal，
/// 避免每个组件独立 use_signal 导致主题切换不能跨组件联动
pub fn use_provide_theme() -> ThemeController {
    let theme = use_context_provider(|| Signal::new(get_saved_theme()));

    use_effect(move || {
        set_html_theme(&(theme)());
    });

    ThemeController { theme }
}

/// 获取全局主题控制器（子组件中使用）
pub fn use_theme() -> ThemeController {
    let theme = use_context::<Signal<String>>();
    ThemeController { theme }
}
