//! 飞书集成会话编排（pkg 层）
//!
//! 封装 lark-cli 子进程的用户级集成能力：
//! - 用户 OAuth device flow（`auth login --no-wait/--device-code`）
//! - 用户授权状态查询（`auth status --json`）与取消授权（`auth logout`）
//! - config init --new 自动化绑定会话（bind session，内存注册表）
//!
//! 复用一期 HOME 隔离设施（`tool_registry::lark_cli` 的 `lark_home`
//! /`ensure_cli_config`/`sanitize_lark_output`）与 tool_readiness 的
//! `command_available` 二进制探测。
//!
//! # 约定
//!
//! - 无绑定凭证（HOME 下无 lark-cli config）→ 引导性错误（InvalidRequest）
//! - keychain 不可用 → 降级返回（degraded=true + hint），不抛 500
//! - 输出经 `sanitize_lark_output` 脱敏；JSON 解析逻辑为纯函数（fixture 可测）

use crate::pkg::process::{ExecOptions, exec};
use crate::pkg::tool_registry::lark_cli::{LARK_CLI_BIN, cli_env, lark_home, sanitize_lark_output};
use crate::pkg::tool_registry::tool_readiness::command_available;
use anyhow::anyhow;
use common::error::{Result, err};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::sync::{Mutex, RwLock};

/// device flow 授权命令超时（--device-code 会轮询等待用户完成授权）
const AUTH_COMMAND_TIMEOUT: Duration = Duration::from_secs(300);
/// 轻量命令超时（status/logout/--no-wait 发起）
const LIGHT_COMMAND_TIMEOUT: Duration = Duration::from_secs(60);

// ==================== 结果结构 ====================

/// device flow 发起结果（`auth login --no-wait --json`）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceLoginStart {
    /// 设备码（后续 `--device-code` 完成授权用）
    pub device_code: String,
    /// 用户浏览器验证 URL
    pub verification_url: String,
    /// 设备码有效期（秒，CLI 未给出时为 None）
    pub expires_in: Option<u64>,
}

/// 用户授权状态（`auth status --json`）
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LarkAuthStatus {
    /// 用户身份是否已授权
    pub logged_in: bool,
    /// 已授权用户名（未登录为 None）
    pub user_name: Option<String>,
    /// 是否降级（keychain 不可用等）
    pub degraded: bool,
    /// 降级/引导提示
    pub hint: Option<String>,
}

/// auth 操作结果（complete/logout 共用）
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LarkAuthOutcome {
    pub success: bool,
    pub degraded: bool,
    pub hint: Option<String>,
}

// ==================== 前置检查 ====================

/// 解析用户 lark-cli HOME 并校验前置条件（二进制可用 + 已绑定应用 config）
fn prepare_lark_home(user_id: &str) -> Result<std::path::PathBuf> {
    if !command_available(LARK_CLI_BIN) {
        return Err(err!(
            InvalidRequest,
            "未找到 lark-cli 二进制，请先安装：https://github.com/larksuite/lark-cli"
        ));
    }
    let home = lark_home(&crate::config::get().base_data_path(), user_id);
    if !home.join(".lark-cli").join("config.json").exists() {
        return Err(err!(
            InvalidRequest,
            "尚未绑定飞书应用，请先在飞书集成中绑定应用凭证"
        ));
    }
    Ok(home)
}

/// 执行 lark-cli 子命令（HOME 隔离 + 稳定 JSON 环境变量），返回 (exit_success, stdout, stderr)
async fn run_cli(
    home_dir: &Path,
    args: &[&str],
    timeout: Duration,
) -> Result<(bool, String, String)> {
    let output = exec(
        &ExecOptions::new(LARK_CLI_BIN, args.iter().map(|s| s.to_string()).collect())
            .envs(cli_env(home_dir))
            .timeout(timeout),
    )
    .await?;
    // 语义保持：超时是执行失败而非「CLI 报告失败」（stderr 为空会让调用方丢失原因）
    if output.timed_out {
        return Err(err!(
            Internal,
            "lark-cli timed out after {}s and was killed",
            timeout.as_secs()
        ));
    }
    Ok((
        output.success,
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    ))
}

