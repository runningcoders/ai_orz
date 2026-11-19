//! Handler: 从 Agent 工作目录下的文件创建 Attachment（供消息正文 `attachment:` 引用出站）

use std::path::{Component, Path, PathBuf};

use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CreateAttachmentFromPathRequest, CreateAttachmentFromPathResponse};
use common::error::{Result, bail_err, err};
use common::mime::normalize_or_infer;

use crate::models::attachment::AttachmentUpload;
use crate::pkg::RequestContext;
use crate::pkg::paths;
use crate::pkg::tool_registry::tool_security::fs::{
    FsAccessMode, ValidationResult, crosses_agent_workspace, is_sensitive_path,
    resolve_and_validate_path,
};
use crate::service::domain::finance::domain;

use super::response::to_detail;

/// 单文件读入内存的安全网（**不是业务上限**）
///
/// 附件是站内资产：创建环节不按渠道上限设限。渠道发不出去只影响「推送」，
/// 不该阻止资产入库——用户要的是「能存、能下载、以后还能引用」，而渠道上限
/// 会随渠道变化，绑在创建口上是把外部约束写进了内部模型。
/// 出站超限由 `message::resource_resolver` 跳过，并在正文里提示。
///
/// 这里只挡「一次性读进内存就会 OOM」的量级，与任何渠道无关。
const MAX_SOURCE_BYTES: u64 = 256 * 1024 * 1024;

/// 取文件名：优先入参，否则取路径最后一段；拒绝绝对路径与 `..` 片段。
fn safe_file_name(source_path: &str, explicit: Option<String>) -> Result<String> {
    if let Some(name) = explicit {
        if name.trim().is_empty()
            || name.contains('/')
            || name.contains('\\')
            || name == ".."
            || name.chars().any(|c| c.is_control())
        {
            bail_err!(InvalidRequest, "file_name 非法: {}", name);
        }
        return Ok(name);
    }
    let trimmed = source_path.trim_end_matches('/').trim_end_matches('\\');
    let base = Path::new(trimmed)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    // basename 理论上不含分隔符，但保底再清洗一次（避免 Windows 风格路径残留）
    Ok(base.replace(['\\', '/'], "_"))
}

/// 入参形状校验（纯函数，不碰全局 config，便于单测）：
/// 拒绝空串、绝对路径与任何父目录片段；返回 trim 后的相对路径。
fn validate_relative_path(source_path: &str) -> Result<&str> {
    let trimmed = source_path.trim();
    if trimmed.is_empty() || trimmed.chars().any(|c| c.is_control()) {
        bail_err!(InvalidRequest, "source_path不能为空或含控制字符");
    }
    let rel = Path::new(trimmed);
    if rel.is_absolute()
        || rel.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        bail_err!(
            InvalidRequest,
            "source_path 必须是相对路径且不含 ..: {}",
            trimmed
        );
    }
    // 与 fs_read/fs_write 同一安全基线：.env / id_rsa / *.pem / secret_key 等
    // 敏感载体一律拒（读档），避免 Agent 把凭据注册成附件外带。
    if is_sensitive_path(trimmed, FsAccessMode::Read) {
        bail_err!(InvalidRequest, "source_path 指向敏感文件: {}", trimmed);
    }
    Ok(trimmed)
}

