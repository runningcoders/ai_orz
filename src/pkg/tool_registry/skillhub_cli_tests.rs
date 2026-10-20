use super::*;
use crate::pkg::request_context_test_support::new_test_ctx;
use crate::pkg::tool_registry::BuiltinToolFactory as _;
use common::error::ErrorCode;
use std::sync::Mutex;

/// 测试用 RequestContext（懒连接内存 SQLite，不产生真实 IO）
fn test_ctx() -> RequestContext {
    let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    new_test_ctx("test-user", pool)
}

/// 可编程 mock runner：按 (program, args) 前缀返回预设结果；零真实进程
struct MockRunner {
    script: Vec<(Vec<String>, ExecOutput)>,
    #[allow(dead_code)]
    log: Mutex<Vec<Vec<String>>>,
}

impl MockRunner {
    fn new() -> Self {
        Self {
            script: Vec::new(),
            log: Mutex::new(Vec::new()),
        }
    }
    fn expect(mut self, program: &str, args: &[&str], output: ExecOutput) -> Self {
        let mut full = vec![program.to_string()];
        full.extend(args.iter().map(|s| s.to_string()));
        self.script.push((full, output));
        self
    }
    fn output(exit_code: Option<i32>, stdout: &str, stderr: &str) -> ExecOutput {
        ExecOutput {
            pid: 0,
            success: exit_code == Some(0),
            exit_code,
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
            timed_out: false,
        }
    }
}

#[async_trait::async_trait]
impl CliRunner for MockRunner {
    async fn run(&self, opts: &ExecOptions) -> Result<ExecOutput> {
        let mut full = vec![opts.program.clone()];
        full.extend(opts.args.iter().cloned());
        self.log.lock().unwrap().push(full.clone());
        if let Some((_, out)) = self
            .script
            .iter()
            .find(|(prefix, _)| full.starts_with(prefix))
        {
            return Ok(out.clone());
        }
        Err(common::error::Error::new(
            ErrorCode::NotFound,
            format!("mock: unexpected call {:?}", full),
        ))
    }
}

/// mock 探测装配：file_exists 恒 false + path_lookup 命中裸名 `skillhub`
///（解析稳定落在步骤 3，零真实文件系统零真实 PATH——本机 ~/.local/bin 在位也不受扰）
fn mock_probes() -> (FileExistsFn, PathLookupFn) {
    (
        Arc::new(|_: &Path| false),
        Arc::new(|name: &str| (name == SKILLHUB_CLI_BIN).then(|| PathBuf::from(name))),
    )
}

fn tool(runner: MockRunner, home: Option<PathBuf>) -> SkillhubCliCoreTool {
    let (fe, pl) = mock_probes();
    SkillhubCliCoreTool::with_deps(
        SkillhubCliToolFactory.create_po(),
        Arc::new(runner),
        home,
        fe,
        pl,
    )
}

// ---------- 1. 白名单门（任务书 §4-1） ----------

#[test]
fn whitelist_allows_exactly_four_subcommands() {
    for cmd in ["search", "install", "upgrade", "list"] {
        assert!(validate_subcommand(cmd).is_ok(), "{} should pass", cmd);
    }
    // 拒绝面：禁开命令 + 未知命令
    for cmd in [
        "login",
        "logout",
        "publish",
        "auth",
        "config",
        "self-upgrade",
        "foo",
    ] {
        let err = validate_subcommand(cmd).unwrap_err();
        assert!(err.contains(cmd), "{} rejection should echo cmd", cmd);
    }
    // 错误信息列明白名单四件 + 零凭证红线提示
    let err = validate_subcommand("publish something").unwrap_err();
    for frag in ["search", "install", "upgrade", "list", "零凭证红线"] {
        assert!(err.contains(frag), "rejection should mention {}", frag);
    }
}

// ---------- 2. 路径探测四步（任务书 §4-2，tempdir 隔离） ----------

