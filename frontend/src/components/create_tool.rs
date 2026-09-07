//! 工具创建弹窗（通用容器）
//!
//! 容器职责：协议选择，以及跨协议共享的基础信息（name/description/tags/参数 Schema）
//! 与提交编排；协议特定字段由各自子表单组件渲染（`create_tool_http::HttpToolSubForm` /
//! `create_tool_shell::ShellToolSubForm`），实现按类型解耦。

use dioxus::prelude::*;

use crate::api::finance::create_tool;
use crate::components::create_tool_http::{
    HttpToolFormState, HttpToolSubForm, build_http_create_request,
};
use crate::components::create_tool_shell::{
    ShellToolFormState, ShellToolSubForm, build_shell_create_request,
};
use crate::components::modal::Modal;
use crate::components::state::ErrorAlert;
use crate::store::toast::use_toast;
use common::api::CreateToolRequest;

/// 容器内协议选择（前端本地 UI 状态）
///
/// 渲染顺序即选项展示顺序；提交时按类型分派到对应的 build_* 构造函数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateToolKind {
    Http,
    Shell,
}

impl CreateToolKind {
    /// 协议选项展示顺序（即容器渲染顺序）
    pub const ALL: [CreateToolKind; 2] = [CreateToolKind::Http, CreateToolKind::Shell];

    pub fn label(self) -> &'static str {
        match self {
            Self::Http => "HTTP 工具",
            Self::Shell => "Shell 工具",
        }
    }
}

/// 基础信息表单状态（跨协议共享字段）
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolBasicsState {
    pub name: String,
    pub description: String,
    pub tags: String,
    pub parameters_schema: String,
}

/// 基础信息校验（各协议 build_* 入口统一调用）
pub fn validate_basics(basics: &ToolBasicsState) -> Result<(), String> {
    if basics.name.trim().is_empty() {
        return Err("名称不能为空".to_string());
    }
    Ok(())
}

/// 工具创建弹窗（容器）
#[component]
pub fn CreateToolModal(
    show: bool,
    on_close: EventHandler<()>,
    on_created: EventHandler<()>,
) -> Element {
    let toast = use_toast();
    let mut kind = use_signal(|| CreateToolKind::Http);
    let mut basics = use_signal(ToolBasicsState::default);
    let mut http_form = use_signal(HttpToolFormState::default);
    let mut shell_form = use_signal(ShellToolFormState::default);
    let mut error_msg = use_signal(String::new);
    let mut submitting = use_signal(|| false);

    // 关闭时重置全部表单状态
    use_effect(move || {
        if !show {
            kind.set(CreateToolKind::Http);
            basics.set(ToolBasicsState::default());
            http_form.set(HttpToolFormState::default());
            shell_form.set(ShellToolFormState::default());
            error_msg.set(String::new());
        }
    });

    let submit = move |_| {
        // 按协议类型分派到对应子表单的构造函数（跨协议校验各自收敛）
        let built = match kind() {
            CreateToolKind::Http => build_http_create_request(&basics.read(), &http_form.read()),
            CreateToolKind::Shell => build_shell_create_request(&basics.read(), &shell_form.read()),
        };
        let req: CreateToolRequest = match built {
            Ok(req) => req,
            Err(e) => {
                error_msg.set(e);
                return;
            }
        };
        error_msg.set(String::new());
        submitting.set(true);
        spawn(async move {
            match create_tool(req).await {
                Ok(resp) => {
                    toast.success(format!("工具 {} 创建成功", resp.name));
                    submitting.set(false);
                    on_created.call(());
                    on_close.call(());
                }
                Err(e) => {
                    error_msg.set(format!("创建失败: {}", e));
                    submitting.set(false);
                }
            }
        });
    };

    rsx! {
        Modal {
            title: "创建工具".to_string(),
            show,
            on_close: move |_| on_close.call(()),
            footer: Some(rsx! {
                button {
                    class: "btn hud-btn btn-primary",
                    disabled: submitting(),
                    onclick: submit,
                    if submitting() { "创建中..." } else { "创建" }
                }
            }),
            div { class: "max-h-[70vh] overflow-y-auto space-y-3 pr-1",
                if !error_msg().is_empty() {
                    ErrorAlert { message: error_msg() }
                }

                // ===== 协议选择 =====
                div { class: "flex flex-wrap gap-2",
                    for k in CreateToolKind::ALL {
                        button {
                            class: if k == kind() {
                                "btn hud-btn btn-sm btn-primary"
                            } else {
                                "btn hud-btn btn-sm btn-ghost"
                            },
                            onclick: move |_| kind.set(k),
                            "{k.label()}"
                        }
                    }
                }

                // ===== 基础信息（跨协议共享） =====
                div { class: "form-control",
                    label { class: "form-label", "名称 *" }
                    input {
                        class: "input input-bordered hud-input w-full",
                        placeholder: "例如：weather_query",
                        value: "{basics.read().name}",
                        oninput: move |e| basics.write().name = e.value(),
                    }
                }
                div { class: "form-control",
                    label { class: "form-label", "描述" }
                    textarea {
                        class: "textarea textarea-bordered hud-input w-full",
                        rows: 2,
                        placeholder: "工具用途说明（供 Agent 理解何时调用）",
                        value: "{basics.read().description}",
                        oninput: move |e| basics.write().description = e.value(),
                    }
                }
                div { class: "form-control",
                    label { class: "form-label", "标签（逗号分隔）" }
                    input {
                        class: "input input-bordered hud-input w-full",
                        placeholder: "例如：weather,query",
                        value: "{basics.read().tags}",
                        oninput: move |e| basics.write().tags = e.value(),
                    }
                }
                div { class: "form-control",
                    label { class: "form-label", "参数 Schema（JSON Schema，可选）" }
                    textarea {
                        class: "textarea textarea-bordered hud-input w-full font-mono text-xs",
                        rows: 3,
                        placeholder: r#"{{"type":"object","properties":{{"city":{{"type":"string"}}}},"required":["city"]}}"#,
                        value: "{basics.read().parameters_schema}",
                        oninput: move |e| basics.write().parameters_schema = e.value(),
                    }
                }

                // ===== 协议子表单（按类型解耦渲染） =====
                {match kind() {
                    CreateToolKind::Http => rsx! {
                        HttpToolSubForm { form: http_form }
                    },
                    CreateToolKind::Shell => rsx! {
                        ShellToolSubForm { form: shell_form }
                    },
                }}
            }
        }
    }
}
