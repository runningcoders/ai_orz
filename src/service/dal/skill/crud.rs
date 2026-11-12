//! 技能元数据的增删改查与列表查询。
//!
//! 这里只处理「行 + 文件组装」，向量维护交给 `vector.rs`：
//! `create` / `update` 会在元信息变化时顺手维护一次元信息级向量，但**不含正文**
//! （那时正文还没落盘）；带内容源的写入走 `content.rs`，那里只算一次。

use super::SkillDalImpl;
use crate::models::skill::{Skill, SkillPo};
use crate::pkg::request_context::RequestContext;
use crate::service::dao::skill::SkillQuery;
use common::enums::SkillStatus;
use common::error::Result;

impl SkillDalImpl {
    pub(super) async fn create(&self, ctx: RequestContext, po: &SkillPo) -> Result<()> {
        // 1. 先保存基础技能数据
        self.skill_dao.insert(ctx.clone(), po).await?;

        // 2. 向量索引自动维护（失败仅 warn 降级，不影响主流程）
        match self.try_build_skill_vector_params(ctx.clone(), po).await {
            Ok(Some(vec_params)) => {
                if let Err(e) = self
                    .skill_vector_dao
                    .upsert_vector(ctx.clone(), &po.id, &vec_params)
                    .await
                {
                    log_warn!(
                        &ctx,
                        "vector_index",
                        skill_id = %po.id,
                        error = ?e,
                        "技能向量索引写入失败，已降级"
                    );
                }
            }
            Ok(None) => {
                // ⚠️ 这里必须是 info 而不是 debug：本分支意味着「技能已落库但没有任何
                // 向量」，是排查「技能搜不到 / 向量路恒空」的第一现场。此前用 debug，
                // 默认日志级别下完全不可见，看板上只能看到「召回集合只有 1~2 条」的
                // 结果，找不到原因。补齐路径：首个 embedding provider 创建成功会自动
                // 注册全量 RebuildVectorsTask（见 create_model_provider.rs）。
                log_info!(
                    &ctx,
                    "vector_index",
                    skill_id = %po.id,
                    "无可用 Embedding Provider，跳过技能向量索引（该技能暂无向量，配置默认 embedding provider 后由全量重建补齐）"
                );
            }
            Err(e) => {
                log_warn!(
                    &ctx,
                    "vector_index",
                    skill_id = %po.id,
                    error = ?e,
                    "技能向量化失败，已降级"
                );
            }
        }

        Ok(())
    }

    pub(super) async fn get_by_id(&self, ctx: RequestContext, id: String) -> Result<Option<Skill>> {
        let Some(po) = self.skill_dao.find_by_id(ctx, &id).await? else {
            return Ok(None);
        };
        let files = self.skill_dao.list_files(&po)?;
        Ok(Some(Skill {
            po,
            files,
            search_match: None,
        }))
    }

    pub(super) async fn get_po_by_id(
        &self,
        ctx: RequestContext,
        id: String,
    ) -> Result<Option<SkillPo>> {
        self.skill_dao.find_by_id(ctx, &id).await
    }

    pub(super) async fn query(
        &self,
        ctx: RequestContext,
        query: SkillQuery,
    ) -> Result<common::api::PagedResult<Skill>> {
        let page = self.skill_dao.query(ctx, query).await?;
        let mut skills = Vec::with_capacity(page.items.len());
        for po in page.items {
            let files = self.skill_dao.list_files(&po)?;
            skills.push(Skill {
                po,
                files,
                search_match: None,
            });
        }
        Ok(common::api::PagedResult {
            items: skills,
            total: page.total,
        })
    }

