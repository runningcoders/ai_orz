//! tests 单元测试（拆分自 browser.rs）
//!
//! 文件瘦身：原 579 行 → 433 行，测试体 147 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use crate::pkg::request_context_test_support::new_test_ctx;

fn test_ctx() -> RequestContext {
    let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    new_test_ctx("test-user", pool)
}

#[test]
fn factory_po_metadata() {
    let po = BrowserToolFactory.create_po();
    assert_eq!(po.id, "browser");
    assert_eq!(po.control_mode, ControlMode::Manual);
    assert_eq!(po.protocol, ToolProtocol::Builtin);
    assert_eq!(po.get_tags(), vec!["browser", "network"]);
    // CLI 命令与运行参数进 PO config（D28 不变式：CLI 型 = po.config.command）
    assert_eq!(po.cli_command().as_deref(), Some(DEFAULT_COMMAND));
    assert!(
        po.cli_install_hint()
            .is_some_and(|hint| hint.contains("agent-browser"))
    );
    assert_eq!(po.config_timeout_ms(0), DEFAULT_TIMEOUT_MS);
    assert_eq!(po.config_max_output_bytes(0), DEFAULT_MAX_OUTPUT_BYTES);
    // 工厂默认不注入任何启动环境变量（继承服务进程环境）
    let cfg: BrowserConfig = serde_json::from_value(po.config.clone()).unwrap();
    assert!(cfg.env.is_none());
    // 默认放行本地/内网（测试场景开箱可用），且不限制子命令（开放 CLI 全部能力）
    assert_eq!(cfg.allow_local_network, None); // None → 运行时 unwrap_or(true)
    assert_eq!(cfg.disabled_commands, None);
    // 描述直接进提示词：保留起步引导与 snapshot-and-ref 心智模型，且不再有硬封禁话术
    let desc = po.description.as_str();
    for phrase in [
        "Pass ONE full command string",
        "skills get core",
        "refs go stale",
        "Prefer 'snapshot'/'read' over 'screenshot'",
        "'wait' then",
        "Nearly all CLI subcommands work",
    ] {
        assert!(desc.contains(phrase), "description missing '{phrase}'");
    }
    assert!(
        !desc.contains("**Manual mode**: each call pauses"),
        "不应再有『每次调用暂停等人审批』的过时话术"
    );
    // schema 已从 command/args 拆字段收敛为单个整串 command
    let schema = po.parameters_schema.as_ref().unwrap();
    let props = schema["properties"].as_object().unwrap();
    assert!(props.contains_key("command"));
    assert!(!props.contains_key("args"), "args 字段应已移除");
}

#[tokio::test]
async fn call_allows_local_open_by_default() {
    let tool = BrowserCoreTool {
        po: BrowserToolFactory.create_po(),
    };
    // 默认放行本地地址：不应再出现 URL 拒绝（本机未装 CLI 会走安装引导，装了则执行）
    let result = tool
        .call(
            test_ctx(),
            json!({ "command": "open http://localhost:3000/" }),
        )
        .await
        .unwrap();
    let error = result["error"].as_str().unwrap_or_default();
    assert!(
        !error.contains("本地/内网") && !error.contains("URL 被安全策略拒绝"),
        "默认不应拒绝本地地址: {error}"
    );
}

#[tokio::test]
async fn call_rejects_local_open_when_admin_disables() {
    let mut po = BrowserToolFactory.create_po();
    let mut config: BrowserConfig = serde_json::from_value(po.config.clone()).unwrap();
    config.allow_local_network = Some(false);
    po.config = serde_json::to_value(config).unwrap();
    let tool = BrowserCoreTool { po };
    // 管理员显式收紧时才拒绝，且拦截在 CLI 就绪检查之前
    let result = tool
        .call(
            test_ctx(),
            json!({ "command": "open http://localhost:3000/" }),
        )
        .await
        .unwrap();
    assert_eq!(result["success"], false);
    let error = result["error"].as_str().unwrap();
    assert!(error.contains("本地/内网"), "error: {error}");
}

#[tokio::test]
async fn call_rejects_cloud_metadata_even_with_local_allowed() {
    let mut po = BrowserToolFactory.create_po();
    let mut config: BrowserConfig = serde_json::from_value(po.config.clone()).unwrap();
    config.allow_local_network = Some(true);
    po.config = serde_json::to_value(config).unwrap();
    let tool = BrowserCoreTool { po };

    let result = tool
        .call(
            test_ctx(),
            json!({ "command": "open http://169.254.169.254/latest/meta-data/" }),
        )
        .await
        .unwrap();
    assert_eq!(result["success"], false);
    assert!(result["error"].as_str().unwrap().contains("云元数据"));
}

