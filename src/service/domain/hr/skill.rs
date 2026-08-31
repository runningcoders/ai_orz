//! Skill 管理具体方法实现

use crate::models::skill::{Skill, SkillPo};
use crate::pkg::RequestContext;
use crate::pkg::utils::fetch_remote_content::{FetchOptions, fetch_remote_content};
use crate::service::dao::skill::{SkillQuery, SkillSearch};
use crate::service::domain::hr::{
    CreateSkillParams, HrDomainImpl, SkillFileImport, SkillManage, UpdateSkillParams,
};
use common::constants::utils::current_timestamp_ms;
use common::enums::SkillStatus;
use common::error::{Result, bail_err, err};
use std::path::{Component, Path};

#[async_trait::async_trait]
impl SkillManage for HrDomainImpl {
    // A. 技能基础管理（CRUD）

    async fn create_skill(&self, ctx: RequestContext, params: CreateSkillParams<'_>) -> Result<()> {
        // 1. 保存元数据
        self.skill_dal.create(ctx.clone(), &params.skill.po).await?;

        // 2. 应用内容源流水线
        self.apply_content_sources(
            ctx.clone(),
            &params.skill.po,
            &params.imports,
            params.remote_source,
        )
        .await?;

        Ok(())
    }

    async fn get_skill(&self, ctx: RequestContext, id: &str) -> Result<Option<Skill>> {
        self.skill_dal.get_by_id(ctx, id.to_string()).await
    }

    async fn update_skill(&self, ctx: RequestContext, params: UpdateSkillParams<'_>) -> Result<()> {
        // 1. 更新元数据
        self.skill_dal.update(ctx.clone(), params.skill).await?;

        // 2. 应用内容源流水线
        self.apply_content_sources(
            ctx.clone(),
            &params.skill.po,
            &params.imports,
            params.remote_source,
        )
        .await?;

        // 3. 处理文件删除：调用 DAL 删除指定文件（禁删 skill.md，canonicalize 双防）
        for filename in params.file_deletes {
            self.skill_dal.delete_file(&params.skill.po, filename)?;
        }

        Ok(())
    }

    async fn delete_skill(&self, ctx: RequestContext, id: &str) -> Result<()> {
        self.skill_dal.delete(ctx, id).await
    }

    // B. 技能查询与搜索

    async fn query_skills(
        &self,
        ctx: RequestContext,
        query: SkillQuery,
    ) -> Result<common::api::PagedResult<Skill>> {
        self.skill_dal.query(ctx, query).await
    }

