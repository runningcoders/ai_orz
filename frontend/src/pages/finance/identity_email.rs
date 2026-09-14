//! 身份凭证 邮箱机器人区块（Finance → Identity 子组件）
//!
//! 管理用户自建代理邮箱凭证（kind = EmailBot）：按邮箱提供商（platform）预设
//! SMTP/IMAP 连接参数并预填表单，用户仅需补齐代理邮箱地址与授权码。
//! 密码/授权码加密落库永不回显（仅尾号 4 位）；支持增删改 + 设默认。
//! 邮箱渠道以 `(CredentialKind::EmailBot, platform)` 匹配解析到该凭证。
//!
//! 数据来源 = `GET /api/v1/finance/identity/email/status`（platform 空串返回全部提供商）。

use crate::components::hud::HudCallout;
use dioxus::prelude::*;

use crate::api::email_integration::{
    create_email_bot_credential, delete_email_bot_credential, get_email_integration_status,
    set_default_email_bot_credential, update_email_bot_credential,
};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::modal::Modal;
use crate::store::toast::use_toast;
use common::api::{
    CreateEmailBotCredentialRequest, EmailBotCredentialSnapshot, UpdateEmailBotCredentialRequest,
};

/// 邮箱提供商预设（顺序即下拉顺序；host/port 预填，hint 为开通引导）
struct ProviderMeta {
    slug: &'static str,
    caption: &'static str,
    smtp_host: &'static str,
    smtp_port: &'static str,
    imap_host: &'static str,
    imap_port: &'static str,
    password_placeholder: &'static str,
    hint: &'static str,
}

const PROVIDERS: &[ProviderMeta] = &[
    ProviderMeta {
        slug: "qq",
        caption: "QQ 邮箱",
        smtp_host: "smtp.qq.com",
        smtp_port: "465",
        imap_host: "imap.qq.com",
        imap_port: "993",
        password_placeholder: "16 位授权码，加密存储，永不回显",
        hint: "开通引导：① QQ 邮箱默认关闭 SMTP/IMAP 服务，需先到网页版「设置 → 账号」开启并生成 16 位授权码；② 修改 QQ 密码后全部授权码立即失效，需重新生成并更新此凭证；③ 发信量有配额（初始约每日百封，随账号信用提升），批量推送注意频控。",
    },
    ProviderMeta {
        slug: "163",
        caption: "网易 163 邮箱",
        smtp_host: "smtp.163.com",
        smtp_port: "465",
        imap_host: "imap.163.com",
        imap_port: "993",
        password_placeholder: "授权码，加密存储，永不回显",
        hint: "开通引导：到网易邮箱网页版「设置 → POP3/SMTP/IMAP」开启服务并获取授权码；登录第三方客户端使用授权码而非邮箱密码，修改密码后授权码失效需重新生成。",
    },
];

fn provider_meta(slug: &str) -> &'static ProviderMeta {
    PROVIDERS
        .iter()
        .find(|p| p.slug == slug)
        .unwrap_or(&PROVIDERS[0])
}