#[tokio::test]
async fn call_file_url_outside_data_root_requires_confirmation() {
    // 与 fs_read 对齐：数据根外的路径不是硬拒，而是 require_confirmation，
    // 让 Agent 停下来问用户授权（复用 resolve_and_validate_path 的同款语义）。
    let _ = crate::config::init();
    let tool = BrowserCoreTool {
        po: BrowserToolFactory.create_po(),
    };
    let result = tool
        .call(test_ctx(), json!({ "command": "open file:///etc/passwd" }))
        .await
        .unwrap();
    assert_eq!(result["success"], false);
    assert_eq!(result["require_confirmation"], true, "result: {result}");
    assert!(
        result["message"].as_str().unwrap().contains("confirm"),
        "result: {result}"
    );
}

#[tokio::test]
async fn call_rejects_file_url_when_admin_disables() {
    // 管理员显式配 allow_file_url=false ⇒ 完全禁用，不进 fs 校验、直接硬拒
    let mut po = BrowserToolFactory.create_po();
    let mut config: common::config::BrowserConfig =
        serde_json::from_value(po.config.clone()).unwrap();
    config.allow_file_url = Some(false);
    po.config = serde_json::to_value(config).unwrap();
    let tool = BrowserCoreTool { po };
    let result = tool
        .call(
            test_ctx(),
            json!({ "command": "open file:///tmp/whatever.html" }),
        )
        .await
        .unwrap();
    assert_eq!(result["success"], false);
    assert!(
        result["require_confirmation"].is_null(),
        "显式禁用是硬拒，不应走授权"
    );
    let error = result["error"].as_str().unwrap();
    assert!(
        error.contains("禁用") || error.contains("allow_file_url"),
        "error: {error}"
    );
}

#[tokio::test]
async fn call_file_url_inside_data_root_passes_url_check() {
    // 数据根内的 file:// 通过 URL 检查（后续因未装 CLI 走安装引导，但不含授权/URL 拒绝）
    let _ = crate::config::init();
    let base = crate::config::get().base_data_path();
    let _ = std::fs::create_dir_all(&base);
    let inside = base
        .canonicalize()
        .unwrap()
        .join("browser_file_inside.html");
    std::fs::write(&inside, "<html>ok</html>").unwrap();
    let url = format!("file://{}", inside.display());

    let tool = BrowserCoreTool {
        po: BrowserToolFactory.create_po(),
    };
    let result = tool
        .call(test_ctx(), json!({ "command": format!("open {url}") }))
        .await
        .unwrap();
    assert!(
        result["require_confirmation"].is_null(),
        "数据根内不应要求授权: {result}"
    );
    let error = result["error"].as_str().unwrap_or_default();
    assert!(
        !error.contains("URL 被安全策略拒绝"),
        "数据根内不应被 URL 拒绝: {error}"
    );
    let _ = std::fs::remove_file(&inside);
}

#[tokio::test]
async fn call_skips_url_check_for_non_open_commands() {
    let tool = BrowserCoreTool {
        po: BrowserToolFactory.create_po(),
    };
    // click 的参数不是 URL，不应被 URL 检查误伤（会因未装 CLI 走安装引导，但不含 URL 拒绝）
    let result = tool
        .call(test_ctx(), json!({ "command": "click @e1" }))
        .await
        .unwrap();
    assert!(
        !result["error"]
            .as_str()
            .unwrap_or_default()
            .contains("URL 被安全策略拒绝")
    );
}

#[test]
fn browser_config_env_roundtrip() {
    // 合法 env 配置能强类型往返（键 / 值均无丢失）
    let value = json!({
        "command": "agent-browser",
        "env": {
            "AGENT_BROWSER_EXECUTABLE_PATH": "/usr/bin/chromium",
            "AGENT_BROWSER_COLOR_SCHEME": "dark"
        }
    });
    let cfg: BrowserConfig = serde_json::from_value(value).unwrap();
    let env = cfg.env.expect("env 应解析");
    assert_eq!(env.len(), 2);
    assert_eq!(
        env.get("AGENT_BROWSER_EXECUTABLE_PATH").map(String::as_str),
        Some("/usr/bin/chromium")
    );
    assert_eq!(
        env.get("AGENT_BROWSER_COLOR_SCHEME").map(String::as_str),
        Some("dark")
    );
}

#[tokio::test]
async fn call_allows_eval_by_default() {
    // 默认不设限：eval 这种 CLI 原生能力也放行（想绕的人走 shell_exec 一样能跑）
    let tool = BrowserCoreTool {
        po: BrowserToolFactory.create_po(),
    };
    let result = tool
        .call(test_ctx(), json!({ "command": "eval alert(1)" }))
        .await
        .unwrap();
    assert!(
        !result["error"]
            .as_str()
            .unwrap_or_default()
            .contains("已被本工具配置禁用"),
        "默认不应拒绝 eval"
    );
}