// ==================== JSON 解析（纯函数） ====================

/// lark-cli 错误信封（stderr，退出码非 0）
#[derive(Debug, Deserialize)]
struct CliErrorEnvelope {
    #[serde(default)]
    error: Option<CliErrorDetail>,
}

#[derive(Debug, Deserialize)]
struct CliErrorDetail {
    #[serde(default)]
    message: String,
    #[serde(default)]
    hint: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    subtype: Option<String>,
}

/// keychain 类错误识别（降级而非 500）
pub fn detect_keychain_degradation(stderr: &str) -> Option<String> {
    let lower = stderr.to_lowercase();
    if lower.contains("keychain") || stderr.contains("钥匙串") {
        return Some(
            "钥匙串不可用，用户授权 token 无法安全存储；可继续使用应用身份（bot）操作飞书"
                .to_string(),
        );
    }
    None
}

/// 解析 `auth login --no-wait --json` 成功输出（stdout 成功信封）
pub fn parse_device_login_json(stdout: &str) -> Result<DeviceLoginStart> {
    let value: serde_json::Value = serde_json::from_str(stdout)
        .map_err(|e| anyhow!("lark-cli auth login output is not valid JSON: {}", e))?;
    // 成功信封字段可能在顶层或 data 内
    let lookup = |keys: &[&str]| -> Option<String> {
        for container in [
            &value,
            value.get("data").unwrap_or(&serde_json::Value::Null),
        ] {
            for key in keys {
                if let Some(v) = container.get(key).and_then(|v| v.as_str())
                    && !v.is_empty()
                {
                    return Some(v.to_string());
                }
            }
        }
        None
    };
    let device_code = lookup(&["device_code"])
        .ok_or_else(|| anyhow!("lark-cli auth login output missing device_code"))?;
    let verification_url = lookup(&["verification_url", "verification_uri_complete", "url"])
        .ok_or_else(|| anyhow!("lark-cli auth login output missing verification_url"))?;
    let expires_in = [
        &value,
        value.get("data").unwrap_or(&serde_json::Value::Null),
    ]
    .iter()
    .find_map(|c| c.get("expires_in").and_then(|v| v.as_u64()));
    Ok(DeviceLoginStart {
        device_code,
        verification_url,
        expires_in,
    })
}