/// 邮箱机器人凭证子区块（嵌入 FinanceIdentity 页面）
#[component]
pub fn IdentityEmailSection() -> Element {
    let toast = use_toast();

    // ===== 状态聚合（platform 空串 = 全部提供商） =====
    let mut credentials = use_signal(Vec::<EmailBotCredentialSnapshot>::new);
    let mut loading = use_signal(|| true);

    // ===== 录入凭证 =====
    let mut show_create_modal = use_signal(|| false);
    let mut new_name = use_signal(String::new);
    let mut new_platform = use_signal(|| PROVIDERS[0].slug.to_string());
    let mut new_email_address = use_signal(String::new);
    let mut new_smtp_host = use_signal(String::new);
    let mut new_smtp_port = use_signal(String::new);
    let mut new_imap_host = use_signal(String::new);
    let mut new_imap_port = use_signal(String::new);
    let mut new_username = use_signal(String::new);
    let mut new_password = use_signal(String::new);
    let mut creating = use_signal(|| false);

    // ===== 编辑凭证 =====
    let mut show_edit_modal = use_signal(|| false);
    let mut edit_id = use_signal(String::new);
    let mut edit_platform = use_signal(String::new);
    let mut edit_name = use_signal(String::new);
    let mut edit_email_address = use_signal(String::new);
    let mut edit_smtp_host = use_signal(String::new);
    let mut edit_smtp_port = use_signal(String::new);
    let mut edit_imap_host = use_signal(String::new);
    let mut edit_imap_port = use_signal(String::new);
    let mut edit_username = use_signal(String::new);
    let mut edit_password = use_signal(String::new);
    let mut saving = use_signal(|| false);

    // ===== 删除凭证 =====
    let mut show_delete_confirm = use_signal(|| false);
    let mut pending_delete_id = use_signal(String::new);

    let refresh = move || {
        spawn(async move {
            loading.set(true);
            match get_email_integration_status("").await {
                Ok(s) => credentials.set(s.credentials),
                Err(e) => toast.error(format!("加载邮箱凭证状态失败: {}", e)),
            }
            loading.set(false);
        });
    };
    use_effect(refresh);

    // ===== 录入：切换提供商时预填连接参数 =====
    let mut handle_provider_change = move |slug: String| {
        new_platform.set(slug.clone());
        let meta = provider_meta(&slug);
        new_smtp_host.set(meta.smtp_host.to_string());
        new_smtp_port.set(meta.smtp_port.to_string());
        new_imap_host.set(meta.imap_host.to_string());
        new_imap_port.set(meta.imap_port.to_string());
    };

    // ===== 录入提交 =====
    let handle_create = move |_| {
        spawn(async move {
            let name = new_name();
            let platform = new_platform();
            let email_address = new_email_address();
            let smtp_host = new_smtp_host();
            let smtp_port_text = new_smtp_port();
            let imap_host = new_imap_host();
            let imap_port_text = new_imap_port();
            let username = new_username();
            let password = new_password();
            if name.trim().is_empty()
                || email_address.trim().is_empty()
                || smtp_host.trim().is_empty()
                || imap_host.trim().is_empty()
                || username.trim().is_empty()
                || password.trim().is_empty()
            {
                toast.error("名称 / 代理邮箱 / SMTP·IMAP 主机 / 登录账号 / 授权码 均为必填");
                return;
            }
            if !email_address.contains('@') {
                toast.error("代理邮箱地址格式不正确（需包含 @）");
                return;
            }
            let Ok(smtp_port) = smtp_port_text.trim().parse::<u16>() else {
                toast.error("SMTP 端口必须是 1-65535 的数字");
                return;
            };
            let Ok(imap_port) = imap_port_text.trim().parse::<u16>() else {
                toast.error("IMAP 端口必须是 1-65535 的数字");
                return;
            };
            if smtp_port == 0 || imap_port == 0 {
                toast.error("端口不能为 0");
                return;
            }
            creating.set(true);
            let req = CreateEmailBotCredentialRequest {
                name,
                platform,
                email_address,
                smtp_host,
                smtp_port,
                imap_host,
                imap_port,
                username,
                password,
            };
            match create_email_bot_credential(req).await {
                Ok(_) => {
                    show_create_modal.set(false);
                    new_name.set(String::new());
                    new_email_address.set(String::new());
                    new_username.set(String::new());
                    new_password.set(String::new());
                    toast.success("邮箱机器人凭证绑定成功");
                    refresh();
                }
                Err(e) => toast.error(format!("绑定失败: {}", e)),
            }
            creating.set(false);
        });
    };

    // ===== 编辑提交（留空保留原值，端口留空/0 不变） =====
    let handle_save = move |_| {
        spawn(async move {
            let id = edit_id();
            saving.set(true);
            let opt = |v: String| if v.trim().is_empty() { None } else { Some(v) };
            let parse_port = |v: String| -> Result<Option<u16>, String> {
                let t = v.trim().to_string();
                if t.is_empty() {
                    return Ok(None);
                }
                let p = t
                    .parse::<u16>()
                    .map_err(|_| format!("端口「{t}」不是有效数字"))?;
                if p == 0 {
                    return Ok(None);
                }
                Ok(Some(p))
            };
            let smtp_port = match parse_port(edit_smtp_port()) {
                Ok(v) => v,
                Err(msg) => {
                    toast.error(msg);
                    saving.set(false);
                    return;
                }
            };
            let imap_port = match parse_port(edit_imap_port()) {
                Ok(v) => v,
                Err(msg) => {
                    toast.error(msg);
                    saving.set(false);
                    return;
                }
            };
            let req = UpdateEmailBotCredentialRequest {
                id,
                name: opt(edit_name()),
                email_address: opt(edit_email_address()),
                smtp_host: opt(edit_smtp_host()),
                smtp_port,
                imap_host: opt(edit_imap_host()),
                imap_port,
                username: opt(edit_username()),
                password: opt(edit_password()),
            };
            match update_email_bot_credential(req).await {
                Ok(_) => {
                    show_edit_modal.set(false);
                    edit_password.set(String::new());
                    toast.success("凭证已更新，下次渠道出站即生效");
                    refresh();
                }
                Err(e) => toast.error(format!("更新失败: {}", e)),
            }
            saving.set(false);
        });
    };

    // ===== 设为默认 =====
    let handle_set_default = move |platform: String, credential_id: String| {
        spawn(async move {
            match set_default_email_bot_credential(&platform, &credential_id).await {
                Ok(_) => {
                    toast.success("默认凭证已更新");
                    refresh();
                }
                Err(e) => toast.error(format!("设置默认凭证失败: {}", e)),
            }
        });
    };

    // ===== 删除 =====
    let handle_delete = move |_| {
        let id = pending_delete_id();
        show_delete_confirm.set(false);
        spawn(async move {
            match delete_email_bot_credential(&id).await {
                Ok(_) => {
                    toast.success("凭证已删除");
                    refresh();
                }
                Err(e) => toast.error(format!("删除失败: {}", e)),
            }
        });
    };

    let list = credentials.read().clone();
    let is_loading = loading();
    let active_meta = provider_meta(&new_platform());
    let edit_platform_val = edit_platform();
    let edit_meta = provider_meta(&edit_platform_val);

    rsx! {
        div { class: "border border-base-300 rounded-lg p-4 mt-4",
            div { class: "flex items-center gap-2 flex-wrap",
                h3 { class: "font-semibold text-lg", "邮箱机器人" }
                span { class: "badge orz-tag badge-sm", "EmailBot" }
            }
            p { class: "text-xs text-base-content/50 mt-1",
                "用户自建代理邮箱（按提供商预填 SMTP/IMAP 参数）；邮箱渠道向对端地址推送全文消息，一期仅出站，IMAP 入站为二期能力。"
            }

            div { class: "border border-base-300 rounded-lg p-4 mt-3",
                div { class: "flex items-center justify-between flex-wrap gap-2",
                    h4 { class: "font-semibold", "代理邮箱凭证" }
                    button {
                        class: "btn hud-btn btn-sm btn-primary",
                        onclick: move |_| {
                            new_name.set(String::new());
                            new_email_address.set(String::new());
                            new_username.set(String::new());
                            new_password.set(String::new());
                            handle_provider_change(new_platform());
                            show_create_modal.set(true);
                        },
                        "+ 绑定代理邮箱"
                    }
                }
                if is_loading && list.is_empty() {
                    div { class: "text-base-content/50 text-sm py-4", "加载中..." }
                } else if list.is_empty() {
                    div { class: "text-sm text-base-content/50 py-3",
                        "尚未绑定代理邮箱；创建邮箱渠道前需先在此添加邮箱机器人凭证"
                    }
                } else {
                    div { class: "space-y-3 mt-3",
                        for cred in list.iter() {
                            {
                                let credential_id = cred.credential_id.clone();
                                let cred_name = cred.name.clone();
                                let platform = cred.platform.clone();
                                let email_address = cred.email_address.clone();
                                let password_tail = cred.password_tail.clone();
                                let is_default = cred.is_default;
                                let id_for_edit = credential_id.clone();
                                let id_for_delete = credential_id.clone();
                                let id_for_default = credential_id.clone();
                                let plat_for_default = platform.clone();
                                rsx! {
                                    div { key: "{credential_id}", class: "border border-base-200 rounded p-3",
                                        div { class: "flex items-center justify-between flex-wrap gap-2",
                                            div { class: "flex items-center gap-2 flex-wrap",
                                                span { class: "font-medium", "{cred_name}" }
                                                span { class: "badge orz-tag badge-sm font-mono", "{platform}" }
                                                span { class: "badge orz-tag badge-sm font-mono", "{email_address}" }
                                                if !password_tail.is_empty() {
                                                    span { class: "badge orz-tag badge-sm font-mono", "****{password_tail}" }
                                                }
                                                if is_default {
                                                    span { class: "badge hud-badge badge-success badge-sm", "默认" }
                                                }
                                            }
                                            div { class: "flex gap-2",
                                                if !is_default {
                                                    button {
                                                        class: "btn hud-btn btn-ghost btn-xs",
                                                        onclick: move |_| handle_set_default(plat_for_default.clone(), id_for_default.clone()),
                                                        "设为默认"
                                                    }
                                                }
                                                button {
                                                    class: "btn hud-btn btn-ghost btn-xs",
                                                    onclick: move |_| {
                                                        edit_id.set(id_for_edit.clone());
                                                        edit_platform.set(platform.clone());
                                                        edit_name.set(String::new());
                                                        edit_email_address.set(String::new());
                                                        edit_smtp_host.set(String::new());
                                                        edit_smtp_port.set(String::new());
                                                        edit_imap_host.set(String::new());
                                                        edit_imap_port.set(String::new());
                                                        edit_username.set(String::new());
                                                        edit_password.set(String::new());
                                                        show_edit_modal.set(true);
                                                    },
                                                    "编辑"
                                                }
                                                button {
                                                    class: "btn hud-btn btn-ghost btn-xs text-error",
                                                    onclick: move |_| {
                                                        pending_delete_id.set(id_for_delete.clone());
                                                        show_delete_confirm.set(true);
                                                    },
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

        // ===== 绑定代理邮箱 Modal =====
        Modal {
            title: "绑定代理邮箱".to_string(),
            show: show_create_modal(),
            on_close: move |_| show_create_modal.set(false),
            footer: rsx! {
                button { class: "btn hud-btn btn-ghost", onclick: move |_| show_create_modal.set(false), "取消" }
                button { class: "btn hud-btn btn-primary", disabled: creating(), onclick: handle_create,
                    if creating() { "绑定中..." } else { "绑定" }
                }
            },
            div { class: "space-y-4",
                div { class: "form-control w-full",
                    label { class: "label", span { class: "label-text font-medium", "凭证名称 *" } }
                    input {
                        class: "input input-bordered hud-input w-full",
                        value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()),
                        placeholder: "如：QQ 代理邮箱"
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label", span { class: "label-text font-medium", "邮箱提供商 *" } }
                    select {
                        class: "select select-bordered hud-input w-full",
                        value: "{new_platform}",
                        onchange: move |e| handle_provider_change(e.value()),
                        for meta in PROVIDERS.iter() {
                            option { key: "{meta.slug}", value: "{meta.slug}", "{meta.caption}" }
                        }
                    }
                    label { class: "label",
                        span { class: "label-text-alt", "切换提供商自动预填 SMTP/IMAP 连接参数（仍可手工修改）" }
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label", span { class: "label-text font-medium", "代理邮箱地址 *" } }
                    input {
                        class: "input input-bordered hud-input w-full font-mono",
                        value: "{new_email_address}",
                        oninput: move |e| {
                            let v = e.value();
                            if new_username().trim().is_empty() {
                                new_username.set(v.clone());
                            }
                            new_email_address.set(v);
                        },
                        placeholder: "如 bot@qq.com（对外主标识）"
                    }
                }
                div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "SMTP 主机 *" } }
                        input {
                            class: "input input-bordered hud-input w-full font-mono",
                            value: "{new_smtp_host}",
                            oninput: move |e| new_smtp_host.set(e.value()),
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "SMTP 端口 *" } }
                        input {
                            class: "input input-bordered hud-input w-full font-mono",
                            value: "{new_smtp_port}",
                            oninput: move |e| new_smtp_port.set(e.value()),
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "IMAP 主机（二期入站使用）*" } }
                        input {
                            class: "input input-bordered hud-input w-full font-mono",
                            value: "{new_imap_host}",
                            oninput: move |e| new_imap_host.set(e.value()),
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "IMAP 端口 *" } }
                        input {
                            class: "input input-bordered hud-input w-full font-mono",
                            value: "{new_imap_port}",
                            oninput: move |e| new_imap_port.set(e.value()),
                        }
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label", span { class: "label-text font-medium", "登录账号 *" } }
                    input {
                        class: "input input-bordered hud-input w-full font-mono",
                        value: "{new_username}",
                        oninput: move |e| new_username.set(e.value()),
                        placeholder: "多数提供商与邮箱地址相同"
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label", span { class: "label-text font-medium", "登录密码 / 授权码 *" } }
                    input {
                        class: "input input-bordered hud-input w-full font-mono",
                        r#type: "password",
                        value: "{new_password}",
                        oninput: move |e| new_password.set(e.value()),
                        placeholder: active_meta.password_placeholder
                    }
                }
                HudCallout { tone: Some("info".to_string()), extra_class: Some("mt-3".to_string()),
                    span { "{active_meta.hint}" }
                }
            }
        }

        // ===== 编辑凭证 Modal（留空保留原值） =====
        Modal {
            title: "编辑邮箱机器人凭证".to_string(),
            show: show_edit_modal(),
            on_close: move |_| show_edit_modal.set(false),
            footer: rsx! {
                button { class: "btn hud-btn btn-ghost", onclick: move |_| show_edit_modal.set(false), "取消" }
                button { class: "btn hud-btn btn-primary", disabled: saving(), onclick: handle_save,
                    if saving() { "保存中..." } else { "保存" }
                }
            },
            div { class: "space-y-4",
                div { class: "form-control w-full",
                    label { class: "label", span { class: "label-text font-medium", "凭证名称" } }
                    input {
                        class: "input input-bordered hud-input w-full",
                        value: "{edit_name}",
                        oninput: move |e| edit_name.set(e.value()),
                        placeholder: "留空保持不变"
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label", span { class: "label-text font-medium", "代理邮箱地址" } }
                    input {
                        class: "input input-bordered hud-input w-full font-mono",
                        value: "{edit_email_address}",
                        oninput: move |e| edit_email_address.set(e.value()),
                        placeholder: "留空保持不变"
                    }
                }
                div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "SMTP 主机" } }
                        input {
                            class: "input input-bordered hud-input w-full font-mono",
                            value: "{edit_smtp_host}",
                            oninput: move |e| edit_smtp_host.set(e.value()),
                            placeholder: "留空保持不变"
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "SMTP 端口" } }
                        input {
                            class: "input input-bordered hud-input w-full font-mono",
                            value: "{edit_smtp_port}",
                            oninput: move |e| edit_smtp_port.set(e.value()),
                            placeholder: "留空保持不变"
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "IMAP 主机" } }
                        input {
                            class: "input input-bordered hud-input w-full font-mono",
                            value: "{edit_imap_host}",
                            oninput: move |e| edit_imap_host.set(e.value()),
                            placeholder: "留空保持不变"
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "IMAP 端口" } }
                        input {
                            class: "input input-bordered hud-input w-full font-mono",
                            value: "{edit_imap_port}",
                            oninput: move |e| edit_imap_port.set(e.value()),
                            placeholder: "留空保持不变"
                        }
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label", span { class: "label-text font-medium", "登录账号" } }
                    input {
                        class: "input input-bordered hud-input w-full font-mono",
                        value: "{edit_username}",
                        oninput: move |e| edit_username.set(e.value()),
                        placeholder: "留空保持不变"
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label", span { class: "label-text font-medium", "登录密码 / 授权码" } }
                    input {
                        class: "input input-bordered hud-input w-full font-mono",
                        r#type: "password",
                        value: "{edit_password}",
                        oninput: move |e| edit_password.set(e.value()),
                        placeholder: "留空保留原值，填写则轮换"
                    }
                }
                HudCallout { tone: Some("info".to_string()), extra_class: Some("mt-3".to_string()),
                    span { "{edit_meta.hint}" }
                }
            }
        }

        ConfirmDialog {
            show: show_delete_confirm(),
            title: "确认删除凭证".to_string(),
            message: "删除后引用该凭证的邮箱渠道将无法出站推送，确定删除？".to_string(),
            on_confirm: handle_delete,
            on_cancel: move |_| show_delete_confirm.set(false),
        }
    }
}