#[tokio::test]
async fn call_enforces_configured_command_denylist() {
    // 管理员显式配置黑名单才屏蔽命中的命令，且提示引导去配置页恢复
    let mut po = BrowserToolFactory.create_po();
    let mut config: BrowserConfig = serde_json::from_value(po.config.clone()).unwrap();
    config.disabled_commands = Some(vec!["eval".into()]);
    po.config = serde_json::to_value(config).unwrap();
    let tool = BrowserCoreTool { po };

    let rejected = tool
        .call(test_ctx(), json!({ "command": "eval alert(1)" }))
        .await
        .unwrap();
    assert_eq!(rejected["success"], false);
    let error = rejected["error"].as_str().unwrap();
    assert!(error.contains("eval"), "error: {error}");
    assert!(error.contains("禁用"), "error: {error}");

    // 未列入黑名单的命令不被此项拦截（会因本机 CLI 状态走别的分支，但不含黑名单拒绝）
    let allowed = tool
        .call(test_ctx(), json!({ "command": "snapshot -i" }))
        .await
        .unwrap();
    assert!(
        !allowed["error"]
            .as_str()
            .unwrap_or_default()
            .contains("已被本工具配置禁用")
    );
}

#[tokio::test]
async fn session_id_isolates_per_agent() {
    let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    let storage = crate::pkg::storage::test_support::create_test_storage(pool.clone());
    let ctx_agent = RequestContext::builder()
        .user_id("u-1".to_string())
        .agent_id("agent-9".to_string())
        .storage(storage.clone())
        .build();
    let ctx_user_only = RequestContext::builder()
        .user_id("u-2".to_string())
        .storage(storage)
        .build();
    let ctx_anon = new_test_ctx("", pool);

    assert_eq!(session_id(&ctx_agent), "ai_orz_agent_agent_9");
    assert_eq!(session_id(&ctx_user_only), "ai_orz_agent_u_2");
    assert_eq!(session_id(&ctx_anon), "ai_orz_agent_default");
}

#[tokio::test]
async fn session_id_sanitizes_owner_to_valid_key() {
    // owner 退化为邮箱（@/.）或 UUID（-）时，非法字符折叠为下划线，key 始终合法。
    let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    let storage = crate::pkg::storage::test_support::create_test_storage(pool);
    let ctx = RequestContext::builder()
        .user_id("zhangsan@example.com".to_string())
        .storage(storage)
        .build();
    let key = session_id(&ctx);
    assert_eq!(key, "ai_orz_agent_zhangsan_example_com");
    assert!(
        key.chars().all(|c| c.is_alphanumeric() || c == '_'),
        "session key 只能含字母数字与下划线: {key}"
    );
}

#[test]
fn output_truncation() {
    let short = "hello";
    let (out, truncated, full) = combine_output(short, "", 1024);
    assert_eq!(out, short);
    assert!(!truncated);
    assert_eq!(full, 5);

    let long = "x".repeat(300);
    let (out, truncated, full) = combine_output(&long, "", 100);
    assert!(truncated);
    assert!(out.ends_with("[truncated]"));
    assert!(out.chars().count() <= 100 + 20);
    assert_eq!(full, 300);
}

#[test]
fn output_truncation_is_utf8_safe() {
    // 中文三字节：max 恰好切在多字节字符中间时不能 panic，回退到字符边界
    let text = "一二三四五".repeat(50); // 每字符 3 字节，共 750 字节
    let (out, truncated, full) = combine_output(&text, "", 100);
    assert!(truncated);
    assert_eq!(full, text.len());
    assert!(out.ends_with("... [truncated]"));
    // 前缀（去掉标记）字节数回退到 <= 上限的 char boundary，且内容是合法汉字
    let prefix = out.strip_suffix("\n\n... [truncated]").unwrap();
    assert!(prefix.len() <= 100);
    assert_eq!(prefix.len() % 3, 0, "应在三字节汉字边界处回退");
    assert!(
        prefix
            .chars()
            .all(|c| c == '一' || c == '二' || c == '三' || c == '四' || c == '五')
    );
}

