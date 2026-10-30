//! 技能文件内容来源适配（Handler 层内部模块）
//!
//! `create_skill` / `update_skill` 共用：把 [`SkillFileInput`] 描述的外部引用
//! （`attachment_id` 或 `artifact_id` 二选一）翻译成 Domain 的 [`SkillFileImport`]，
//! 下游 `process_skill_package` 从此不必关心 id 属于哪种实体。
//!
//! ## 为什么两种来源都解析成磁盘路径
//!
//! 走同一条 PATH 分支，技能包（zip）由 `process_skill_package` 按 magic 解包、
//! 单文件 copy 落盘——**整包安装只有这一个实现**，不会因来源增加而长出第二套
//! 解包逻辑（这是当初把兼容分支全部清掉要保的性质）。
//!
//! 补充：早在支持 artifact 之前，这条 PATH 分支的单文件部分靠 rename(move) 落地，
//! 会把源文件吃掉；现已改为 copy 并保留源文件（Attachment / Artifact 都是带 DB
//! 行、可被消息与任务引用的实体，不能被导入动作搬空）。
//!
//! ## 边界
//!
//! 本模块是内部适配器，**禁止注册为 Agent 工具**。

use std::path::PathBuf;

use common::api::SkillFileInput;
use common::error::{Result, bail_err, err};

use crate::models::attachment::AttachmentGetOptions;
use crate::pkg::RequestContext;
use crate::pkg::paths;
use crate::service::domain::finance::domain as finance_domain;
use crate::service::domain::hr::SkillFileImport;
use crate::service::domain::project;

/// 把一批 `SkillFileInput` 翻译为 Domain 的导入描述符。
///
/// 校验前置：[`SkillContentInput::validate_files_single_source`](common::api::SkillContentInput::validate_files_single_source)
/// 已保证每项有且仅有一个来源 id；这里的 `match` 兜底是防御性的（防止将来有人
/// 绕过入口校验直接调本函数时落到含糊分支）。
pub async fn to_skill_file_imports(
    ctx: RequestContext,
    files: &[SkillFileInput],
) -> Result<Vec<SkillFileImport>> {
    let mut imports = Vec::with_capacity(files.len());

    for file in files {
        let target_path = if file.target_path.trim().is_empty() {
            None
        } else {
            Some(file.target_path.trim().to_string())
        };

        let (source_abs_path, suggested_name) = match (file.has_attachment(), file.has_artifact()) {
            (true, false) => resolve_attachment(ctx.clone(), file).await?,
            (false, true) => resolve_artifact(ctx.clone(), file).await?,
            _ => bail_err!(
                InvalidRequest,
                "技能文件来源必须二选一：请提供 attachment_id 或 artifact_id"
            ),
        };

        imports.push(SkillFileImport {
            target_path,
            source_abs_path: Some(source_abs_path),
            content_bytes: None,
            suggested_name: Some(suggested_name),
            executable: None,
        });
    }

    Ok(imports)
}

/// 来源 A：通用附件 → 磁盘绝对路径。
async fn resolve_attachment(
    ctx: RequestContext,
    file: &SkillFileInput,
) -> Result<(PathBuf, String)> {
    let id = file.attachment_id.as_deref().unwrap_or_default().trim();

    let attachment = finance_domain()
        .attachment_manage()
        .get_attachment(
            ctx.clone(),
            id,
            AttachmentGetOptions {
                include_file_content: false,
            },
        )
        .await?
        .ok_or_else(|| err!(InvalidRequest, "附件 {} 不存在或无权访问", id))?;

    // 与 DAO 读写内容的落地点同源（路径推导的唯一真相在模型层；附件根目录必须由
    // 调用方给出——DAO 侧是可注入的，这里没有注入，取部署配置即是实际生效的那个）。
    let abs_path = attachment.file_abs_path(&paths::attachments_dir(
        &crate::config::get().base_data_path(),
    ))?;

    Ok((abs_path, attachment.po.original_name))
}

/// 来源 B：任务产出物 → 磁盘绝对路径（仅 GeneratedContent 型）。
///
/// 权限由 `get` 内部的项目归属校验保证（`validate_project_access`），越权 id 在
/// 这一步就转成「不存在」错误，拿不到路径；因此这里无需也不应重复校验一遍
/// （那会多打一次库）。
async fn resolve_artifact(ctx: RequestContext, file: &SkillFileInput) -> Result<(PathBuf, String)> {
    let id = file.artifact_id.as_deref().unwrap_or_default().trim();

    let artifact = project::domain()
        .artifact_manage()
        .get(ctx.clone(), id)
        .await?
        .ok_or_else(|| err!(InvalidRequest, "产物 {} 不存在或无权访问", id))?;

    // 与 DAO 读写内容的落地点同源（路径推导的唯一真相在模型层）。
    let abs_path = artifact.file_abs_path(&crate::config::get().base_data_path())?;

    // 优先用实际落盘的文件名：它带扩展名，能正确驱动 target 推断
    // （例如 *.md → 归位为 skill.md 主文件）；缺失时退回用户可读的产物名。
    let suggested_name = std::path::Path::new(&artifact.po.file_meta.0.file_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| artifact.po.name.clone());

    Ok((abs_path, suggested_name))
}
