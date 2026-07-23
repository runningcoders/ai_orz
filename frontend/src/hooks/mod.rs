use dioxus::prelude::*;
use dioxus_router::use_navigator;

pub mod use_resource;

use crate::api::organization::get_current_user_info;
use crate::pages::Route;
use crate::store::auth::{save_role, use_auth_state};
use crate::utils::local_storage;

#[allow(unused_imports)]
pub use use_resource::{use_resource, ResourceState};

pub fn use_breakpoint() -> Signal<bool> {
    use_context::<Signal<bool>>()
}

pub fn use_require_auth() -> bool {
    let mut auth = use_auth_state();
    let navigator = use_navigator();

    use_effect(move || {
        if !auth.read().logged_in {
            navigator.replace(Route::Reception {});
        } else {
            // 修复 HIGH #1 + R-M1：刷新页面后 AuthState.restore() 仅恢复 logged_in 和
            // 持久化的 role；若 role=0（旧版本未持久化 role 的缓存）或登录时硬编码
            // role=1 的旧 session，调用 /user/me 回填真实 role，避免管理员菜单消失。
            // 仅在 role=0 时触发，避免每次渲染都请求。
            let needs_role_restore = auth.read().role == 0;
            if needs_role_restore {
                spawn(async move {
                    match get_current_user_info().await {
                        Ok(resp) => {
                            let mut state = auth.write();
                            state.role = resp.data.role;
                            state.username = resp.data.username.clone();
                            state.org_id = resp.data.organization_id.clone();
                            // 持久化 role 供下次刷新直接恢复
                            save_role(resp.data.role);
                        }
                        Err(_) => {
                            // 获取用户信息失败（cookie 可能过期），静默处理
                            // 后续 API 调用会 401，由各页面自行处理
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
    ("bumblebee", "Bumblebee"),
    ("emerald", "Emerald"),
    ("corporate", "Corporate"),
    ("synthwave", "Synthwave"),
    ("retro", "Retro"),
    ("cyberpunk", "Cyberpunk"),
    ("valentine", "Valentine"),
    ("halloween", "Halloween"),
    ("garden", "Garden"),
    ("forest", "Forest"),
    ("aqua", "Aqua"),
    ("lofi", "Lofi"),
    ("pastel", "Pastel"),
    ("fantasy", "Fantasy"),
    ("luxury", "Luxury"),
    ("dracula", "Dracula"),
    ("autumn", "Autumn"),
    ("business", "Business"),
    ("night", "Night"),
    ("coffee", "Coffee"),
    ("winter", "Winter"),
    ("dim", "Dim"),
    ("nord", "Nord"),
    ("sunset", "Sunset"),
];

fn get_saved_theme() -> String {
    local_storage()
        .and_then(|s| s.get_item("ai_orz_theme").ok().flatten())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "orz-light".to_string())
}

fn set_html_theme(theme: &str) {
    if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
        if let Some(html) = doc.document_element() {
            let _ = html.set_attribute("data-theme", theme);
        }
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
