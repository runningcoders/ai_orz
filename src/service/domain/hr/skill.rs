//! Skill 管理具体方法实现

use crate::models::skill::Skill;
use crate::pkg::RequestContext;
use crate::service::dao::skill::{SkillQuery, SkillSearch};
use crate::service::domain::hr::{HrDomainImpl, SkillManage, UpdateSkillParams};
use common::constants::utils::current_timestamp;
use common::enums::SkillStatus;
use common::error::{Result, bail_err};
use std::path::{Component, Path};

#[async_trait::async_trait]
impl SkillManage for HrDomainImpl {
    // A. 技能基础管理（CRUD）

    async fn create_skill(&self, ctx: RequestContext, skill: &Skill) -> Result<()> {
        // DAL 的 create 接收 SkillPo，需要先保存元数据
        self.skill_dal.create(ctx.clone(), &skill.po).await?;

        // 保存文件
        for file in &skill.files {
            if let Some(content) = &file.content {
                self.skill_dal
                    .write_file(&skill.po, &file.filename, content)?;
            }
        }

        Ok(())
    }

    async fn get_skill(&self, ctx: RequestContext, id: &str) -> Result<Option<Skill>> {
        self.skill_dal.get_by_id(ctx, id.to_string()).await
    }

    async fn update_skill(&self, ctx: RequestContext, params: UpdateSkillParams<'_>) -> Result<()> {
        // 1. 先校验所有附加文件导入路径，避免后续失败时产生部分文件/元数据更新。
        for file_import in &params.file_imports {
            validate_skill_import_target_path(&file_import.target_path)?;
        }

        // 2. 更新元数据
        self.skill_dal.update(ctx.clone(), params.skill).await?;

        // 3. 处理文件写入
        for (filename, content) in params.file_writes {
            self.skill_dal
                .write_file(&params.skill.po, filename, content)?;
        }

        // 4. 处理文件删除：DAL 层暂时没有删除文件的方法，这里先留空
        // 后续可以在 DAO/DAL 层添加
        for _filename in params.file_deletes {
            // TODO: 实现文件删除
        }

        // 5. 处理附加文件导入。路径安全规则属于 HR Skill Domain，Handler 只负责编排数据来源。
        for file_import in params.file_imports {
            self.skill_dal.write_file_bytes(
                &params.skill.po,
                &file_import.target_path,
                &file_import.bytes,
            )?;
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

    async fn search_skills(&self, ctx: RequestContext, search: SkillSearch) -> Result<Vec<Skill>> {
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
        po.updated_at = current_timestamp();
        po.modifier_id = ctx.uid().to_string();
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

    let components: Vec<_> = path.components().collect();
    if components.len() == 1
        && matches!(components[0], Component::Normal(part) if part.eq_ignore_ascii_case("skill.md"))
    {
        bail_err!(
            InvalidRequest,
            "Skill import target_path 不能覆盖主内容文件 skill.md"
        );
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
