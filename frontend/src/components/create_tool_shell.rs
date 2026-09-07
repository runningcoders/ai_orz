//! Shell 工具子表单（由 `create_tool` 容器按协议渲染）
//!
//! 对齐后端 `ShellToolConfig` 结构：program（固定可执行文件）+ argv 模板
//! （每行一项，支持 `{{args.x}}` 占位符）+ working_dir + timeout_ms。
//! 执行侧参数逐项传递不经 `sh -c`（免注入），模板占位符语法与 HTTP 工具
//! 共用同一约定（`{{args.x}}`，后端统一校验）。

use dioxus::prelude::*;

use crate::components::create_tool::{ToolBasicsState, validate_basics};
use crate::components::create_tool_http::parse_optional_u64;
use common::api::CreateToolRequest;
use common::enums::ToolProtocol;

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
}

/// 拆分 argv 模板文本：每行一项，忽略空行与纯空白行
pub fn parse_args_template(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// 校验并构造 CreateToolRequest（纯函数，便于单测）
pub fn build_shell_create_request(
    basics: &ToolBasicsState,
    form: &ShellToolFormState,
) -> Result<CreateToolRequest, String> {
    validate_basics(basics)?;
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
    let parameters_schema = crate::components::create_tool_http::parse_optional_json(
        &basics.parameters_schema,
        "参数 Schema",
    )?;
    let tags = crate::components::create_tool_http::parse_comma_list(&basics.tags);

    let config = serde_json::json!({
        "program": program,
        "args_template": args_template,
        "working_dir": working_dir,
        "timeout_ms": timeout_ms,
    });

    Ok(CreateToolRequest {
        name: basics.name.trim().to_string(),
        description: basics.description.trim().to_string(),
        protocol: ToolProtocol::Shell,
        config: Some(config),
        parameters_schema,
        tags: if tags.is_empty() { None } else { Some(tags) },
        control_mode: None,
        enabled: None,
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
}