/// 解析 `auth status --json` 输出（成功信封；logged_in 依据 identities.user 状态）
pub fn parse_auth_status_json(stdout: &str) -> LarkAuthStatus {
    let value: serde_json::Value = match serde_json::from_str(stdout) {
        Ok(v) => v,
        Err(_) => return LarkAuthStatus::default(),
    };
    if value.get("ok").and_then(|v| v.as_bool()) == Some(false) {
        return LarkAuthStatus::default();
    }
    // identities.user.userName / identities.user.tokenStatus
    let user = value.pointer("/identities/user");
    let user_name = user
        .and_then(|u| u.get("userName"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let token_status = user
        .and_then(|u| u.get("tokenStatus"))
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    // verified 字段优先；否则以 userName 存在视为已登录
    let logged_in = value
        .get("verified")
        .and_then(|v| v.as_bool())
        .unwrap_or_else(|| {
            !user_name.is_none() && !matches!(token_status, "" | "expired" | "revoked" | "invalid")
        });
    LarkAuthStatus {
        logged_in,
        user_name,
        degraded: false,
        hint: None,
    }
}

/// 解析 `auth logout --json` 输出
pub fn parse_logout_json(stdout: &str) -> bool {
    let value: serde_json::Value = match serde_json::from_str(stdout) {
        Ok(v) => v,
        Err(_) => return false,
    };
    value
        .get("loggedOut")
        .and_then(|v| v.as_bool())
        .unwrap_or_else(|| value.get("ok").and_then(|v| v.as_bool()).unwrap_or(false))
}

/// 从错误信封提取引导信息（脱敏后）
fn extract_error_hint(stderr: &str) -> String {
    if let Ok(envelope) = serde_json::from_str::<CliErrorEnvelope>(stderr)
        && let Some(detail) = envelope.error
    {
        let mut msg = detail.message;
        if let Some(hint) = detail.hint.filter(|h| !h.is_empty()) {
            msg = format!("{}（{}）", msg, hint);
        }
        if !msg.is_empty() {
            return sanitize_lark_output(&msg);
        }
    }
    sanitize_lark_output(&stderr.lines().take(3).collect::<Vec<_>>().join("\n"))
}

// ==================== 运行时编排 ====================

/// 发起 device flow 用户授权
///
/// `domains` 为空时请求全部业务域（`--domain all`）。
pub async fn start_device_login(user_id: &str, domains: &[String]) -> Result<DeviceLoginStart> {
    let home = prepare_lark_home(user_id)?;
    let mut args: Vec<&str> = vec!["auth", "login", "--no-wait", "--json"];
    if domains.is_empty() {
        args.extend(["--domain", "all"]);
    } else {
        for d in domains {
            args.extend(["--domain", d.as_str()]);
        }
    }
    let (success, stdout, stderr) = run_cli(&home, &args, LIGHT_COMMAND_TIMEOUT).await?;
    if !success {
        if let Some(hint) = detect_keychain_degradation(&stderr) {
            return Err(err!(InvalidRequest, "{}", hint));
        }
        return Err(err!(
            ThirdPartyError,
            "飞书授权发起失败: {}",
            extract_error_hint(&stderr)
        ));
    }
    parse_device_login_json(&stdout).map_err(|e| err!(ThirdPartyError, "{}", e))
}

/// 以 device_code 完成授权（CLI 内部轮询直到用户完成或过期）
pub async fn complete_device_login(user_id: &str, device_code: &str) -> Result<LarkAuthOutcome> {
    let home = prepare_lark_home(user_id)?;
    let args = ["auth", "login", "--device-code", device_code, "--json"];
    let (success, stdout, stderr) = run_cli(&home, &args, AUTH_COMMAND_TIMEOUT).await?;
    if !success {
        if let Some(hint) = detect_keychain_degradation(&stderr) {
            return Ok(LarkAuthOutcome {
                success: false,
                degraded: true,
                hint: Some(hint),
            });
        }
        return Ok(LarkAuthOutcome {
            success: false,
            degraded: false,
            hint: Some(extract_error_hint(&stderr)),
        });
    }
    // 成功后输出可能含 token 类字段，不解析正文，仅标记成功
    let _ = stdout;
    Ok(LarkAuthOutcome {
        success: true,
        degraded: false,
        hint: None,
    })
}

/// 查询用户授权状态
pub async fn auth_status(user_id: &str) -> Result<LarkAuthStatus> {
    let home = prepare_lark_home(user_id)?;
    let args = ["auth", "status", "--json"];
    let (success, stdout, stderr) = run_cli(&home, &args, LIGHT_COMMAND_TIMEOUT).await?;
    if !success {
        if let Some(hint) = detect_keychain_degradation(&stderr) {
            return Ok(LarkAuthStatus {
                logged_in: false,
                degraded: true,
                hint: Some(hint),
                ..Default::default()
            });
        }
        // 未登录时 CLI 也返回 ok=false 信封，视为未授权而非错误
        return Ok(LarkAuthStatus {
            hint: Some(extract_error_hint(&stderr)),
            ..Default::default()
        });
    }
    Ok(parse_auth_status_json(&stdout))
}

/// 取消用户授权（清本机登录态）
pub async fn auth_logout(user_id: &str) -> Result<LarkAuthOutcome> {
    let home = prepare_lark_home(user_id)?;
    let args = ["auth", "logout", "--json"];
    let (success, stdout, stderr) = run_cli(&home, &args, LIGHT_COMMAND_TIMEOUT).await?;
    if !success {
        if let Some(hint) = detect_keychain_degradation(&stderr) {
            return Ok(LarkAuthOutcome {
                success: false,
                degraded: true,
                hint: Some(hint),
            });
        }
        return Ok(LarkAuthOutcome {
            success: false,
            hint: Some(extract_error_hint(&stderr)),
            ..Default::default()
        });
    }
    Ok(LarkAuthOutcome {
        success: parse_logout_json(&stdout),
        degraded: false,
        hint: None,
    })
}

// ==================== config init --new 自动绑定会话 ====================
//
// 实测结论（分支 B）：`config init --new` 完成后 app_secret 存于系统 keychain，
// `config show` 脱敏为 ****，secret 不可读出。因此 done 后前端引导用户
// 「去飞书集成补填 App Secret」（手动录入凭证），本模块仅负责进程编排与 URL 抓取。

/// 绑定会话 TTL（惰性清理；仅清理已终态会话）
const BIND_SESSION_TTL: Duration = Duration::from_secs(10 * 60);

/// 绑定会话状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindPhase {
    /// 等待用户在浏览器完成建应用授权
    Pending,
    /// 进程成功退出（secret 不可读，需补填）
    Done,
    /// 进程非零退出或被取消
    Failed,
}

impl BindPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            BindPhase::Pending => "pending",
            BindPhase::Done => "done",
            BindPhase::Failed => "failed",
        }
    }
}

