//! Shell 工具子表单（由 `create_tool` 容器按协议渲染）
//!
//! 对齐后端 `ShellToolConfig` 结构：program（固定可执行文件）+ argv 模板
//! （每行一项，支持 `{{args.x}}` 占位符）+ working_dir + timeout_ms。
//! 执行侧参数逐项传递不经 `sh -c`（免注入），模板占位符语法与 HTTP 工具
//! 共用同一约定（`{{args.x}}`，后端统一校验）。
//! 凭据需求声明走工具级顶层契约（Shell scope 仅 Env binding），运行时按调用者
//! 身份解析后经 `shell_env::resolve` 注入子进程；预校验委托 common 单点。

use dioxus::prelude::*;

use crate::components::create_tool::{ToolBasicsState, validate_basics};
use crate::components::create_tool_http::parse_optional_u64;
use crate::components::credential_form::{
    all_credential_kinds, binding_name, injection_value_preview, kind_from_value,
    normalize_requirements, validate_requirements_scoped,
};
use common::api::CreateToolRequest;
use common::enums::ToolProtocol;
use common::models::{
    CredentialBinding, CredentialKind, CredentialRequirement, CredentialRequirementScope,
};

/// Shell 工具协议特定表单状态（全部为文本输入，提交时统一解析校验）
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShellToolFormState {
    /// 可执行文件（纯名称走 PATH 解析；不含空白）
    pub program: String,
    /// argv 模板：每行一项，支持 `{{args.x}}` 占位符
    pub args_template: String,
    /// 工作目录（绝对路径，可选；空 = 运行时默认取 base 数据目录）
    pub working_dir: String,
    /// 执行超时毫秒（可选，默认 60s，上限 10 分钟）
    pub timeout_ms: String,
    /// 凭据需求声明（Shell scope：仅 Env binding）
    pub credential_requirements: Vec<CredentialRequirement>,
}

