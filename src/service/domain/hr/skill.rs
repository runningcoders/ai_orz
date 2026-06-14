//! Skill 管理具体方法实现

use crate::error::AppError;
use crate::models::skill::Skill;
use crate::pkg::RequestContext;
use crate::service::dao::skill::{SkillQuery, SkillSearch};
use crate::service::domain::hr::{HrDomainImpl, SkillManage, UpdateSkillParams};
use common::enums::SkillStatus;

#[async_trait::async_trait]
impl SkillManage for HrDomainImpl {
    // A. 技能基础管理（CRUD）

    async fn create_skill(&self, ctx: RequestContext, skill: &Skill) -> Result<(), AppError> {
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

    async fn get_skill(&self, ctx: RequestContext, id: &str) -> Result<Option<Skill>, AppError> {
        self.skill_dal.get_by_id(ctx, id.to_string()).await
    }

    async fn update_skill(
        &self,
        ctx: RequestContext,
        params: UpdateSkillParams<'_>,
    ) -> Result<(), AppError> {
        // 1. 更新元数据
        self.skill_dal.update(ctx.clone(), params.skill).await?;

        // 2. 处理文件写入
        for (filename, content) in params.file_writes {
            self.skill_dal
                .write_file(&params.skill.po, filename, content)?;
        }

        // 3. 处理文件删除：DAL 层暂时没有删除文件的方法，这里先留空
        // 后续可以在 DAO/DAL 层添加
        for _filename in params.file_deletes {
            // TODO: 实现文件删除
        }

        Ok(())
    }

    async fn delete_skill(&self, ctx: RequestContext, id: &str) -> Result<(), AppError> {
        self.skill_dal.delete(ctx, id).await
    }

    // B. 技能查询与搜索

    async fn query_skills(
        &self,
        ctx: RequestContext,
        query: SkillQuery,
    ) -> Result<Vec<Skill>, AppError> {
        self.skill_dal.query(ctx, query).await
    }

    async fn list_by_status(
        &self,
        ctx: RequestContext,
        status: SkillStatus,
    ) -> Result<Vec<Skill>, AppError> {
        self.query_skills(
            ctx,
            SkillQuery {
                status: Some(status),
                ..Default::default()
            },
        )
        .await
    }

    async fn list_by_category(
        &self,
        ctx: RequestContext,
        category: &str,
    ) -> Result<Vec<Skill>, AppError> {
        self.query_skills(
            ctx,
            SkillQuery {
                category: Some(category.to_string()),
                ..Default::default()
            },
        )
        .await
    }

    async fn list_by_author(
        &self,
        ctx: RequestContext,
        author_id: &str,
    ) -> Result<Vec<Skill>, AppError> {
        self.query_skills(
            ctx,
            SkillQuery {
                author_id: Some(author_id.to_string()),
                ..Default::default()
            },
        )
        .await
    }

    async fn list_for_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Vec<Skill>, AppError> {
        self.skill_dal.list_for_agent(ctx, agent_id).await
    }

    async fn search_skills(
        &self,
        ctx: RequestContext,
        search: SkillSearch,
    ) -> Result<Vec<Skill>, AppError> {
        self.skill_dal.search(ctx, search).await
    }

    // C. Agent 技能安装

    async fn install_to_agent(
        &self,
        ctx: RequestContext,
        source_skill_id: &str,
        agent_id: &str,
    ) -> Result<Skill, AppError> {
        self.skill_dal
            .install_to_agent(ctx, source_skill_id, agent_id)
            .await
    }
}