/// 绑定会话共享进度：(verification_url, phase, error)
type BindProgress = Arc<RwLock<(Option<String>, BindPhase, Option<String>)>>;

/// 绑定会话（内存态，完成即消亡）
struct BindSession {
    user_id: String,
    child: Mutex<tokio::process::Child>,
    /// 共享进度：(verification_url, phase, error)
    progress: BindProgress,
    created_at: Instant,
}

type SessionRegistry = RwLock<HashMap<String, Arc<BindSession>>>;

static BIND_SESSIONS: std::sync::OnceLock<SessionRegistry> = std::sync::OnceLock::new();

fn bind_registry() -> &'static SessionRegistry {
    BIND_SESSIONS.get_or_init(|| RwLock::new(HashMap::new()))
}

/// 从 `config init --new` 输出行中提取验证 URL（纯函数，可测）
///
/// 实测输出（stderr）：QR 码块后「打开以下链接配置应用:」紧跟 URL 行。
pub fn extract_verification_url(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let trimmed = line.trim();
        trimmed
            .starts_with("http://")
            .then(|| trimmed.to_string())
            .or_else(|| trimmed.starts_with("https://").then(|| trimmed.to_string()))
    })
}

/// 逐行扫描 `config init --new` 输出流，拼接后提取验证 URL（模块级 fn，供 spawn）
async fn scan_bind_output(
    reader: Box<dyn tokio::io::AsyncRead + Unpin + Send>,
    progress: BindProgress,
    joined: Arc<Mutex<String>>,
) {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if progress.read().await.0.is_some() {
            continue;
        }
        let mut buf = joined.lock().await;
        buf.push_str(&line);
        buf.push('\n');
        if let Some(url) = extract_verification_url(&buf) {
            progress.write().await.0 = Some(url);
        }
    }
}