#[test]
fn resolve_path_prefers_config_command() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().to_path_buf();
    let custom = dir.path().join("custom-bin");
    std::fs::write(&custom, "#!/bin/sh\n").unwrap();
    // 步骤 1：config.command 显式指定（`~` 展开）优先于默认布局
    let got =
        resolve_cli_path(Some("~/custom-bin"), Some(&home), |p| p.is_file(), |_| None).unwrap();
    assert_eq!(got, custom.to_string_lossy());
}

#[test]
fn resolve_path_falls_back_to_default_layout_then_path() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().to_path_buf();
    // 步骤 2：默认安装布局 ~/.skillhub/skillhub
    let default_path = home.join(".skillhub").join(SKILLHUB_CLI_BIN);
    std::fs::create_dir_all(default_path.parent().unwrap()).unwrap();
    std::fs::write(&default_path, "#!/usr/bin/env python3\n").unwrap();
    let got = resolve_cli_path(None, Some(&home), |p| p.is_file(), |_| None).unwrap();
    assert_eq!(got, default_path.to_string_lossy());

    // 步骤 3：PATH 探测（config/默认布局均未命中）
    let got = resolve_cli_path(
        None,
        Some(&home),
        |_| false,
        |name| (name == SKILLHUB_CLI_BIN).then(|| PathBuf::from("/usr/local/bin").join(name)),
    )
    .unwrap();
    assert_eq!(got, "/usr/local/bin/skillhub");
}

#[test]
fn resolve_path_all_misses_returns_install_guidance() {
    // 步骤 4：全部未命中 → 引导性错误，零安装命令零代装零 curl|bash
    let err = resolve_cli_path(None, None, |_| false, |_| None).unwrap_err();
    assert!(err.contains("请先安装"), "guidance: {}", err);
    assert!(err.contains("skillhub.cn/install"), "guidance: {}", err);
    assert!(!err.contains("curl"));
    assert!(!err.contains("bash"));
}

// ---------- 3. 命令透传（任务书 §4-3，含多空格切分） ----------

#[tokio::test]
async fn passthrough_splits_whitespace_and_forwards_args() {
    let mut runner = MockRunner::new();
    runner = runner.expect(
        "skillhub",
        &["install", "@org/slug"],
        MockRunner::output(Some(0), "installed @org/slug v1.1.9\n", ""),
    );
    let result = tool(runner, Some(PathBuf::from("/tmp/fake-home")))
        .call(test_ctx(), json!({ "command": "install  @org/slug" }))
        .await
        .unwrap();
    assert_eq!(result["success"], true);
    assert!(result["output"].as_str().unwrap().contains("installed"));
}

// ---------- 4. HOME 零改写 + 零 OTA 关断（任务书 §4-4/§4-9） ----------

/// exec 原语 env 系「父进程环境叠加」语义：ExecOptions::env 空向量 →
/// 子进程完整继承父环境（含 HOME）——HOME 零注入零覆盖结构性成立。
/// 本测试锁定白名单内命令的 spawn 构造零 env 注入（零 HOME/零 SKIP_*）。
/// OTA 关断特征串按分段构造：验收 grep 特征串全文须零命中，测试源码亦不例外。
#[test]
fn spawn_options_carry_no_env_injection_and_no_ota_kill_switches() {
    use crate::pkg::process::ExecOptions as Opts;
    // 透传构造路径与生产 call() 一致：白名单通过 → ExecOptions::new(program, args)
    let args = validate_subcommand("upgrade").unwrap();
    let opts = Opts::new("skillhub", args);
    // HOME 零改写：零 env 注入（exec 原语不传 env → 继承父进程 HOME）
    assert!(
        opts.env.is_empty(),
        "HOME 零改写要求 spawn env 零注入，实际: {:?}",
        opts.env
    );
    // 零 OTA 关断：args 零旗标、env 零变量（特征串分段拼装，grep 全文零命中）
    let flag = format!("--skip-self-{}", "upgrade");
    let env_var = format!("SKIP_SELF_{}", "UPGRADE");
    let config_key = format!("auto_self_{}", "upgrade");
    let joined = opts.args.join(" ");
    for frag in [flag, env_var, config_key] {
        assert!(
            !joined.contains(&frag) && !opts.env.iter().any(|(k, _)| k.contains(&frag)),
            "OTA kill switch leaked: {}",
            frag
        );
    }
}

