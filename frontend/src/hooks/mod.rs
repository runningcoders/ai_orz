use dioxus::prelude::*;
use dioxus_router::use_navigator;

pub mod use_resource;
pub mod use_workspace_data;

use crate::api::organization::get_current_user_info;
use crate::pages::Route;
use crate::store::auth::{has_saved_role, is_logged_in, logout, save_role, use_auth_state};
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

/// 全局登录态全生命周期探活（根组件 App 中仅需调用一次）
///
/// 背景：三道防线（/me 401 清 Cookie、受保护路由 use_require_auth 兜底、
/// 接待页自检）都依赖「切路由 / 发业务请求」触发。当用户停驻在已打开的受保护
/// 页面不做任何操作时，既不打 /user/me 也不发任何 HTTP，假登录态（后端清库、
/// 用户被删后 JWT Cookie 仍在）可残留到下次交互为止。
///
/// 本 hook 补上「即便是不操作也要能检出失效」这条最后一环，触发条件三选一：
///   1. 根组件挂载后立即探一次（冷启动兜底：打开页面就一直放着不动的情况）
///   2. 页面可见性从「隐藏 → 显示」切换时（切 tab 回来 / 最小化恢复 / 锁屏回来）
///   3. 每 10 分钟一次的弱心跳（前台发呆超过 10 分钟也会自检，而不是等交互）
///
/// 前提：仅当 localStorage `ai_orz_logged_in=true` 或内存 AuthState 仍认为已登录
/// 时才会真正打请求；未登录时三条通路都空转，不浪费任何流量。
///
/// 节流：inflight 门闩 + 30 秒最小间隔，防止 visibilitychange 事件在切换瞬间
/// 被浏览器连续触发数遍时叠加请求。
///
/// 失败处理（与 use_require_auth / 接待页自检完全同构，三处行为保持一致）：
///   - 401：网络层 `handle_unauthorized` 已清 localStorage + location 跳 /login，
///     本处不再重复跳转（避免重复 location 写入历史栈）
///   - 非 401（旧后端返回 404、临时网络失败、服务端停机等）：主动 `logout`
///     清脏 localStorage + 内存 AuthState，再 `navigator.replace(Reception)` 兜底
/// 成功：顺便回填 AuthState，保证长时间停驻后用户信息（角色、显示名、组织）
/// 也是最新的（一致性的免费福利）。
pub fn use_login_liveness() {
    let mut auth = use_auth_state();
    let navigator = use_navigator();
    let probe_inflight = use_signal(|| false);
    let last_probe_at = use_signal(|| 0f64);

    // 单次探活闭包：逻辑与 use_require_auth Err 分支完全对齐
    let do_probe = move || {
        // 本地干净 → 不打请求（任何失败都会在三道防线的某处清掉 logged_in）
        if !is_logged_in() && !auth.read().logged_in {
            return;
        }
        if probe_inflight() {
            return;
        }
        // 30s 内已有发起 → 跳过（visibilitychange 切 tab 瞬间连打会触发这个）
        if let Some(perf) = web_sys::window().and_then(|w| w.performance()) {
            let now = perf.now();
            if now - last_probe_at() < 30_000.0 {
                return;
            }
            last_probe_at.set(now);
        }

        probe_inflight.set(true);
        let mut auth = auth;
        let navigator = navigator;
        spawn(async move {
            let res = get_current_user_info().await;
            probe_inflight.set(false);
            match res {
                Ok(resp) => {
                    let mut state = auth.write();
                    state.logged_in = true;
                    state.role = resp.data.role;
                    state.user_id = resp.data.user_id.clone();
                    state.username = resp.data.username.clone();
                    state.display_name = resp.data.display_name.clone().unwrap_or_default();
                    state.org_id = resp.data.organization_id.clone();
                    save_role(resp.data.role);
                }
                Err(e) => {
                    let is_401 = e.http_status == 401;
                    logout(auth);
                    if !is_401 {
                        navigator.replace(Route::Reception {});
                    }
                }
            }
        });
    };

    use_effect(move || {
        let Some(window) = web_sys::window() else {
            return;
        };
        let Some(document) = window.document() else {
            return;
        };

        // 1) 挂载立刻探一次：冷启动兜底
        do_probe();

        // 2) 可见性事件：切 tab 回来 / 最小化恢复触发（只在 hidden=false 时探）
        let probe_vis = do_probe.clone();
        let cb_vis = wasm_bindgen::closure::Closure::new(move || {
            if document.hidden() {
                return;
            }
            probe_vis();
        });
        let _ = document
            .add_event_listener_with_callback("visibilitychange", cb_vis.as_ref().unchecked_ref());
        std::mem::forget(cb_vis);

        // 3) 10 分钟弱心跳：前台发呆也会被扫到
        let probe_int = do_probe;
        let cb_int = wasm_bindgen::closure::Closure::new(move || {
            probe_int();
        });
        let _ = window.set_interval_with_callback_and_timeout_and_arguments_0(
            cb_int.as_ref().unchecked_ref(),
            10 * 60 * 1000,
        );
        std::mem::forget(cb_int);
    });
}