#[tokio::test]
async fn persist_full_output_writes_call_id_log() {
    let dir = std::env::temp_dir().join(format!(
        "ai-orz-browser-log-test-{}",
        chrono::Utc::now().timestamp_millis()
    ));
    let ctx = test_ctx().to_builder().tool_call_id("call-xyz").build();
    let path = persist_full_output(&dir, &ctx, "full output here")
        .await
        .unwrap();
    assert!(path.to_string_lossy().contains("browser/logs/"));
    assert!(path.file_name().unwrap() == "call-xyz.log");
    let saved = tokio::fs::read_to_string(&path).await.unwrap();
    assert_eq!(saved, "full output here");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

// ===== 整串 command 分词 =====

#[test]
fn tokenize_splits_on_whitespace() {
    assert_eq!(
        tokenize_command("open https://example.com").unwrap(),
        vec!["open", "https://example.com"]
    );
    assert_eq!(
        tokenize_command("  snapshot   -i  ").unwrap(),
        vec!["snapshot", "-i"]
    );
    assert_eq!(tokenize_command("open").unwrap(), vec!["open"]);
}

#[test]
fn tokenize_preserves_quoted_spaces() {
    // 核心场景：fill 的文本参数含空格，必须作为一个 token
    assert_eq!(
        tokenize_command("fill @e3 \"hello world\"").unwrap(),
        vec!["fill", "@e3", "hello world"]
    );
    assert_eq!(
        tokenize_command("fill @e3 'hello world'").unwrap(),
        vec!["fill", "@e3", "hello world"]
    );
    // 空引号是合法 token（清空输入框）
    assert_eq!(
        tokenize_command("fill @e3 \"\"").unwrap(),
        vec!["fill", "@e3", ""]
    );
}

#[test]
fn tokenize_handles_escapes_and_rejects_unclosed_quote() {
    // 反斜杠转义空格
    assert_eq!(
        tokenize_command("type @e1 hello\\ world").unwrap(),
        vec!["type", "@e1", "hello world"]
    );
    // 双引号内转义
    assert_eq!(
        tokenize_command("fill @e1 \"say \\\"hi\\\"\"").unwrap(),
        vec!["fill", "@e1", "say \"hi\""]
    );
    // 未闭合引号 / 末尾反斜杠 → 错误
    assert!(tokenize_command("fill @e1 \"unterminated").is_err());
    assert!(tokenize_command("fill @e1 tail\\").is_err());
}

#[tokio::test]
async fn call_passes_skills_args_through_without_bulk_rejection() {
    // 不再有 skills 参数硬校验：--all / --full / path 等 CLI 原生形态一律透传
    // （本机 CLI 状态决定实际结果，但不应再出现旧的「skills 参数被拒绝」拦截）
    let tool = BrowserCoreTool {
        po: BrowserToolFactory.create_po(),
    };
    let result = tool
        .call(test_ctx(), json!({ "command": "skills get core" }))
        .await
        .unwrap();
    assert!(
        !result["error"]
            .as_str()
            .unwrap_or_default()
            .contains("skills 参数被拒绝"),
        "不应再拦截 skills 参数"
    );
}

#[test]
fn screenshot_path_arg_skips_flag_values() {
    let value_flags = [
        "--threshold",
        "--screenshot-dir",
        "--screenshot-format",
        "--screenshot-quality",
        "--hide-scrollbars",
    ];
    let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();

    // 带值选项的数值不能被误当 PATH（真 bug 回归）
    assert_eq!(
        screenshot_path_arg(&args(&["--threshold", "0.01", "shot.png"]), &value_flags),
        Some("shot.png".to_string())
    );
    assert_eq!(
        screenshot_path_arg(&args(&["--screenshot-quality", "80"]), &value_flags),
        None,
        "只有带值选项、无位置参数时应返回 None 让调用方注入临时路径"
    );
    // --key=value 内嵌值不占后续 token
    assert_eq!(
        screenshot_path_arg(&args(&["--threshold=0.01", "shot.png"]), &value_flags),
        Some("shot.png".to_string())
    );
    // `--` 边界：其后第一个 token 强制是位置参数，即便长得像选项
    assert_eq!(
        screenshot_path_arg(&args(&["--", "--draft.png"]), &value_flags),
        Some("--draft.png".to_string())
    );
    // 普通位置参数
    assert_eq!(
        screenshot_path_arg(&args(&["shot.png", "--full"]), &value_flags),
        Some("shot.png".to_string())
    );
}

#[tokio::test]
async fn session_id_disambiguates_all_symbol_owners() {
    // 纯标点 owner 折叠后全是 `_`，必须靠稳定哈希区分，否则不同 Agent 串会话
    let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    let storage = crate::pkg::storage::test_support::create_test_storage(pool);
    let key_of = |owner: &str| {
        let ctx = RequestContext::builder()
            .agent_id(owner.to_string())
            .storage(storage.clone())
            .build();
        session_id(&ctx)
    };
    let a = key_of("!!!");
    let b = key_of("???");
    assert_ne!(a, b, "不同纯符号 owner 不能折叠成同一 session key");
    assert!(a.starts_with("ai_orz_agent___"), "key 前缀仍合法: {a}");
    assert_eq!(key_of("!!!"), a, "同一 owner 必须得到稳定 key");

    // 正常 owner（含字母数字）不加哈希后缀，保持简洁格式
    assert_eq!(key_of("agent-9"), "ai_orz_agent_agent_9");
}
