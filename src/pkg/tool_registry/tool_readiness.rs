//! 工具环境就绪判定纯函数与统一引导（三层就绪提示体系的 pkg 基础设施）
//!
//! 设计见 docs/design/web_search_and_browser_tools_design.md 决策 6/7/11 与
//! docs/design/tool_credential_requirement_design.md D28：
//! - **数据驱动**：就绪状态是 Tool 数据的纯派生——CLI 型（po.config.command）
//!   → 二进制可寻址；key 型（credential_requirements 非空）→ 用户凭据解析命中。
//!   取数与 TTL 缓存在 domain `tool_readiness`（runtime/tool_execution.rs），
//!   pkg 只留纯函数（与凭据解析 D17 同一哲学）。
//! - **统一引导**：`cli_not_installed` / `api_key_missing` 结构化 JSON（调用时兜底），
//!   供 lark_cli / gh_cli / browser / 内置搜索工具共用
//!
//! 分层说明：本模块零数据访问、零注册机制；引导文案常量与各工具 spawn 内
//! 文案统一来源，避免两处漂移。

use std::path::Path;

use serde_json::{Value, json};

use common::api::RuntimeReady;
use common::models::{CredentialKind, CredentialRequirement};

// ==================== CLI 型判定纯函数 ====================

/// 二进制可寻址：含路径分隔符（绝对/相对路径）直接判文件存在，纯名称扫 PATH
pub fn command_available(command: &str) -> bool {
    let path = Path::new(command);
    if path.is_absolute() || path.components().count() > 1 {
        return path.is_file();
    }
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path_var).any(|dir| dir.join(command).is_file())
}

/// CLI 型就绪判定（不可寻址 → not_ready{cli_not_installed}）
///
/// `install_hint` 为空（存量 PO config 无该字段）时仅输出 `config_hint`。
pub fn cli_binary_readiness(command: &str, install_hint: &str, config_hint: &str) -> RuntimeReady {
    if command_available(command) {
        RuntimeReady::Ready
    } else {
        let hint = if install_hint.is_empty() {
            config_hint.to_string()
        } else {
            format!("{}；{}", install_hint, config_hint)
        };
        RuntimeReady::NotReady {
            reason: "cli_not_installed".to_string(),
            hint,
        }
    }
}

// ==================== key 型判定引导 ====================

/// key 型未就绪引导（api_key_missing 的 hint 文案）：kind 是数据，文案是其纯函数
pub fn credential_missing_hint(requirement: &CredentialRequirement) -> String {
    match requirement.kind {
        CredentialKind::LarkApp => {
            "请先在个人设置的飞书集成中绑定应用，并创建引用该凭证的 Lark 渠道".to_string()
        }
        CredentialKind::GithubToken => {
            "请先在个人设置的 GitHub 集成中绑定访问令牌（Personal Access Token）".to_string()
        }
        // GenericToken 类凭据按 platform 出精细化引导（单字段 API Key 类多平台共用）
        CredentialKind::GenericToken => match requirement.platform.as_deref() {
            Some("tavily") => {
                "绑定个人 Tavily key（设置 → 身份凭证 → 通用令牌，platform 选 tavily）"
                    .to_string()
            }
            Some("doubao_search") => {
                "绑定豆包搜索 key（设置 → 身份凭证 → 通用令牌，platform 选 doubao_search）"
                    .to_string()
            }
            _ => "绑定个人通用令牌（设置 → 身份凭证 → 通用令牌）并设为默认".to_string(),
        },
        _ => "绑定个人凭据（设置 → 身份凭证）并设为默认".to_string(),
    }
}

// ==================== 统一引导构造（调用时兜底） ====================

/// CLI 未安装结构化引导（spawn NotFound 分支统一出口）
pub fn cli_not_installed_json(bin: &str, install_hint: &str, config_hint: &str) -> Value {
    json!({
        "success": false,
        "error_code": "cli_not_installed",
        "error": format!("未找到 {} 二进制，请先安装或配置路径", bin),
        "install": install_hint,
        "hint": config_hint
    })
}