    async fn list_by_status(&self, ctx: RequestContext, status: SkillStatus) -> Result<Vec<Skill>> {
        let page = self
            .query_skills(
                ctx,
                SkillQuery {
                    status: Some(status),
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    async fn list_by_category(&self, ctx: RequestContext, category: &str) -> Result<Vec<Skill>> {
        let page = self
            .query_skills(
                ctx,
                SkillQuery {
                    category: Some(category.to_string()),
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    async fn list_by_author(&self, ctx: RequestContext, author_id: &str) -> Result<Vec<Skill>> {
        let page = self
            .query_skills(
                ctx,
                SkillQuery {
                    author_id: Some(author_id.to_string()),
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    async fn list_for_agent(&self, ctx: RequestContext, agent_id: &str) -> Result<Vec<Skill>> {
        let ctx = ctx.to_builder().agent_id(agent_id).build();
        self.skill_dal.list_for_agent(ctx, agent_id).await
    }

    async fn list_published_by_tag(&self, ctx: RequestContext, tag: &str) -> Result<Vec<Skill>> {
        self.skill_dal.list_published_by_tag(ctx, tag).await
    }

    async fn search_skills(
        &self,
        ctx: RequestContext,
        search: SkillSearch,
    ) -> Result<common::api::PagedResult<Skill>> {
        self.skill_dal.search(ctx, search).await
    }

    /// 列出所有已发布技能的 distinct tags
    async fn list_skill_tags(&self, ctx: RequestContext) -> Result<Vec<String>> {
        self.skill_dal.list_tags(ctx).await
    }

    // C. Agent 技能安装

    async fn install_to_agent(
        &self,
        ctx: RequestContext,
        source_skill_id: &str,
        agent_id: &str,
    ) -> Result<Skill> {
        let ctx = ctx.to_builder().agent_id(agent_id).build();
        self.skill_dal
            .install_to_agent(ctx, source_skill_id, agent_id)
            .await
    }

    async fn uninstall_from_agent(
        &self,
        ctx: RequestContext,
        skill_id: &str,
        agent_id: &str,
    ) -> Result<()> {
        let Some(po) = self
            .skill_dal
            .get_po_by_id(ctx.clone(), skill_id.to_string())
            .await?
        else {
            bail_err!(NotFound, "Skill not found: {}", skill_id);
        };
        if po.author_id != agent_id {
            bail_err!(
                InvalidRequest,
                "Skill {} does not belong to agent {}",
                skill_id,
                agent_id
            );
        }
        if po.parent_skill_id.is_empty() {
            bail_err!(
                InvalidRequest,
                "Skill {} is not an installed copy, cannot uninstall",
                skill_id
            );
        }
        // 复用 DAL delete：同时删除 DB 记录 + 文件目录
        self.skill_dal.delete(ctx, skill_id).await
    }

    async fn list_skill_files(
        &self,
        ctx: RequestContext,
        skill_id: &str,
    ) -> Result<Option<Vec<crate::models::skill::SkillFile>>> {
        let uid = ctx.uid().to_string();
        let Some(po) = self
            .skill_dal
            .get_po_by_id(ctx, skill_id.to_string())
            .await?
        else {
            return Ok(None);
        };

        // 权限检查：仅作者可访问
        if po.author_id != uid {
            bail_err!(InvalidRequest, "你没有权限访问该 Skill");
        }

        let files = self.skill_dal.list_files(&po)?;
        Ok(Some(files))
    }

    async fn get_skill_file_content(
        &self,
        ctx: RequestContext,
        skill_id: &str,
        filename: &str,
    ) -> Result<Option<String>> {
        let uid = ctx.uid().to_string();
        let Some(po) = self
            .skill_dal
            .get_po_by_id(ctx, skill_id.to_string())
            .await?
        else {
            return Ok(None);
        };

        // 权限检查：仅作者可访问
        if po.author_id != uid {
            bail_err!(InvalidRequest, "你没有权限访问该 Skill");
        }

        let content = self.skill_dal.read_file(&po, filename)?;
        Ok(Some(content))
    }

    async fn update_skill_file_content(
        &self,
        ctx: RequestContext,
        skill_id: &str,
        filename: &str,
        content: &str,
        expected_updated_at: Option<i64>,
    ) -> Result<()> {
        let Some(mut po) = self
            .skill_dal
            .get_po_by_id(ctx.clone(), skill_id.to_string())
            .await?
        else {
            bail_err!(NotFound, "Skill not found: {}", skill_id);
        };

        // 权限检查：仅作者可修改
        if po.author_id != ctx.uid() {
            bail_err!(InvalidRequest, "你没有权限修改该 Skill");
        }

        // 乐观锁校验
        if let Some(expected) = expected_updated_at
            && po.updated_at != expected
        {
            bail_err!(
                Conflict,
                "Skill updated_at mismatch: expected {}, current {}",
                expected,
                po.updated_at
            );
        }

        // 校验文件名合法性（复用导入校验逻辑）
        validate_skill_import_target_path(filename)?;

        // 写入文件内容
        self.skill_dal.write_file(&po, filename, content)?;

        // 更新 skill 元数据
        po.updated_at = current_timestamp_ms();
        po.modifier_id = ctx.caller_id_or_system();
        self.skill_dal
            .update(
                ctx.clone(),
                &Skill {
                    po,
                    files: vec![],
                    search_match: None,
                },
            )
            .await?;

        Ok(())
    }
}

impl HrDomainImpl {
    /// 内容源处理流水线（create/update 共享同一份代码）。
    ///
    /// 1. URL download 到 tmp → 合并进 imports
    /// 2. 统一 process_skill_package（zip 解包 / 5 级 target 推断 / rename 0 拷贝 / copy 降级 / write bytes 兜底）
    /// 3. Frontmatter Meta Merge + Vectors Refresh
    async fn apply_content_sources(
        &self,
        ctx: RequestContext,
        po: &SkillPo,
        imports: &[SkillFileImport],
        remote_source: Option<&str>,
    ) -> Result<()> {
        // 1. URL download 到 tmp → 合并进 imports
        let mut imports_copy = imports.to_vec();
        if let Some(url) = remote_source {
            let result = fetch_remote_content(url, &FetchOptions::default()).await?;
            let tmp_name = format!("skill_import_{}.tmp", current_timestamp_ms());
            let tmp_path = std::env::temp_dir().join(tmp_name);
            std::fs::write(&tmp_path, &result.bytes)?;
            imports_copy.push(SkillFileImport {
                target_path: None,
                source_abs_path: Some(tmp_path),
                content_bytes: None,
                suggested_name: None,
            });
        }

        // 2. 统一 process_skill_package
        self.process_skill_package(po, &imports_copy)?;

        // 3. Frontmatter Meta Merge + Vectors Refresh
        // 从最终写入的 skill.md 提取 YAML frontmatter，若 Po 字段为空/默认则覆盖
        if let Ok(md_content) = self.skill_dal.read_file(po, "skill.md")
            && let Some(fm) = parse_frontmatter(&md_content)
        {
            let mut updated_po = po.clone();
            let mut changed = false;

            if updated_po.name.is_empty()
                && let Some(title) = find_in_frontmatter(&fm, "title")
            {
                updated_po.name = title.to_string();
                changed = true;
            }

            if updated_po.description.is_empty()
                && let Some(desc) = find_in_frontmatter(&fm, "description")
            {
                updated_po.description = desc.to_string();
                changed = true;
            }

            if (updated_po.tags.is_empty() || updated_po.tags == "[]")
                && let Some(tags_raw) = find_in_frontmatter(&fm, "tags")
            {
                let tags_vec: Vec<String> = tags_raw
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                if !tags_vec.is_empty() {
                    updated_po.tags = serde_json::to_string(&tags_vec).unwrap_or_default();
                    changed = true;
                }
            }

            if (updated_po.category.is_empty() || updated_po.category == "uncategorized")
                && let Some(cat) = find_in_frontmatter(&fm, "category")
            {
                updated_po.category = cat.to_string();
                changed = true;
            }

            if changed {
                updated_po.updated_at = current_timestamp_ms();
                let updated_skill = Skill {
                    po: updated_po,
                    files: vec![],
                    search_match: None,
                };
                self.skill_dal.update(ctx, &updated_skill).await?;
            }
        }

        Ok(())
    }

    /// 统一处理技能文件包导入（PATH 分支 + BYTES 分支 + zip 解包 + 5 级 target 推断）。
    fn process_skill_package(&self, po: &SkillPo, imports: &[SkillFileImport]) -> Result<()> {
        for import in imports {
            if let Some(src_path) = &import.source_abs_path {
                // PATH 分支
                self.process_path_source(
                    po,
                    src_path,
                    &import.target_path,
                    &import.suggested_name,
                )?;
            } else if let Some(bytes) = &import.content_bytes {
                // BYTES 分支
                self.process_bytes_source(po, bytes, &import.target_path, &import.suggested_name)?;
            } else {
                bail_err!(
                    InvalidRequest,
                    "技能文件无内容来源（source_abs_path 和 content_bytes 均为空）"
                );
            }
        }
        Ok(())
    }

    /// PATH 分支：源文件在磁盘上，优先 rename 0 拷贝。
    fn process_path_source(
        &self,
        po: &SkillPo,
        src_path: &Path,
        target_path: &Option<String>,
        suggested_name: &Option<String>,
    ) -> Result<()> {
        // 读前 4B 做 zip magic 检测
        let mut file = std::fs::File::open(src_path)?;
        let mut header = [0u8; 4];
        use std::io::Read as _;
        let n = file.read(&mut header)?;
        drop(file);

        if n >= 4 && header == [0x50, 0x4B, 0x03, 0x04] {
            // zip 解包
            let file = std::fs::File::open(src_path)?;
            let mut archive = zip::ZipArchive::new(file)
                .map_err(|e| err!(InvalidRequest, "zip 解包失败: {}", e))?;
            self.unpack_zip_archive(po, &mut archive)?;
        } else {
            // 单文件
            let target = resolve_target(target_path, Some(src_path), None, suggested_name)?;
            if target != "skill.md" {
                validate_skill_import_target_path(&target)?;
            }
            let target_abs = self.skill_dal.file_abs_path(po, &target);
            if let Some(parent) = target_abs.parent() {
                std::fs::create_dir_all(parent)?;
            }
            // 尝试 rename（0 拷贝），失败则 copy + delete（跨 fs 降级）
            match std::fs::rename(src_path, &target_abs) {
                Ok(()) => {}
                Err(_) => {
                    std::fs::copy(src_path, &target_abs)?;
                    let _ = std::fs::remove_file(src_path);
                }
            }
        }
        Ok(())
    }

    /// BYTES 分支：源内容在内存里，直接 write。
    fn process_bytes_source(
        &self,
        po: &SkillPo,
        bytes: &[u8],
        target_path: &Option<String>,
        suggested_name: &Option<String>,
    ) -> Result<()> {
        if bytes.len() >= 4 && bytes[0..4] == [0x50, 0x4B, 0x03, 0x04] {
            let cursor = std::io::Cursor::new(bytes);
            let mut archive = zip::ZipArchive::new(cursor)
                .map_err(|e| err!(InvalidRequest, "zip 解包失败: {}", e))?;
            self.unpack_zip_archive(po, &mut archive)?;
        } else {
            let target = resolve_target(target_path, None, Some(bytes), suggested_name)?;
            if target != "skill.md" {
                validate_skill_import_target_path(&target)?;
            }
            if target == "skill.md" {
                let content = String::from_utf8_lossy(bytes);
                self.skill_dal.write_file(po, &target, &content)?;
            } else {
                self.skill_dal.write_file_bytes(po, &target, bytes)?;
            }
        }
        Ok(())
    }

    /// zip 解包：skill.md 必须在根目录，其他文件逐一校验+写入。
    fn unpack_zip_archive<R: std::io::Read + std::io::Seek>(
        &self,
        po: &SkillPo,
        archive: &mut zip::ZipArchive<R>,
    ) -> Result<()> {
        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| err!(InvalidRequest, "zip 读取条目失败: {}", e))?;
            let name = file.name().to_string();

            if name.ends_with('/') {
                continue;
            }

            if name.ends_with("/skill.md") {
                bail_err!(
                    InvalidRequest,
                    "skill.md 必须在 zip 根目录，不应有目录前缀: {}",
                    name
                );
            }

            if name == "skill.md" {
                let mut buf = Vec::new();
                std::io::Read::read_to_end(&mut file, &mut buf)
                    .map_err(|e| err!(InvalidRequest, "zip skill.md 读取失败: {}", e))?;
                let content = String::from_utf8_lossy(&buf);
                self.skill_dal.write_file(po, "skill.md", &content)?;
            } else {
                validate_skill_import_target_path(&name)?;
                let mut buf = Vec::new();
                std::io::Read::read_to_end(&mut file, &mut buf)
                    .map_err(|e| err!(InvalidRequest, "zip 条目 {} 读取失败: {}", name, e))?;
                self.skill_dal.write_file_bytes(po, &name, &buf)?;
            }
        }
        Ok(())
    }
}

/// 校验 target_path 安全性：禁止绝对路径、`..`、`\`、控制字符。
pub(crate) fn validate_skill_import_target_path(target_path: &str) -> Result<()> {
    if target_path.trim().is_empty() {
        bail_err!(InvalidRequest, "Skill import target_path 不能为空");
    }

    let path = Path::new(target_path);
    if path.is_absolute() {
        bail_err!(InvalidRequest, "Skill import target_path 不能是绝对路径");
    }

    if path.components().next().is_none() {
        bail_err!(InvalidRequest, "Skill import target_path 不能为空");
    }

    if target_path.contains('\\') {
        bail_err!(
            InvalidRequest,
            "Skill import target_path 不能包含反斜杠路径分隔符"
        );
    }

    if target_path.ends_with('/') {
        bail_err!(InvalidRequest, "Skill import target_path 不能指向目录");
    }

    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                bail_err!(InvalidRequest, "Skill import target_path 包含非法路径片段");
            }
        }
    }

    Ok(())
}

/// target_path=None 时的 5 级降级推断链：
/// 1. suggested_name → 直接用（附件 original_name）
/// 2. source_abs_path.file_name() → 用文件名
/// 3. 内容 magic 识别 → zip（已在外部处理）/ md → skill.md
/// 4. 特定场景默认组织 → 图片 assets/、脚本 scripts/、md skill.md
/// 5. 全失败 → 报错
fn resolve_target(
    target_path: &Option<String>,
    source_abs_path: Option<&Path>,
    content_bytes: Option<&[u8]>,
    suggested_name: &Option<String>,
) -> Result<String> {
    // 用户指定了 target_path → 直接用
    if let Some(tp) = target_path
        && !tp.trim().is_empty()
    {
        return Ok(tp.clone());
    }

    // 第 1 级：suggested_name（附件 original_name）
    if let Some(name) = suggested_name
        && !name.trim().is_empty()
    {
        return Ok(apply_default_organization(name));
    }

    // 第 2 级：source_abs_path.file_name()
    if let Some(src) = source_abs_path
        && let Some(fname) = src.file_name()
    {
        let name = fname.to_string_lossy().to_string();
        return Ok(apply_default_organization(&name));
    }

    // 第 3 级：内容 magic 识别（bytes 分支，zip 已在外部处理）
    if let Some(bytes) = content_bytes
        && is_likely_markdown(bytes)
    {
        return Ok("skill.md".to_string());
    }

    // 第 5 级：全失败
    bail_err!(
        InvalidRequest,
        "无法自动推断目标路径，请为该文件填写 target_path"
    );
}

/// 特定场景默认组织：图片 → assets/、脚本 → scripts/、md → skill.md
fn apply_default_organization(name: &str) -> String {
    let lower = name.to_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    match ext {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" => format!("assets/{}", name),
        "py" | "js" | "sh" | "ts" => format!("scripts/{}", name),
        "md" => "skill.md".to_string(),
        _ => name.to_string(),
    }
}

/// 检测 bytes 是否可能是 Markdown（YAML frontmatter 或 # 开头）
fn is_likely_markdown(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    let prefix = std::str::from_utf8(&bytes[..bytes.len().min(4)]).unwrap_or("");
    prefix.starts_with("---") || prefix.starts_with("# ")
}

/// 解析 Markdown frontmatter（YAML 头部）。
fn parse_frontmatter(content: &str) -> Option<Vec<(String, String)>> {
    let trimmed = content.trim_start();
    let prefix = "---\n";
    let prefix_crlf = "---\r\n";

    if !trimmed.starts_with(prefix) && !trimmed.starts_with(prefix_crlf) {
        return None;
    }

    let after_first = trimmed.split_once('\n')?.1;

    let yaml_end = after_first
        .find("\n---\n")
        .or_else(|| after_first.find("\n---\r\n"))
        .or_else(|| after_first.find("\n---"))?;

    let yaml_part = &after_first[..yaml_end];

    let mut pairs = Vec::new();
    for line in yaml_part.lines() {
        if line.starts_with("---") || line.trim().is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_lowercase();
            let value = value
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();
            if !key.is_empty() && !value.is_empty() {
                pairs.push((key, value));
            }
        }
    }

    if pairs.is_empty() { None } else { Some(pairs) }
}

/// 从 frontmatter 键值对中查找指定 key 的值
fn find_in_frontmatter<'a>(fm: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fm.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}