/// 发起自动绑定会话（spawn `config init --new`，后台扫 URL）
///
/// per-user 同时仅一个活跃会话；已有 pending 会话时返回 Conflict。
/// 返回 `(session_id, verification_url)`；URL 未能在启动窗口内抓到时返回空串，
/// 前端通过 status 轮询补取。
pub async fn start_bind_session(user_id: &str) -> Result<(String, String)> {
    if !command_available(LARK_CLI_BIN) {
        return Err(err!(
            InvalidRequest,
            "未找到 lark-cli 二进制，请先安装：https://github.com/larksuite/lark-cli"
        ));
    }
    let registry = bind_registry();
    // 惰性 TTL 清理（仅终态会话）+ per-user 单活跃会话约束
    {
        let mut sessions = registry.write().await;
        let mut expired = Vec::new();
        for (id, s) in sessions.iter() {
            let phase = s.progress.read().await.1;
            if matches!(phase, BindPhase::Done | BindPhase::Failed)
                && s.created_at.elapsed() > BIND_SESSION_TTL
            {
                expired.push(id.clone());
            }
        }
        for id in expired {
            sessions.remove(&id);
        }
        for s in sessions.values() {
            if s.user_id == user_id && s.progress.read().await.1 == BindPhase::Pending {
                return Err(err!(Conflict, "已有一个进行中的绑定会话，请先完成或取消"));
            }
        }
    }

    let home = lark_home(&crate::config::get().base_data_path(), user_id);
    tokio::fs::create_dir_all(&home).await?;
    let mut command = Command::new(LARK_CLI_BIN);
    command
        .args(["config", "init", "--new"])
        .env("HOME", &home)
        .env("LARKSUITE_CLI_NO_UPDATE_NOTIFIER", "1")
        .env("LARKSUITE_CLI_NO_SKILLS_NOTIFIER", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|e| anyhow!("Failed to spawn lark-cli config init --new: {}", e))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let session_id = uuid::Uuid::now_v7().to_string();
    let progress: BindProgress = Arc::new(RwLock::new((None, BindPhase::Pending, None)));

    // 后台监控任务：并行扫 stdout+stderr 抓验证 URL（终态由 status 查询时 try_wait 检测）
    tokio::spawn({
        let progress = progress.clone();
        async move {
            let joined = Arc::new(Mutex::new(String::new()));
            let mut handles = Vec::new();
            for reader in [
                stdout.map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Unpin + Send>),
                stderr.map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Unpin + Send>),
            ]
            .into_iter()
            .flatten()
            {
                handles.push(tokio::spawn(scan_bind_output(
                    reader,
                    progress.clone(),
                    joined.clone(),
                )));
            }
            for h in handles {
                let _ = h.await;
            }
        }
    });

    let session = Arc::new(BindSession {
        user_id: user_id.to_string(),
        child: Mutex::new(child),
        progress,
        created_at: Instant::now(),
    });
    // 启动窗口内尝试抓 URL（最多 5s，抓不到由轮询补取）
    let mut url = String::new();
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Some(u) = session.progress.read().await.0.clone() {
            url = u;
            break;
        }
    }
    registry.write().await.insert(session_id.clone(), session);
    Ok((session_id, url))
}

/// 绑定会话状态快照
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindSessionSnapshot {
    pub phase: BindPhase,
    pub verification_url: Option<String>,
    pub error: Option<String>,
}