/// 授权缺失结构化引导（引导绑定个人凭证）
pub fn api_key_missing_json(error: &str, guidance: &str) -> Value {
    json!({
        "success": false,
        "error_code": "api_key_missing",
        "error": error,
        "guidance": guidance
    })
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_available_absolute_path() {
        // 绝对路径：存在的文件 → true；不存在 → false
        assert!(command_available("/bin/ls"));
        assert!(!command_available("/no/such/binary-xyz"));
    }

    #[test]
    fn command_available_path_scan() {
        // 纯名称：macOS/Linux 环境必有 ls
        assert!(command_available("ls"));
        assert!(!command_available("no-such-binary-xyz-abc"));
    }

    #[test]
    fn cli_binary_readiness_shape() {
        assert_eq!(
            cli_binary_readiness("/bin/ls", "install x", "config hint"),
            RuntimeReady::Ready
        );
        let status = cli_binary_readiness("/no/such/binary-xyz", "install x", "config hint");
        assert_eq!(
            status,
            RuntimeReady::NotReady {
                reason: "cli_not_installed".to_string(),
                hint: "install x；config hint".to_string(),
            }
        );
    }

    #[test]
    fn cli_binary_readiness_empty_install_hint_falls_back_to_config_hint() {
        // 存量 PO config 无 install_hint → 仅输出 config_hint（不产生「；」前缀）
        let status = cli_binary_readiness("/no/such/binary-xyz", "", "config hint");
        assert_eq!(
            status,
            RuntimeReady::NotReady {
                reason: "cli_not_installed".to_string(),
                hint: "config hint".to_string(),
            }
        );
    }

    #[test]
    fn credential_missing_hint_by_kind() {
        let requirement = |kind: CredentialKind| CredentialRequirement {
            kind,
            platform: None,
            field: None,
            enhancer: None,
            binding: common::models::CredentialBinding::Internal {
                field: "credential".to_string(),
            },
        };
        assert!(
            credential_missing_hint(&requirement(CredentialKind::GithubToken)).contains("GitHub")
        );
        assert!(credential_missing_hint(&requirement(CredentialKind::LarkApp)).contains("飞书"));
        // generic 类 kind 走通用引导
        assert!(
            credential_missing_hint(&requirement(CredentialKind::GenericToken))
                .contains("身份凭证")
        );
        // GenericToken + platform=tavily 出 Tavily 专属引导
        let mut tavily_req = requirement(CredentialKind::GenericToken);
        tavily_req.platform = Some("tavily".to_string());
        assert!(credential_missing_hint(&tavily_req).contains("Tavily"));
        assert!(credential_missing_hint(&tavily_req).contains("tavily"));
        // GenericToken + platform=doubao_search 出豆包专属引导
        let mut doubao_req = requirement(CredentialKind::GenericToken);
        doubao_req.platform = Some("doubao_search".to_string());
        assert!(credential_missing_hint(&doubao_req).contains("豆包搜索"));
        assert!(credential_missing_hint(&doubao_req).contains("doubao_search"));
    }

    #[test]
    fn cli_not_installed_json_shape() {
        let v =
            cli_not_installed_json("agent-browser", "brew install agent-browser", "config hint");
        assert_eq!(v["success"], false);
        assert_eq!(v["error_code"], "cli_not_installed");
        assert!(v["error"].as_str().unwrap().contains("agent-browser"));
        assert_eq!(v["install"], "brew install agent-browser");
        assert_eq!(v["hint"], "config hint");
    }

    #[test]
    fn api_key_missing_json_shape() {
        let v = api_key_missing_json("no key", "guidance text");
        assert_eq!(v["success"], false);
        assert_eq!(v["error_code"], "api_key_missing");
        assert_eq!(v["error"], "no key");
        assert_eq!(v["guidance"], "guidance text");
    }
}