/// 拆分 argv 模板文本：每行一项，忽略空行与纯空白行
pub fn parse_args_template(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// 校验并构造 common SSOT 结构体 `ShellToolConfig`（创建页与详情页更新共用同一构造点）
pub fn shell_config_from_form(
    form: &ShellToolFormState,
) -> Result<common::config::ShellToolConfig, String> {
    let program = form.program.trim().to_string();
    if program.is_empty() {
        return Err("可执行文件不能为空".to_string());
    }
    if program.chars().any(|c| c.is_whitespace()) {
        return Err("可执行文件不能包含空白（只能填单个程序名或绝对路径）".to_string());
    }
    let args_template = parse_args_template(&form.args_template);
    for item in &args_template {
        // 占位符语法粗检：{{ 不成对即拒（完整校验由后端统一执行）
        if item.matches("{{").count() != item.matches("}}").count() {
            return Err(format!("argv 模板项占位符不配对: {}", item));
        }
    }
    let working_dir = {
        let dir = form.working_dir.trim();
        if dir.is_empty() {
            None
        } else {
            if !dir.starts_with('/') {
                return Err("工作目录必须是绝对路径".to_string());
            }
            Some(dir.to_string())
        }
    };
    let timeout_ms = parse_optional_u64(&form.timeout_ms, "超时时间")?;
    // 凭据需求预校验（Shell 工具恒 Shell scope：仅 Env），规则本体与后端同源
    let requirements = normalize_requirements(form.credential_requirements.clone());
    validate_requirements_scoped(&requirements, CredentialRequirementScope::Shell)?;

    Ok(common::config::ShellToolConfig {
        program,
        args_template,
        working_dir,
        timeout_ms,
        credential_requirements: requirements,
    })
}

/// 由已有 config 反填 Shell 表单（详情页编辑入口；缺失字段 → 空串，由占位符提示缺省值）
///
/// 与 [`shell_config_from_form`] 互逆：`ShellToolConfig` 增删 / 改名字段时两处都要同步。
pub fn shell_form_from_config(config: Option<&serde_json::Value>) -> ShellToolFormState {
    let Some(object) = config.and_then(|c| c.as_object()) else {
        return ShellToolFormState::default();
    };
    ShellToolFormState {
        program: object
            .get("program")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        // argv 模板每行一项（与 parse_args_template 互逆）
        args_template: object
            .get("args_template")
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default(),
        working_dir: object
            .get("working_dir")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        timeout_ms: object
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .map(|n| n.to_string())
            .unwrap_or_default(),
        credential_requirements: object
            .get("credential_requirements")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
    }
}

/// 校验并构造 CreateToolRequest（纯函数，便于单测）
pub fn build_shell_create_request(
    basics: &ToolBasicsState,
    form: &ShellToolFormState,
) -> Result<CreateToolRequest, String> {
    validate_basics(basics)?;

    // 构造 common SSOT 结构体 `ShellToolConfig`（前后端共享同一类型定义，杜绝键名漂移），
    // 再序列化为 config JSON：Option 字段为 None 序列化成 null，与旧行为一致。
    // ⚠️ 先于 schema/tags 解析：保持既有错误报告顺序（字段级错误优先于 schema JSON 错误）
    let shell_config = shell_config_from_form(form)?;
    let requirements = shell_config.credential_requirements.clone();
    let config = serde_json::to_value(shell_config)
        .map_err(|e| format!("Shell 工具配置序列化失败: {}", e))?;
    let parameters_schema =
        crate::components::create_tool_http::parse_parameters_schema(&basics.parameters_schema)?;
    let tags = crate::components::create_tool_http::parse_comma_list(&basics.tags);

    Ok(CreateToolRequest {
        name: basics.name.trim().to_string(),
        description: basics.description.trim().to_string(),
        protocol: ToolProtocol::Shell,
        config: Some(config),
        parameters_schema,
        tags: if tags.is_empty() { None } else { Some(tags) },
        control_mode: None,
        enabled: None,
        // 工具级顶层契约：后端归并进 config.credential_requirements，避免详情
        // 回显时 config 内同名键被 redact_config 打成 [REDACTED] 后保存丢失
        credential_requirements: Some(requirements),
    })
}

/// Shell 工具子表单（协议特定字段渲染，状态由容器持有）
#[component]
pub fn ShellToolSubForm(mut form: Signal<ShellToolFormState>) -> Element {
    rsx! {
        div { class: "form-control",
            label { class: "form-label", "可执行文件 *" }
            input {
                class: "input input-bordered hud-input w-full",
                placeholder: "例如：ffmpeg 或 /usr/local/bin/ffmpeg",
                value: "{form.read().program}",
                oninput: move |e| form.write().program = e.value(),
            }
            div { class: "text-xs opacity-60 mt-1",
                "只能填单个程序名或绝对路径；执行时参数逐项传递，不经 shell 解释"
            }
        }
        div { class: "form-control",
            label { class: "form-label", "参数模板（每行一项）" }
            textarea {
                class: "textarea textarea-bordered hud-input w-full font-mono text-xs",
                rows: 5,
                placeholder: "-i\n{{args.input}}\n-c:v\nlibx264\n{{args.output}}",
                value: "{form.read().args_template}",
                oninput: move |e| form.write().args_template = e.value(),
            }
            div { class: "text-xs opacity-60 mt-1",
                "支持 {{{{args.x}}}} 占位符，运行时由参数 Schema 约束的入参填充"
            }
        }
        div { class: "form-control",
            label { class: "form-label", "工作目录（可选，绝对路径）" }
            input {
                class: "input input-bordered hud-input w-full",
                placeholder: "留空 = 默认数据目录",
                value: "{form.read().working_dir}",
                oninput: move |e| form.write().working_dir = e.value(),
            }
        }
        div { class: "form-control",
            label { class: "form-label", "超时（毫秒，可选，默认 60000，上限 600000）" }
            input {
                class: "input input-bordered hud-input w-full",
                placeholder: "60000",
                value: "{form.read().timeout_ms}",
                oninput: move |e| form.write().timeout_ms = e.value(),
            }
        }
        div { class: "border border-base-300 rounded-box p-3 space-y-2",
            div { class: "flex justify-between items-center",
                span { class: "text-sm font-semibold", "凭据需求（注入子进程环境变量）" }
                button { class: "btn hud-btn btn-ghost btn-xs",
                    onclick: move |_| {
                        form.write().credential_requirements.push(CredentialRequirement {
                            kind: CredentialKind::GenericToken,
                            platform: Some(String::new()),
                            field: None,
                            enhancer: None,
                            binding: CredentialBinding::Env { name: String::new() },
                        });
                    },
                    "+ 添加"
                }
            }
            p { class: "text-xs opacity-60",
                "只声明凭据类型与注入点，不填凭据原文；运行时按调用者身份解析后注入，模型与工具入参均看不到值。"
            }
            if form.read().credential_requirements.is_empty() {
                p { class: "text-xs text-base-content/40", "暂未声明凭据需求" }
            } else {
                div { class: "mt-2 space-y-2",
                    for (idx, req) in form.read().credential_requirements.clone().iter().enumerate() {
                        {
                            let kind_value = req.kind.as_str();
                            let platform_value = req.platform.clone().unwrap_or_default();
                            let field_value = req.field.clone().unwrap_or_default();
                            let env_name_value = binding_name(&req.binding).to_string();
                            let requires_platform = req.kind.requires_platform();
                            let preview = injection_value_preview(req);
                            let idx_remove = idx;
                            let idx_kind = idx;
                            let idx_platform = idx;
                            let idx_field = idx;
                            let idx_env = idx;
                            rsx! {
                                div { class: "border border-base-300 rounded-box p-3 space-y-2", key: "{idx}",
                                    div { class: "flex justify-between items-center",
                                        span { class: "text-xs font-semibold text-base-content/60", "需求 #{idx + 1}" }
                                        button { class: "btn hud-btn btn-ghost btn-xs text-error",
                                            onclick: move |_| { form.write().credential_requirements.remove(idx_remove); },
                                            "✕ 移除"
                                        }
                                    }
                                    div { class: "flex gap-2 flex-wrap",
                                        div { class: "form-control flex-1 min-w-[10rem]",
                                            label { class: "label", span { class: "label-text text-xs", "凭据类型 *" } }
                                            select { class: "select select-bordered hud-input select-sm w-full", value: "{kind_value}",
                                                onchange: move |e| {
                                                    let kind = kind_from_value(&e.value()).unwrap_or(CredentialKind::GenericToken);
                                                    let mut f = form.write();
                                                    let r = &mut f.credential_requirements[idx_kind];
                                                    r.kind = kind;
                                                    if !kind.requires_platform() { r.platform = None; }
                                                },
                                                for kind in all_credential_kinds() {
                                                    option { value: "{kind.as_str()}", selected: kind.as_str() == kind_value, "{kind.as_str()}" }
                                                }
                                            }
                                        }
                                        if requires_platform {
                                            div { class: "form-control flex-1 min-w-[8rem]",
                                                label { class: "label", span { class: "label-text text-xs", "平台标识 *" } }
                                                input { class: "input input-bordered hud-input input-sm w-full", value: "{platform_value}",
                                                    oninput: move |e| {
                                                        let v = e.value();
                                                        form.write().credential_requirements[idx_platform].platform =
                                                            if v.is_empty() { None } else { Some(v) };
                                                    },
                                                    placeholder: "如 my_platform"
                                                }
                                            }
                                        }
                                    }
                                    div { class: "flex gap-2 flex-wrap",
                                        div { class: "form-control flex-1 min-w-[8rem]",
                                            label { class: "label", span { class: "label-text text-xs", "提取字段（可选）" } }
                                            input { class: "input input-bordered hud-input input-sm w-full", value: "{field_value}",
                                                oninput: move |e| {
                                                    let v = e.value();
                                                    form.write().credential_requirements[idx_field].field =
                                                        if v.is_empty() { None } else { Some(v) };
                                                },
                                                placeholder: "如 token / api_key"
                                            }
                                        }
                                        div { class: "form-control flex-1 min-w-[8rem]",
                                            label { class: "label", span { class: "label-text text-xs", "注入环境变量名 *" } }
                                            input { class: "input input-bordered hud-input input-sm w-full font-mono", value: "{env_name_value}",
                                                oninput: move |e| {
                                                    let v = e.value();
                                                    let mut f = form.write();
                                                    if let CredentialBinding::Env { name } = &mut f.credential_requirements[idx_env].binding {
                                                        *name = v;
                                                    }
                                                },
                                                placeholder: "如 DEPLOY_TOKEN"
                                            }
                                        }
                                    }
                                    div { class: "text-xs text-base-content/50",
                                        code { class: "bg-base-200 rounded px-1", "{preview}" }
                                        " ← 注入值形态"
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

#[cfg(test)]
mod tests {
    use super::*;

    fn basics() -> ToolBasicsState {
        ToolBasicsState {
            name: "video_convert".into(),
            description: String::new(),
            tags: String::new(),
            parameters_schema: String::new(),
        }
    }

    fn form() -> ShellToolFormState {
        ShellToolFormState {
            program: "ffmpeg".into(),
            args_template: "-i\n{{args.input}}\n{{args.output}}".into(),
            working_dir: String::new(),
            timeout_ms: String::new(),
            credential_requirements: Vec::new(),
        }
    }

    #[test]
    fn build_request_ok() {
        let req = build_shell_create_request(&basics(), &form()).unwrap();
        assert_eq!(req.protocol, ToolProtocol::Shell);
        let config = req.config.unwrap();
        assert_eq!(config["program"], "ffmpeg");
        assert_eq!(config["args_template"][0], "-i");
        assert_eq!(config["working_dir"], serde_json::Value::Null);
        assert_eq!(config["timeout_ms"], serde_json::Value::Null);
        // SSOT 保险：前端产物必须能被 common 的共享结构体解析（键名漂移即在此暴露）
        let parsed: common::config::ShellToolConfig =
            serde_json::from_value(config.clone()).unwrap();
        assert_eq!(parsed.program, "ffmpeg");
        assert_eq!(parsed.args_template[0], "-i");
        assert_eq!(parsed.working_dir, None);
        assert_eq!(parsed.timeout_ms, None);
    }

    #[test]
    fn build_request_rejects_empty_program() {
        let mut f = form();
        f.program = "  ".into();
        assert!(build_shell_create_request(&basics(), &f).is_err());
    }

    #[test]
    fn build_request_rejects_program_with_whitespace() {
        let mut f = form();
        f.program = "echo hi".into();
        assert!(build_shell_create_request(&basics(), &f).is_err());
    }

    #[test]
    fn build_request_rejects_unbalanced_placeholder() {
        let mut f = form();
        f.args_template = "{{args.input".into();
        assert!(build_shell_create_request(&basics(), &f).is_err());
    }

    #[test]
    fn build_request_rejects_relative_working_dir() {
        let mut f = form();
        f.working_dir = "relative/path".into();
        assert!(build_shell_create_request(&basics(), &f).is_err());
    }

    #[test]
    fn build_request_rejects_bad_timeout() {
        let mut f = form();
        f.timeout_ms = "abc".into();
        assert!(build_shell_create_request(&basics(), &f).is_err());
    }

    #[test]
    fn build_request_carries_credential_requirements_top_level() {
        let mut f = form();
        f.credential_requirements = vec![CredentialRequirement {
            kind: CredentialKind::GenericToken,
            platform: Some("my_platform".to_string()),
            field: None,
            enhancer: None,
            binding: CredentialBinding::Env {
                name: "DEPLOY_TOKEN".to_string(),
            },
        }];
        let req = build_shell_create_request(&basics(), &f).unwrap();
        let reqs = req.credential_requirements.clone().unwrap();
        assert_eq!(reqs.len(), 1);
        assert_eq!(binding_name(&reqs[0].binding), "DEPLOY_TOKEN");
        // config 内同源（后端归并时顶层覆盖，值一致）
        assert_eq!(
            req.config.unwrap()["credential_requirements"][0]["binding"]["name"],
            "DEPLOY_TOKEN"
        );
    }

    #[test]
    fn build_request_rejects_non_env_binding_for_shell() {
        let mut f = form();
        f.credential_requirements = vec![CredentialRequirement {
            kind: CredentialKind::GithubToken,
            platform: None,
            field: None,
            enhancer: None,
            binding: CredentialBinding::Header {
                name: "authorization".to_string(),
            },
        }];
        assert!(build_shell_create_request(&basics(), &f).is_err());
    }

    #[test]
    fn parse_args_template_ignores_blank_lines() {
        let parsed = parse_args_template("-i\n\n  \n{{args.input}}\n");
        assert_eq!(parsed, vec!["-i".to_string(), "{{args.input}}".to_string()]);
    }

    #[test]
    fn build_request_requires_name() {
        let mut b = basics();
        b.name = " ".into();
        assert!(build_shell_create_request(&b, &form()).is_err());
    }

    // ===== 详情页复用：config ⇄ 表单 往返（新增字段必须两处同步，否则此组测试失败）=====

    #[test]
    fn shell_form_from_none_config_is_default() {
        assert_eq!(shell_form_from_config(None), ShellToolFormState::default());
    }

    #[test]
    fn shell_form_from_config_fills_all_fields() {
        let config = serde_json::json!({
            "program": "ffmpeg",
            "args_template": ["-i", "{{args.input}}", "{{args.output}}"],
            "working_dir": "/tmp/work",
            "timeout_ms": 60000
        });
        let form = shell_form_from_config(Some(&config));
        assert_eq!(form.program, "ffmpeg");
        assert_eq!(form.args_template, "-i\n{{args.input}}\n{{args.output}}");
        assert_eq!(form.working_dir, "/tmp/work");
        assert_eq!(form.timeout_ms, "60000");

        // 缺失 / null → 空串（由占位符提示缺省值）
        let sparse = shell_form_from_config(Some(&serde_json::json!({
            "program": "ffmpeg", "working_dir": null, "timeout_ms": null
        })));
        assert_eq!(sparse.args_template, "");
        assert_eq!(sparse.working_dir, "");
        assert_eq!(sparse.timeout_ms, "");
    }

    #[test]
    fn shell_config_round_trip_preserves_fields() {
        // 创建页 → config → 详情页表单 → config：字段集必须等价（否则详情页保存会丢字段）
        let mut f = form();
        f.working_dir = "/tmp/work".into();
        f.timeout_ms = "120000".into();

        let json = serde_json::to_value(shell_config_from_form(&f).unwrap()).unwrap();
        let back = shell_form_from_config(Some(&json));
        assert_eq!(back, f, "config ⇄ 表单 往返应等价");

        // 反填结果再构造一次仍稳定（幂等）
        assert_eq!(
            serde_json::to_value(shell_config_from_form(&back).unwrap()).unwrap(),
            json
        );
    }
}