/// 解析并校验源文件路径，返回规范化后的绝对路径。
///
/// 依次尝试两个允许根（与 `fs_write` 的落盘根 + prompt 声称的默认工作区对齐）：
///   1. `agents/{agent_id}/`
///   2. `users/{uid}/`（user_agent_workspace 在其下）
///
/// 命中存在的候选后**委托** [`resolve_and_validate_path`] 做安全基线（词法归一 →
/// canonicalize 摊平符号链接 → 作用域 → 摊平后的敏感名 → 拒符号链接），与
/// `fs_read` / `fs_write` 是同一份实现，避免策略分裂（只在那边补规则时这里漏掉）。
///
/// 本入口比工具侧多两条**更严**的策略（无用户确认通道，宁可硬拒）：
///   - 只收相对路径，且末端不得是符号链接；
///   - 跨 Agent 工作区直接拒（`crosses_agent_workspace`；`fs_write` 那里是弹确认）。
fn resolve_source(ctx: &RequestContext, agent_id: &str, source_path: &str) -> Result<PathBuf> {
    let trimmed = validate_relative_path(source_path)?;
    let rel = Path::new(trimmed);

    let base = crate::config::get().base_data_path();
    let mut roots = vec![paths::agent_data_dir(&base, agent_id)];
    if let Some(uid) = ctx.user_id().filter(|uid| !uid.is_empty()) {
        roots.push(paths::user_home(&base, uid));
    }

    for root in roots {
        let candidate = root.join(rel);
        // 末端符号链接直接拒（与 fs_read/fs_write 一致）；中间段的符号链接
        // 由下面 canonicalize + 作用域 + 摊平后敏感名检查共同约束。
        if let Ok(meta) = candidate.symlink_metadata()
            && meta.file_type().is_symlink()
        {
            bail_err!(InvalidRequest, "source_path 不能是符号链接: {}", trimmed);
        }
        // 共享基线放行「尚不存在」的路径（写新文件场景），本入口只取已存在的
        // 普通文件，故先自行判一次，未命中就试下一个根。
        if !candidate.is_file() {
            continue;
        }
        let Some(candidate_str) = candidate.to_str() else {
            continue;
        };
        let resolved =
            match resolve_and_validate_path(&base, candidate_str, &[], FsAccessMode::Read)? {
                ValidationResult::Valid(path) => path,
                // 两个根都在 base 之下：走到这里说明摊平符号链接后已越出 base
                ValidationResult::NeedConfirmation(_) => continue,
            };
        if !resolved.is_file() {
            continue;
        }
        // Agent 身份边界：user_home 根天然包含 users/{uid}/agents/{other}/work/，
        // 仅靠作用域 + 敏感词检查挡不住「读同用户其他 Agent 的工作区再外发」。
        if crosses_agent_workspace(&base, &resolved, Some(agent_id)) {
            bail_err!(
                InvalidRequest,
                "source_path 位于其他 Agent 的工作区: {}",
                trimmed
            );
        }
        return Ok(resolved);
    }
    bail_err!(InvalidRequest, "源文件不存在或越界: {}", trimmed)
}

