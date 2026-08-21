//! Integration tests for browser tool & tool readiness system.
//!
//! Covers：
//! - 内置同步：browser 随 init_base_data 入库，tags 含 browser + network（network tag 复用）
//! - 清单级就绪标志（第①层）：runtime_ready 与 agent-browser 可寻址性一致
//!   （Ready / NotReady{cli_not_installed + 安装引导}）
//! - debug-call 白名单拒绝：eval / cookies / close --all / screenshot 位置参数
//!   （校验先于 CLI 预检，与是否安装 agent-browser 无关，断言稳定）
//! - CLI 未安装统一引导：不可寻址环境 open 调用返回 cli_not_installed 结构化输出；
//!   已安装环境自动跳过（避免真实浏览器/网络副作用）
//!
//! 路由：`/api/v1/finance/tools/`（见 `src/router.rs::finance_routes`）

#[path = "../common/mod.rs"]
mod common;

use ai_orz::pkg::RequestContext;
use ai_orz::pkg::tool_registry::tool_readiness::command_available;
use serde_json::{Value, json};
use sqlx::SqlitePool;

/// 从 DB 读 browser 工具 PO config 的 CLI 命令（D28 不变式：CLI 型 = po.config.command）
///
/// 集成测试数据在全局 Storage 单例（init_full_test_env 初始化），
/// 不在 `#[sqlx::test]` 注入的独立 pool —— 经返回的 ctx 取真实 pool。
async fn browser_command(ctx: &RequestContext) -> String {
    let config: Value = sqlx::query_scalar("SELECT config FROM tools WHERE id = 'browser'")
        .fetch_one(ctx.db_pool())
        .await
        .expect("browser tool config in DB");
    config
        .get("command")
        .and_then(|v| v.as_str())
        .expect("config.command field")
        .to_string()
}

/// 从 query 接口提取 browser 工具条目（init_base_data 已同步内置工具入 DB）
async fn fetch_browser_tool(app: &crate::common::TestApp, jwt: &str) -> Value {
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/finance/tools/query",
            &json!({ "ids": ["browser"], "pagination": { "limit": 50, "offset": 0 } }),
            jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    data.get("items")
        .and_then(|v| v.as_array())
        .expect("items array")
        .iter()
        .find(|t| t.get("id").and_then(|v| v.as_str()) == Some("browser"))
        .cloned()
        .unwrap_or_else(|| panic!("browser tool should be listed: {}", body))
}

/// debug-call browser 工具，返回工具业务输出（data.result）
///
/// 注意：path+body 混合提取约定，body 内亦需携带 `id` 字段（会被 path 值覆盖）
async fn debug_call(app: &crate::common::TestApp, jwt: &str, args: Value) -> Value {
    let (status, body) = app
        .post_with_jwt(
            "/api/v1/finance/tools/browser/debug-call",
            &json!({ "id": "browser", "args": args }),
            jwt,
        )
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    data.get("result")
        .cloned()
        .expect("debug-call result field")
}

/// browser 内置工具同步入库 + tags（browser/network）+ 清单级就绪标志
#[sqlx::test]
async fn test_browser_tool_listed_with_readiness(pool: SqlitePool) {
    let ctx = crate::common::init_full_test_env(pool).await;
    let app = crate::common::TestApp::new(ctx.db_pool().clone()).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    let tool = fetch_browser_tool(&app, &jwt).await;

    // tags：browser + network（network 与 tavily_search 共用，浏览器工具复用该 tag）
    let tags: Vec<&str> = tool
        .get("tags")
        .and_then(|v| v.as_array())
        .expect("tags array")
        .iter()
        .filter_map(|t| t.as_str())
        .collect();
    assert!(
        tags.contains(&"browser"),
        "tags should contain browser: {:?}",
        tags
    );
    assert!(
        tags.contains(&"network"),
        "tags should contain network: {:?}",
        tags
    );

    // protocol=Builtin / control_mode=Manual（浏览器有真实网络副作用，需人工确认）
    assert_eq!(
        tool.get("protocol").and_then(|v| v.as_str()),
        Some("Builtin")
    );
    assert_eq!(
        tool.get("control_mode").and_then(|v| v.as_str()),
        Some("Manual")
    );

    // runtime_ready 与 CLI 可寻址性一致（第①层清单级标志，命令取自 PO config）
    let cli_available = command_available(&browser_command(&ctx).await);
    let ready = tool.get("runtime_ready").expect("runtime_ready field");
    let status_str = ready
        .get("status")
        .and_then(|v| v.as_str())
        .expect("status tag");
    if cli_available {
        assert_eq!(
            status_str, "ready",
            "installed agent-browser should be ready"
        );
    } else {
        assert_eq!(status_str, "not_ready");
        assert_eq!(
            ready.get("reason").and_then(|v| v.as_str()),
            Some("cli_not_installed")
        );
        let hint = ready.get("hint").and_then(|v| v.as_str()).unwrap_or("");
        assert!(
            hint.contains("agent-browser"),
            "hint should guide install: {}",
            hint
        );
    }
}