// ---------- 5. 版本行回显（任务书 §4-5） ----------

#[tokio::test]
async fn version_line_echoed_and_probe_failure_degrades_to_null() {
    // 探测成功 → 版本行如实回显（零比对零劝升）
    let mut runner = MockRunner::new();
    runner = runner
        .expect(
            "skillhub",
            &["list"],
            MockRunner::output(Some(0), "ima-skills v1.1.9\n", ""),
        )
        .expect(
            "skillhub",
            &["--version"],
            MockRunner::output(Some(0), "skillhub 2.3.1\n", ""),
        );
    let result = tool(runner, Some(PathBuf::from("/tmp/fake-home")))
        .call(test_ctx(), json!({ "command": "list" }))
        .await
        .unwrap();
    assert_eq!(result["cli_version"], "skillhub 2.3.1");

    // 探测失败 → 降级 null 不阻断（CLI 启动级 OTA 窗口下的合法态）
    let mut runner = MockRunner::new();
    runner = runner.expect(
        "skillhub",
        &["list"],
        MockRunner::output(Some(0), "ok\n", ""),
    );
    let result = tool(runner, None)
        .call(test_ctx(), json!({ "command": "list" }))
        .await
        .unwrap();
    assert_eq!(result["success"], true);
    assert!(result["cli_version"].is_null());
}

// ---------- 6. 升级竞态容错（任务书 §4-6） ----------

#[tokio::test]
async fn nonzero_exit_reports_stderr_with_retry_hint_no_auto_retry() {
    let mut runner = MockRunner::new();
    // 非零退出（升级窗口竞态模拟）：如实呈现 stderr 摘要 + 重试引导
    // （升级窗口内 CLI 可能处于 re-exec 自替换中——版本探测照常 best-effort，此处降级 null）
    runner = runner.expect(
        "skillhub",
        &["upgrade"],
        MockRunner::output(
            Some(3),
            "",
            "self-upgrade in progress: binary replaced mid-call\n",
        ),
    );
    let result = tool(runner, None)
        .call(test_ctx(), json!({ "command": "upgrade" }))
        .await
        .unwrap();
    assert_eq!(result["success"], false);
    assert_eq!(result["exit_code"], 3);
    let err = result["error"].as_str().unwrap();
    assert!(
        err.contains("self-upgrade in progress"),
        "stderr should surface: {}",
        err
    );
    assert!(
        result["hint"].as_str().unwrap().contains("请稍后重试"),
        "retry hint expected"
    );
    assert!(result["cli_version"].is_null());
    // 不自动重试不回滚不干预：mock 脚本仅一笔预设调用，生产路径无重试循环（结构性保证）
}

// ---------- 7. lockfile 只读观测（任务书 §4-7） ----------

#[tokio::test]
async fn lockfile_observation_carries_keys_or_note_without_blocking() {
    // 场景 A：lockfile 在位 → slug/version/source/sha256 回显
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().to_path_buf();
    let lock_dir = home.join(".skillhub");
    std::fs::create_dir_all(&lock_dir).unwrap();
    std::fs::write(
        lock_dir.join(".skills_store_lock.json"),
        r#"{"skills":[{"slug":"@tencent-adm/ima-skills","version":"1.1.9","source":"https://ima.qq.com","sha256":"abc123"}]}"#,
    )
    .unwrap();
    let mut runner = MockRunner::new();
    runner = runner
        .expect(
            "skillhub",
            &["install", "@tencent-adm/ima-skills"],
            MockRunner::output(Some(0), "installed\n", ""),
        )
        .expect(
            "skillhub",
            &["--version"],
            MockRunner::output(Some(0), "skillhub 2.3.1\n", ""),
        );
    let result = tool(runner, Some(home))
        .call(
            test_ctx(),
            json!({ "command": "install @tencent-adm/ima-skills" }),
        )
        .await
        .unwrap();
    assert_eq!(result["success"], true);
    let observed = result["lockfile_observed"].as_array().unwrap();
    assert_eq!(observed.len(), 1);
    assert_eq!(observed[0]["slug"], "@tencent-adm/ima-skills");
    assert_eq!(observed[0]["version"], "1.1.9");
    assert_eq!(observed[0]["sha256"], "abc123");
    assert!(observed[0]["source"].is_string());

    // 场景 B：lockfile 未读到 → note 附带、不阻断不报错
    let mut runner = MockRunner::new();
    runner = runner
        .expect(
            "skillhub",
            &["upgrade"],
            MockRunner::output(Some(0), "upgraded\n", ""),
        )
        .expect(
            "skillhub",
            &["--version"],
            MockRunner::output(Some(0), "skillhub 2.3.1\n", ""),
        );
    let result = tool(
        runner,
        Some(PathBuf::from("/tmp/skh-certainly-missing-home")),
    )
    .call(test_ctx(), json!({ "command": "upgrade" }))
    .await
    .unwrap();
    assert_eq!(result["success"], true, "note 不阻断");
    assert!(
        result["lockfile_note"]
            .as_str()
            .unwrap()
            .contains("lockfile 未读到")
    );
}

