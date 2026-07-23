//! 备份管理页面 - 列表 / 创建 / 删除 / 恢复脚本预览

use dioxus::prelude::*;

use crate::api::system::{
    create_backup, delete_backup, get_restore_script, list_backups, BackupInfo,
};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use crate::utils::{format_file_size as format_size, format_rfc3339 as format_timestamp};

/// 截断 MD5（取前 16 位并加省略号）
fn truncate_md5(md5: &str) -> String {
    if md5.len() > 16 {
        format!("{}…", &md5[..16])
    } else {
        md5.to_string()
    }
}

/// 复制文本到剪贴板，并通过 toast 反馈
fn copy_to_clipboard(content: &str, toast: crate::store::toast::ToastState) {
    if let Some(window) = web_sys::window() {
        let navigator = window.navigator();
        let clipboard = navigator.clipboard();
        let promise = clipboard.write_text(content);
        let toast = toast;
        wasm_bindgen_futures::spawn_local(async move {
            match wasm_bindgen_futures::JsFuture::from(promise).await {
                Ok(_) => toast.info("已复制到剪贴板"),
                Err(_) => toast.error("复制失败"),
            }
        });
    } else {
        toast.error("剪贴板不可用");
    }
}

#[component]
pub fn SystemBackup() -> Element {
    let toast = use_toast();

    let mut backups = use_signal(Vec::<BackupInfo>::new);
    let loading = use_signal(|| true);
    let mut creating = use_signal(|| false);

    // 恢复脚本弹窗
    let mut show_restore_modal = use_signal(|| false);
    let mut restore_version = use_signal(|| Option::<u64>::None);
    let mut restore_script = use_signal(String::new);
    let mut restore_loading = use_signal(|| false);

    // 删除确认弹窗
    let mut show_delete_modal = use_signal(|| false);
    let mut delete_version = use_signal(|| Option::<u64>::None);
    let mut delete_loading = use_signal(|| false);

    /// 刷新备份列表
    fn reload(
        mut loading: Signal<bool>,
        mut backups: Signal<Vec<BackupInfo>>,
        toast: crate::store::toast::ToastState,
    ) {
        loading.set(true);
        spawn(async move {
            match list_backups().await {
                Ok(list) => backups.set(list),
                Err(e) => toast.error(&format!("加载备份列表失败: {}", e)),
            }
            loading.set(false);
        });
    }

    // 初始加载
    use_effect(move || {
        reload(loading, backups, toast);
    });

    // 创建备份
    let handle_create = move |_| {
        creating.set(true);
        spawn(async move {
            match create_backup().await {
                Ok(info) => {
                    toast.success(&format!("已创建备份 v{}", info.version));
                    match list_backups().await {
                        Ok(list) => backups.set(list),
                        Err(e) => toast.error(&format!("刷新列表失败: {}", e)),
                    }
                }
                Err(e) => toast.error(&format!("创建备份失败: {}", e)),
            }
            creating.set(false);
        });
    };

    // 点击恢复按钮
    let mut on_click_restore = move |version: u64| {
        restore_version.set(Some(version));
        restore_script.set(String::new());
        show_restore_modal.set(true);
        restore_loading.set(true);
        spawn(async move {
            match get_restore_script(version).await {
                Ok(script) => restore_script.set(script),
                Err(e) => {
                    toast.error(&format!("获取恢复脚本失败: {}", e));
                    show_restore_modal.set(false);
                }
            }
            restore_loading.set(false);
        });
    };

    // 复制恢复脚本
    let handle_copy_script = move |_| {
        let s = restore_script();
        copy_to_clipboard(&s, toast);
    };

    // 点击删除按钮（打开确认弹窗）
    let mut on_click_delete = move |version: u64| {
        delete_version.set(Some(version));
        show_delete_modal.set(true);
    };

    // 确认删除
    let handle_confirm_delete = move |_| {
        let v = match delete_version() {
            Some(v) => v,
            None => return,
        };
        delete_loading.set(true);
        spawn(async move {
            match delete_backup(v).await {
                Ok(_) => {
                    toast.success(&format!("已删除备份 v{}", v));
                    show_delete_modal.set(false);
                    match list_backups().await {
                        Ok(list) => backups.set(list),
                        Err(e) => toast.error(&format!("刷新列表失败: {}", e)),
                    }
                }
                Err(e) => toast.error(&format!("删除失败: {}", e)),
            }
            delete_loading.set(false);
        });
    };

    let backups_list = backups.read().clone();
    let total_count = backups_list.len();
    let latest_version = backups_list.first().map(|b| b.version).unwrap_or(0);

    rsx! {
        AppLayout {
            div { class: "card bg-base-100 shadow-md",
                div { class: "card-header",
                    h2 { class: "card-title", "备份管理" }
                    div { class: "page-header-actions",
                        button {
                            class: "btn btn-ghost btn-sm",
                            onclick: move |_| reload(loading, backups, toast),
                            "🔄 刷新"
                        }
                        button {
                            class: "btn btn-primary",
                            disabled: creating(),
                            onclick: handle_create,
                            if creating() { "创建中..." } else { "+ 创建备份" }
                        }
                    }
                }

                // 顶部统计
                div { class: "overview-stats",
                    div { class: "overview-stat-item",
                        span { class: "overview-stat-value primary", "{total_count}" }
                        span { class: "overview-stat-label", "备份总数" }
                    }
                    div { class: "overview-stat-item",
                        span { class: "overview-stat-value warning", "{latest_version}" }
                        span { class: "overview-stat-label", "最新版本号" }
                    }
                }

                // 列表
                if loading() {
                    Loading {}
                } else if backups_list.is_empty() {
                    EmptyState { icon: "💾".to_string(), message: "暂无备份".to_string() }
                } else {
                    table { class: "table table-zebra",
                        thead { tr {
                            th { "版本" }
                            th { "时间" }
                            th { "文件名" }
                            th { "大小" }
                            th { "MD5" }
                            th { "操作" }
                        }}
                        tbody {
                            for b in backups_list.iter() {
                                {
                                    let version = b.version;
                                    let timestamp = b.timestamp.clone();
                                    let file_name = b.file_name.clone();
                                    let size = format_size(b.size_bytes);
                                    let md5_full = b.md5.clone();
                                    let md5_short = truncate_md5(&b.md5);

                                    rsx! {
                                        tr { key: "{version}",
                                            td { "data-label": "版本",
                                                span { class: "badge badge-info", "v{version}" }
                                            }
                                            td { class: "font-mono text-base-content/70", style: "white-space: nowrap;", "data-label": "时间",
                                                "{format_timestamp(&timestamp)}"
                                            }
                                            td { class: "font-mono", "data-label": "文件名", "{file_name}" }
                                            td { "data-label": "大小", "{size}" }
                                            td { "data-label": "MD5",
                                                span {
                                                    class: "font-mono text-base-content/70",
                                                    title: "{md5_full}",
                                                    "{md5_short}"
                                                }
                                            }
                                            td { "data-label": "操作",
                                                button {
                                                    class: "btn btn-outline btn-sm",
                                                    onclick: move |_| on_click_restore(version),
                                                    "恢复"
                                                }
                                                button {
                                                    class: "btn btn-error btn-sm",
                                                    onclick: move |_| on_click_delete(version),
                                                    "删除"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 恢复脚本弹窗
        Modal {
            title: format!(
                "恢复脚本 - v{}",
                restore_version().unwrap_or(0)
            ),
            show: show_restore_modal(),
            on_close: move |_| show_restore_modal.set(false),
            footer: rsx! {
                button {
                    class: "btn btn-ghost",
                    onclick: move |_| show_restore_modal.set(false),
                    "关闭"
                }
                button {
                    class: "btn btn-primary",
                    disabled: restore_loading() || restore_script().is_empty(),
                    onclick: handle_copy_script,
                    "📋 复制脚本"
                }
            },
            if restore_loading() {
                Loading {}
            } else {
                div {
                    div {
                        class: "alert alert-warning",
                        style: "margin-bottom: var(--space-4);",
                        "⚠️ 恢复操作将覆盖当前数据，请先停止服务并备份现有数据后再执行。"
                    }
                    pre {
                        class: "font-mono",
                        style: "white-space: pre-wrap; word-break: break-word; background: var(--color-mistral-black); color: var(--color-text-on-dark); padding: var(--space-4); border-radius: var(--radius-md); max-height: 360px; overflow: auto; font-size: 12px; line-height: 1.5;",
                        "{restore_script()}"
                    }
                }
            }
        }

        // 删除确认弹窗
        Modal {
            title: "确认删除备份".to_string(),
            show: show_delete_modal(),
            on_close: move |_| show_delete_modal.set(false),
            footer: rsx! {
                button {
                    class: "btn btn-ghost",
                    disabled: delete_loading(),
                    onclick: move |_| show_delete_modal.set(false),
                    "取消"
                }
                button {
                    class: "btn btn-error",
                    disabled: delete_loading(),
                    onclick: handle_confirm_delete,
                    if delete_loading() { "删除中..." } else { "确认删除" }
                }
            },
            div {
                p { style: "line-height: 1.6; margin-bottom: var(--space-3);",
                    "即将删除备份 "
                    strong { "v{delete_version().unwrap_or(0)}" }
                    "，此操作不可恢复。"
                }
                p { class: "text-base-content/70", style: "font-size: 13px;",
                    "删除后归档文件将被永久移除，无法找回。"
                }
            }
        }
    }
}