/// 白名单拒绝：脚本执行类（eval）/ 状态泄露类（cookies/storage）/
/// close --all / screenshot 位置参数——校验先于 CLI 预检，不依赖本机安装状态
#[sqlx::test]
async fn test_browser_debug_call_whitelist_rejection(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = crate::common::TestApp::new(pool).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 脚本执行类（eval）
    let result = debug_call(
        &app,
        &jwt,
        json!({ "command": "eval", "args": ["alert(1)"] }),
    )
    .await;
    assert_eq!(result["success"], false);
    let error = result["error"].as_str().unwrap();
    assert!(
        error.contains("eval") && error.contains("白名单"),
        "eval rejection: {}",
        error
    );

    // 状态泄露类（cookies / storage）
    for leak in ["cookies", "storage"] {
        let result = debug_call(&app, &jwt, json!({ "command": leak })).await;
        assert_eq!(result["success"], false, "{} should be rejected", leak);
        assert!(
            result["error"].as_str().unwrap().contains("白名单"),
            "{} rejection: {}",
            leak,
            result["error"]
        );
    }

    // close --all 会波及其他 Agent 会话
    let result = debug_call(&app, &jwt, json!({ "command": "close", "args": ["--all"] })).await;
    assert_eq!(result["success"], false);
    assert!(result["error"].as_str().unwrap().contains("--all"));

    // screenshot 位置参数（产物路径由系统管理，防 Agent 指定任意写入路径）
    let result = debug_call(
        &app,
        &jwt,
        json!({ "command": "screenshot", "args": ["/etc/evil.png"] }),
    )
    .await;
    assert_eq!(result["success"], false);
    assert!(result["error"].as_str().unwrap().contains("路径"));
}

/// CLI 未安装统一引导：合法子命令在不可寻址环境返回 cli_not_installed
/// 结构化输出（install/hint 双通道）；agent-browser 已安装的环境跳过
/// （真实调用会产生浏览器/网络副作用）
#[sqlx::test]
async fn test_browser_debug_call_cli_not_installed_guidance(pool: SqlitePool) {
    let ctx = crate::common::init_full_test_env(pool).await;
    let app = crate::common::TestApp::new(ctx.db_pool().clone()).await;
    let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    if command_available(&browser_command(&ctx).await) {
        // 开发机已安装 agent-browser：跳过，避免真实浏览器/网络副作用
        return;
    }

    let result = debug_call(
        &app,
        &jwt,
        json!({ "command": "open", "args": ["https://example.com"] }),
    )
    .await;
    assert_eq!(result["success"], false);
    assert_eq!(result["error_code"], "cli_not_installed");
    assert!(result["error"].as_str().unwrap().contains("agent-browser"));
    let install = result["install"].as_str().unwrap();
    assert!(
        install.contains("agent-browser"),
        "install guidance should mention agent-browser: {}",
        install
    );
    let hint = result["hint"].as_str().unwrap_or("");
    assert!(
        hint.contains("工具配置"),
        "hint should mention tool config: {}",
        hint
    );
}