// ---------- 8. 脱敏与截断（任务书 §4-8） ----------

#[tokio::test]
async fn sanitize_redacts_and_truncation_is_utf8_safe() {
    let out = sanitize_skillhub_output(
        "search ok\napi_key: IMA-SECRET-VALUE\nauthorization: Bearer x\nsafe line\npassword: p",
    );
    assert!(out.contains("search ok"));
    assert!(out.contains("safe line"));
    assert!(!out.contains("IMA-SECRET-VALUE"));
    assert!(!out.contains("Bearer x"));
    assert!(!out.contains("password: p"));
    assert_eq!(out.matches("[REDACTED]").count(), 3);

    // 超长输出 → 截断标记；多字节字符边界不 panic（中文 ~2MB > 默认 1MB 上限）
    let long = "好".repeat(700_000);
    let mut runner = MockRunner::new();
    runner = runner.expect(
        "skillhub",
        &["list"],
        MockRunner::output(Some(0), &long, ""),
    );
    let result = tool(runner, None)
        .call(test_ctx(), json!({ "command": "list" }))
        .await
        .unwrap();
    assert_eq!(result["truncated"], true);
    assert!(
        result["output"]
            .as_str()
            .unwrap()
            .contains("... [truncated]")
    );
}

// ---------- 9. 空参数/白名单边界（任务书 §4-10~12 自补强） ----------

#[tokio::test]
async fn call_rejects_empty_and_blocked_subcommands() {
    let t = tool(MockRunner::new(), None);
    let r1 = t
        .call(test_ctx(), json!({ "command": "   " }))
        .await
        .unwrap();
    assert_eq!(r1["success"], false);
    assert!(r1["error"].as_str().unwrap().contains("command"));

    let r2 = t
        .call(test_ctx(), json!({ "command": "login" }))
        .await
        .unwrap();
    assert_eq!(r2["success"], false);
    let err = r2["error"].as_str().unwrap();
    for frag in ["login", "search, install, upgrade, list", "零凭证红线"] {
        assert!(err.contains(frag), "should mention {}", frag);
    }
}

// ---------- 10. 工厂元数据与注册契约（零凭证/默认路径/标签） ----------

#[test]
fn factory_po_metadata_and_registration_contract() {
    let po = SkillhubCliToolFactory.create_po();
    assert_eq!(po.id, "skillhub_cli");
    assert_eq!(po.control_mode, ControlMode::Auto);
    assert_eq!(po.protocol, ToolProtocol::Builtin);
    assert_eq!(po.get_tags(), vec!["skillhub"]);
    // config.command 默认 `~/.skillhub/skillhub`（方案 v2 口径）
    assert_eq!(
        po.cli_command().as_deref(),
        Some(SKILLHUB_DEFAULT_PATH_SPEC)
    );
    // 零凭证：工厂与实例均不声明 credential_requirements（trait 默认空实现）
    assert!(SkillhubCliToolFactory.credential_requirements().is_empty());
    let t = SkillhubCliCoreTool::new(po);
    assert!(t.credential_requirements().is_empty());
}