/// 查询绑定会话状态（驱动终态检测：child.try_wait）
pub async fn bind_session_status(
    user_id: &str,
    session_id: &str,
) -> Result<Option<BindSessionSnapshot>> {
    let registry = bind_registry();
    let sessions = registry.read().await;
    let Some(session) = sessions.get(session_id) else {
        return Ok(None);
    };
    if session.user_id != user_id {
        return Ok(None);
    }
    // 终态检测：进程已退出时定 Done/Failed（分支 B：secret 不可读，前端引导补填）
    {
        let mut child = session.child.lock().await;
        let mut progress = session.progress.write().await;
        if progress.1 == BindPhase::Pending {
            match child.try_wait() {
                Ok(Some(status)) => {
                    if status.success() {
                        progress.1 = BindPhase::Done;
                    } else {
                        progress.1 = BindPhase::Failed;
                        progress.2 =
                            Some(format!("lark-cli config init 退出码 {:?}", status.code()));
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    progress.1 = BindPhase::Failed;
                    progress.2 = Some(format!("绑定进程状态检查失败: {}", e));
                }
            }
        }
    }
    let progress = session.progress.read().await;
    Ok(Some(BindSessionSnapshot {
        phase: progress.1,
        verification_url: progress.0.clone(),
        error: progress.2.clone(),
    }))
}

/// 取消绑定会话（kill 进程并移除）
pub async fn cancel_bind_session(user_id: &str, session_id: &str) -> Result<bool> {
    let registry = bind_registry();
    let session = registry.write().await.remove(session_id);
    let Some(session) = session else {
        return Ok(false);
    };
    if session.user_id != user_id {
        return Ok(false);
    }
    let mut child = session.child.lock().await;
    let _ = child.kill().await;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_device_login_success_top_level() {
        let stdout = r#"{"ok":true,"identity":"user","device_code":"dev-123","verification_url":"https://open.feishu.cn/verify?code=abc","expires_in":600}"#;
        let result = parse_device_login_json(stdout).unwrap();
        assert_eq!(result.device_code, "dev-123");
        assert_eq!(
            result.verification_url,
            "https://open.feishu.cn/verify?code=abc"
        );
        assert_eq!(result.expires_in, Some(600));
    }

    #[test]
    fn parse_device_login_success_nested_data() {
        let stdout = r#"{"ok":true,"data":{"device_code":"dev-9","verification_uri_complete":"https://x/v"}}"#;
        let result = parse_device_login_json(stdout).unwrap();
        assert_eq!(result.device_code, "dev-9");
        assert_eq!(result.verification_url, "https://x/v");
        assert_eq!(result.expires_in, None);
    }

    #[test]
    fn parse_device_login_missing_fields_errors() {
        assert!(parse_device_login_json(r#"{"ok":true}"#).is_err());
        assert!(parse_device_login_json("not json").is_err());
    }

    #[test]
    fn parse_auth_status_logged_in() {
        let stdout = r#"{"ok":true,"identity":"user","verified":true,"identities":{"user":{"userName":"zhangsan","openId":"ou_x","tokenStatus":"valid","scope":"all"}}}"#;
        let status = parse_auth_status_json(stdout);
        assert!(status.logged_in);
        assert_eq!(status.user_name.as_deref(), Some("zhangsan"));
    }

    #[test]
    fn parse_auth_status_not_logged_in() {
        let stdout = r#"{"ok":false,"error":{"type":"authorization","message":"not logged in"}}"#;
        let status = parse_auth_status_json(stdout);
        assert!(!status.logged_in);
        assert!(status.user_name.is_none());
        // 非法 JSON 同样降级为未登录
        let status = parse_auth_status_json("garbage");
        assert!(!status.logged_in);
    }

    #[test]
    fn parse_logout_json_variants() {
        assert!(parse_logout_json(r#"{"ok":true,"loggedOut":true}"#));
        assert!(!parse_logout_json(r#"{"ok":true,"loggedOut":false}"#));
        assert!(parse_logout_json(r#"{"ok":true}"#));
        assert!(!parse_logout_json("garbage"));
    }

    #[test]
    fn keychain_degradation_detection() {
        assert!(detect_keychain_degradation("error: failed to access macOS keychain").is_some());
        assert!(detect_keychain_degradation("钥匙串访问被拒绝").is_some());
        assert!(detect_keychain_degradation("network error").is_none());
    }

    #[test]
    fn extract_error_hint_uses_envelope_and_redacts() {
        let stderr = r#"{"ok":false,"error":{"type":"config","subtype":"not_configured","message":"not configured","hint":"run config init"}}"#;
        let hint = extract_error_hint(stderr);
        assert!(hint.contains("not configured"));
        assert!(hint.contains("run config init"));
        // 脱敏：含 secret 关键字的行被替换
        let stderr_plain = "app_secret: leaked\nline2";
        let hint = extract_error_hint(stderr_plain);
        assert!(!hint.contains("leaked"));
    }

    /// config init --new 实测输出 fixture：QR 码块后的 URL 行
    #[test]
    fn extract_verification_url_from_real_output() {
        let output = "████████████████\n████ ▄▄▄▄▄ ████\n▀▀▀▀▀▀\n\n打开以下链接配置应用:\n\n  https://open.feishu.cn/page/cli?user_code=ABCD-1234&lpv=1.0.72\n\n等待配置应用...";
        assert_eq!(
            extract_verification_url(output).as_deref(),
            Some("https://open.feishu.cn/page/cli?user_code=ABCD-1234&lpv=1.0.72")
        );
        assert!(extract_verification_url("no url here").is_none());
        assert!(extract_verification_url("").is_none());
    }

    /// BindPhase 字符串表示稳定（前端轮询判定）
    #[test]
    fn bind_phase_as_str() {
        assert_eq!(BindPhase::Pending.as_str(), "pending");
        assert_eq!(BindPhase::Done.as_str(), "done");
        assert_eq!(BindPhase::Failed.as_str(), "failed");
    }
}