/// 从磁盘文件创建 Attachment
#[register_handler_tool(
    id = "create_attachment_from_path",
    name = "Create Attachment From Path",
    description = "Register a file already on disk as an attachment and return its detail (including the ID to reference in messages as [name](attachment:id)). Pass source_path relative to the agent data directory (agents/{agent_id}/) or your user home — the same roots fs_write can write to. Binary files such as images are supported; there is no per-channel size limit at creation, though files beyond the chat channel's limit are skipped when the message is pushed and the omission is stated in the message text. MIME type is inferred from the file name if omitted. This is the way to attach a generated/downloaded image to a reply.",
    params = "common::api::CreateAttachmentFromPathRequest",
    tags = "file_management"
)]
#[generate_http_handler]
pub async fn create_attachment_from_path(
    ctx: RequestContext,
    params: CreateAttachmentFromPathRequest,
) -> Result<CreateAttachmentFromPathResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }
    let agent_id = ctx
        .agent_id()
        .ok_or_else(|| err!(InvalidRequest, "当前请求缺少 agent 上下文"))?;

    let source = resolve_source(&ctx, agent_id, &params.source_path)?;
    let file_name = safe_file_name(&params.source_path, params.file_name)?;
    let mime_type = normalize_or_infer(params.mime_type.as_deref().unwrap_or_default(), &file_name);
    // 先查元数据再读：拦的只是「读进内存会 OOM」的量级（见 MAX_SOURCE_BYTES），
    // 与渠道发得出发不出去无关。
    let meta = std::fs::metadata(&source).map_err(|e| {
        err!(
            InvalidRequest,
            "读取源文件元数据失败: {} err={}",
            params.source_path,
            e
        )
    })?;
    if meta.len() > MAX_SOURCE_BYTES {
        bail_err!(
            InvalidRequest,
            "源文件过大，无法读入内存: size={} limit={}",
            meta.len(),
            MAX_SOURCE_BYTES
        );
    }
    let bytes = std::fs::read(&source).map_err(|e| {
        err!(
            InvalidRequest,
            "读取源文件失败: {} err={}",
            params.source_path,
            e
        )
    })?;

    let upload = AttachmentUpload {
        original_name: file_name,
        mime_type,
        purpose: String::new(),
        bytes,
    };

    let attachment = domain()
        .attachment_manage()
        .create_attachment(ctx, upload)
        .await?;
    Ok(to_detail(&attachment))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_name_from_path_last_segment() {
        assert_eq!(
            safe_file_name("work/out/report.png", None).unwrap(),
            "report.png"
        );
    }

    #[test]
    fn explicit_file_name_rejects_path_and_empty() {
        assert!(safe_file_name("x", Some("../evil".to_string())).is_err());
        assert!(safe_file_name("x", Some("a/b".to_string())).is_err());
        assert!(safe_file_name("x", Some("  ".to_string())).is_err());
        assert_eq!(
            safe_file_name("x", Some("ok.png".to_string())).unwrap(),
            "ok.png"
        );
    }

    #[test]
    fn source_path_rejects_absolute_and_parent_dir() {
        // 纯函数校验：绝对路径与 .. 在碰全局 config 之前就被拒
        assert!(validate_relative_path("/etc/passwd").is_err());
        assert!(validate_relative_path("../secret").is_err());
        assert!(validate_relative_path("a/../b").is_err());
        assert!(validate_relative_path("   ").is_err());
        assert_eq!(
            validate_relative_path("  work/a.png  ").unwrap(),
            "work/a.png"
        );
    }

    #[test]
    fn source_path_rejects_sensitive_carriers() {
        // 与 fs_read/fs_write 同一读档：凭据载体一律不许注册成附件
        for p in [
            ".env",
            ".env.local",
            ".ssh/id_rsa",
            "work/secret_key.txt",
            "config/api_token.json",
            "cert/server.pem",
        ] {
            assert!(validate_relative_path(p).is_err(), "敏感路径应被拒: {p}");
        }
        // 普通文件名不得误杀
        for p in ["work/report.png", "notes/tokenizer.md", ".gitignore"] {
            assert!(validate_relative_path(p).is_ok(), "普通路径应放行: {p}");
        }
    }

    #[test]
    fn source_path_rejects_control_chars() {
        for p in ["work/a\n.png", "work/a\u{0000}.png", "work/a\r.png"] {
            assert!(validate_relative_path(p).is_err(), "控制字符应被拒");
        }
        assert!(safe_file_name("x", Some("a\nb.png".to_string())).is_err());
    }

    #[test]
    fn agent_workspace_boundary_rejects_other_agent_under_user_home() {
        // user_home 根天然含 users/{uid}/agents/{other}/work/：strip_prefix 虽在
        // 本用户树内，但必须靠 crosses_agent_workspace 挡住「读他人工作区」。
        use std::path::Path;
        let base = Path::new("/data");
        let current = Some("agent_a");
        // 其他 Agent 工作区（user_home 下）→ 越界
        assert!(crosses_agent_workspace(
            base,
            Path::new("/data/users/u1/agents/agent_b/work/out.png"),
            current,
        ));
        // 顶层 agents 树里其他 Agent → 越界
        assert!(crosses_agent_workspace(
            base,
            Path::new("/data/agents/agent_b/work/out.png"),
            current,
        ));
        // 当前 Agent 自己的工作区 → 放行
        assert!(!crosses_agent_workspace(
            base,
            Path::new("/data/agents/agent_a/work/out.png"),
            current,
        ));
        // 用户共享目录（非 agents 树）→ 放行
        assert!(!crosses_agent_workspace(
            base,
            Path::new("/data/users/u1/shared/projects/p1/out.png"),
            current,
        ));
    }
}