    pub(super) async fn list_by_status(
        &self,
        ctx: RequestContext,
        status: SkillStatus,
    ) -> Result<Vec<Skill>> {
        let page = self
            .query(
                ctx,
                SkillQuery {
                    status: Some(status),
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    pub(super) async fn list_by_category(
        &self,
        ctx: RequestContext,
        category: &str,
    ) -> Result<Vec<Skill>> {
        let page = self
            .query(
                ctx,
                SkillQuery {
                    category: Some(category.to_string()),
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    pub(super) async fn list_by_author(
        &self,
        ctx: RequestContext,
        author_id: &str,
    ) -> Result<Vec<Skill>> {
        let page = self
            .query(
                ctx,
                SkillQuery {
                    author_id: Some(author_id.to_string()),
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    pub(super) async fn list_for_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Vec<Skill>> {
        // 排除 Expired：过期技能是安装时间戳/包版本演进的 tombstone，
        // 不应出现在 Agent 详情页的可用技能栏里（恢复走 restore_skill 单条变更）。
        let page = self
            .query(
                ctx,
                SkillQuery {
                    author_id: Some(agent_id.to_string()),
                    exclude_status: Some(SkillStatus::Expired),
                    ..Default::default()
                },
            )
            .await?;

        // M2 修复 A：持有语义收紧——只保留 Agent「实际持有」的技能：
        // ① 安装副本（parent_skill_id 非空，install_to_agent 产物）；
        // ② 自有非正式发布技能（Agent 上下文创建的草稿等，status != Published）。
        // 共享库正式发布版根技能（author_id 历史上指向本 Agent、parent 为空、Published）
        // 属于共享库资产而非 Agent 私有持有，不再计入 Agent 技能列表。
        let mut skills = page.items;
        skills
            .retain(|s| !s.po.parent_skill_id.is_empty() || s.po.status != SkillStatus::Published);
        Ok(skills)
    }

    pub(super) async fn list_expired_for_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Vec<Skill>> {
        // 只返回 Expired：与 list_for_agent 天然互斥，用于「过期技能虚拟包」显示。
        let page = self
            .query(
                ctx,
                SkillQuery {
                    author_id: Some(agent_id.to_string()),
                    status: Some(SkillStatus::Expired),
                    exclude_status: None,
                    pagination: common::api::PaginationParams {
                        limit: None,
                        offset: None,
                    },
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    pub(super) async fn update(&self, ctx: RequestContext, skill: &Skill) -> Result<()> {
        // 1. 先更新基础技能数据
        self.skill_dao.update(ctx.clone(), &skill.po).await?;

        // 2. 向量索引自动维护：内容变化时重新索引（失败仅 warn 降级）
        let old_hash = self
            .get_vector_content_hash(ctx.clone(), &skill.po.id)
            .await?;
        // ⚠️ hash 必须按「元信息 + 正文」算，且与 upsert 用的文本同源；否则只改正文
        // 会被判成「内容未变」而跳过重算，正文永远进不了向量。
        // 注意此处的正文是调用方传入的 PO 对应的磁盘正文；带内容源的写入走
        // `update_with_content`，那里顺序是「先落盘 → 再落行 → 最后算向量」。
        let text = self.skill_vector_text(&skill.po);
        let new_hash = sha256::digest(&text);

        if old_hash.as_deref() != Some(&new_hash) {
            match self
                .build_vector_params(ctx.clone(), &skill.po, &text)
                .await
            {
                Ok(Some(vec_params)) => {
                    if let Err(e) = self
                        .skill_vector_dao
                        .upsert_vector(ctx.clone(), &skill.po.id, &vec_params)
                        .await
                    {
                        log_warn!(
                            &ctx,
                            "vector_index",
                            skill_id = %skill.po.id,
                            error = ?e,
                            "技能向量索引更新失败，已降级"
                        );
                    }
                }
                Ok(None) => {
                    // 同 `create`：无 provider ⇒ 该技能无向量，升 info 保证可观测。
                    log_info!(
                        &ctx,
                        "vector_index",
                        skill_id = %skill.po.id,
                        "无可用 Embedding Provider，跳过技能向量索引（该技能暂无向量，配置默认 embedding provider 后由全量重建补齐）"
                    );
                }
                Err(e) => {
                    log_warn!(
                        &ctx,
                        "vector_index",
                        skill_id = %skill.po.id,
                        error = ?e,
                        "技能向量化失败，已降级"
                    );
                }
            }
        }

        Ok(())
    }

    pub(super) async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()> {
        // 先获取 PO（用于删除文件时获取 content_path）
        let Some(po) = self.skill_dao.find_by_id(ctx.clone(), id).await? else {
            return Ok(()); // 不存在就返回成功
        };
        // 先删文件，再删数据库记录
        self.skill_dao.delete_skill_dir(&po)?;
        self.skill_dao.delete_by_id(ctx.clone(), id).await?;

        // 删除时清理向量索引（与 Tool DAL 一致，best-effort 不影响主流程）
        let _ = self.skill_vector_dao.delete_vector(ctx, id).await;

        Ok(())
    }

    pub(super) async fn list_published_by_tag(
        &self,
        ctx: RequestContext,
        tag: &str,
    ) -> Result<Vec<Skill>> {
        let page = self
            .query(
                ctx,
                SkillQuery {
                    tags: Some(vec![tag.to_string()]),
                    status: Some(SkillStatus::Published),
                    // M2 修复 B：排除安装副本（has_parent=false 即 parent_skill_id='' 根技能）。
                    // 共享库安装源必须来自正式发布的根技能，防止 Published 状态的
                    // Agent 副本（历史脏数据）被当作安装源复制出二级副本。
                    has_parent: Some(false),
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    pub(super) async fn list_tags(&self, ctx: RequestContext) -> Result<Vec<String>> {
        self.skill_dao.list_distinct_tags(ctx).await
    }
}
