//! 技能安装与副本：安装到 Agent、发布回写副本、Agent 副本检索。
//!
//! 三者共享同一个副本模型：`(author_id = agent, parent_skill_id = 源技能)`。
//! - `install_to_agent`：安装/同步入口，**原地覆盖**已有副本（含 Expired 复活），
//!   避免「每次同步新增一行」的堆积；
//! - `publish_writeback_copy`：发布后的反向补偿，给作者 Agent 一份可编辑副本；
//! - `find_agent_skill_copies`：批量查已有副本（同步增量检测用），排除 Expired。

use super::SkillDalImpl;
use crate::models::skill::Skill;
use crate::pkg::request_context::RequestContext;
use crate::service::dao::skill::SkillQuery;
use common::enums::SkillStatus;
use common::error::{Result, err};

impl SkillDalImpl {
    pub(super) async fn install_to_agent(
        &self,
        ctx: RequestContext,
        source_skill_id: &str,
        agent_id: &str,
    ) -> Result<Skill> {
        let ctx = ctx.to_builder().agent_id(agent_id).build();
        // 先获取源技能（PO + 文件列表，文件列表含每个文件的元信息）
        let source_po = self
            .skill_dao
            .find_by_id(ctx.clone(), source_skill_id)
            .await?
            .ok_or_else(|| err!(ResourceNotFound, "Skill not found"))?;
        let source_files = self.skill_dao.list_files(&source_po)?;

        // ===== 自有原始技能幂等守卫 =====
        // Agent 安装「自己创建的原始技能」（author_id 即本 Agent 且无 parent）时，
        // 该技能本就以 author_id = agent_id 在自己的技能池内，若继续副本化会生成
        // parent_skill_id 指向自己的冗余行，导致提示词出现两份同名技能。
        // 与下方副本幂等策略同语义：直接返回源技能本体，不产生新行。
        if source_po.author_id == agent_id && source_po.parent_skill_id.is_empty() {
            return Ok(Skill {
                po: source_po,
                files: source_files,
                search_match: None,
            });
        }

        // ===== 原地更新策略（解决 Expired 副本堆积） =====
        // 含 Expired 在内：只要 (author_id=agent_id, parent_skill_id=source_id) 存在任意状态副本，
        // 就复用其 ID，执行「状态重置（Expired→Draft）+ 元数据覆盖 + 字节级文件 diff 覆盖写」。
        // 这样 agent 技能过期后点"同步/重建"直接恢复为最新版本，不会出现重复行堆积。
        let existing = self
            .skill_dao
            .query(
                ctx.clone(),
                SkillQuery {
                    author_id: Some(agent_id.to_string()),
                    parent_skill_id: Some(source_skill_id.to_string()),
                    ..Default::default()
                },
            )
            .await?;

        if let Some(mut existing_po) = existing.items.into_iter().next() {
            // 1. 目标状态：统一 Draft（Expired 恢复、其他状态保持当前业务期望的可编辑状态）
            let target_status = if matches!(existing_po.status, SkillStatus::Published) {
                SkillStatus::Published
            } else {
                SkillStatus::Draft
            };
            let is_status_change = existing_po.status != target_status;
            if is_status_change {
                existing_po.status = target_status;
            }

            // 2. 元数据以源为准（name/description/tags/category 覆盖；保留 updated_at 重置到本次）
            // 仅在字段实际不同时置 changed，避免无谓 UPDATE
            let mut is_meta_change = false;
            if existing_po.name != source_po.name && !source_po.name.is_empty() {
                existing_po.name = source_po.name.clone();
                is_meta_change = true;
            }
            if existing_po.description != source_po.description {
                existing_po.description = source_po.description.clone();
                is_meta_change = true;
            }
            if existing_po.tags != source_po.tags {
                existing_po.tags = source_po.tags.clone();
                is_meta_change = true;
            }
            if existing_po.category != source_po.category {
                existing_po.category = source_po.category.clone();
                is_meta_change = true;
            }

            // 3. 文件字节级 diff：只覆盖有变化的文件（减少不必要 IO）。
            // 源路径 → 副本绝对路径，读取两侧字节比较。
            let mut any_file_changed = false;
            for sf in &source_files {
                let src_path = self.skill_dao.file_abs_path(&source_po, &sf.filename);
                // 源文件不存在（极小概率，list_files 刚拿到但瞬时被删）→ skip
                let src_bytes = match std::fs::read(&src_path) {
                    Ok(b) => b,
                    Err(_) => continue,
                };
                let dst_path = self.skill_dao.file_abs_path(&existing_po, &sf.filename);
                let dst_bytes = std::fs::read(&dst_path).ok();
                let dst_match = dst_bytes.as_ref().map(|b| b == &src_bytes).unwrap_or(false);
                if !dst_match {
                    any_file_changed = true;
                    self.skill_dao
                        .write_file_bytes(&existing_po, &sf.filename, &src_bytes)?;
                }
            }

            // 4. 若有变更 → 统一 UPDATE PO + 重置 updated_at + 刷新向量索引
            let needs_update = is_status_change || is_meta_change || any_file_changed;
            if needs_update {
                existing_po.updated_at = common::constants::utils::current_timestamp_ms();
                let updated_skill = Skill {
                    po: existing_po.clone(),
                    files: vec![],
                    search_match: None,
                };
                // 走 self.update() 复用"向量索引 hash 变化才重算"的逻辑，避免无谓 embed
                self.update(ctx.clone(), &updated_skill).await?;
            }

            // 组装返回实体（重新 list_files，确保字节落盘后与展示一致）
            let files = self.skill_dao.list_files(&existing_po)?;
            return Ok(Skill {
                po: existing_po,
                files,
                search_match: None,
            });
        }

        // 无副本：走 DAO 原子安装（复制文件 + 创建 DB 记录）
        let installed_po = self
            .skill_dao
            .install_to_agent(ctx.clone(), &source_po, agent_id)
            .await?;
        // 新创建的技能：由于直接走 DAO.install_to_agent 没有调用 DAL 的 create() → 不会自动建向量索引，
        // 这里补一次向量索引（create 流程等价，失败仅 warn 降级）。
        if let Ok(Some(vec_params)) = self
            .try_build_skill_vector_params(ctx.clone(), &installed_po)
            .await
        {
            let _ = self
                .skill_vector_dao
                .upsert_vector(ctx.clone(), &installed_po.id, &vec_params)
                .await;
        }
        let files = self.skill_dao.list_files(&installed_po)?;
        Ok(Skill {
            po: installed_po,
            files,
            search_match: None,
        })
    }

    pub(super) async fn publish_writeback_copy(
        &self,
        ctx: RequestContext,
        source_skill_id: &str,
        agent_id: &str,
    ) -> Result<Option<Skill>> {
        // 1. 源技能必须存在（发布事务的回写对象；缺失=可能已被删除，不阻断）
        let Some(source_po) = self
            .skill_dao
            .find_by_id(ctx.clone(), source_skill_id)
            .await?
        else {
            log_warn!(
                &ctx,
                "skill_writeback",
                skill_id = %source_skill_id,
                "回写跳过：源技能不存在（可能已被删除），不阻断发布主流程"
            );
            return Ok(None);
        };

        // 2. 幂等查重：该 Agent 名下已存在指向本源的任意有效状态副本（!= Expired）
        //    → 跳过。重复发布/补偿重放不产生重复副本。
        let existing = self
            .skill_dao
            .query(
                ctx.clone(),
                SkillQuery {
                    author_id: Some(agent_id.to_string()),
                    parent_skill_id: Some(source_skill_id.to_string()),
                    exclude_status: Some(SkillStatus::Expired),
                    ..Default::default()
                },
            )
            .await?;
        if !existing.items.is_empty() {
            log_debug!(
                &ctx,
                "skill_writeback",
                skill_id = %source_skill_id,
                agent_id = %agent_id,
                "回写跳过：已存在有效副本，幂等去重"
            );
            return Ok(None);
        }

        // 3. 纯建副本路径：直调 DAO install_to_agent（无 self-created 幂等分支：
        //    校验源 Published → 新 v7 id → parent=源 id → author=Agent → Draft →
        //    copy_skill_dir 文件拷贝 → insert）
        let installed_po = self
            .skill_dao
            .install_to_agent(ctx.clone(), &source_po, agent_id)
            .await?;

        // 4. 向量索引补偿：DAO 直建副本不会自动建索引，补一次（best-effort，失败仅 warn 降级）
        if let Ok(Some(vec_params)) = self
            .try_build_skill_vector_params(ctx.clone(), &installed_po)
            .await
            && let Err(e) = self
                .skill_vector_dao
                .upsert_vector(ctx.clone(), &installed_po.id, &vec_params)
                .await
        {
            log_warn!(
                &ctx,
                "skill_writeback",
                skill_id = %installed_po.id,
                error = ?e,
                "回写副本向量索引补偿失败，已降级"
            );
        }

        let files = self.skill_dao.list_files(&installed_po)?;
        log_info!(
            &ctx,
            "skill_writeback",
            source_skill_id = %source_skill_id,
            copy_id = %installed_po.id,
            agent_id = %agent_id,
            "发布回写完成：已为作者 Agent 生成本地工作副本"
        );
        Ok(Some(Skill {
            po: installed_po,
            files,
            search_match: None,
        }))
    }

    pub(super) async fn find_agent_skill_copies(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        parent_skill_ids: &[String],
    ) -> Result<Vec<Skill>> {
        if parent_skill_ids.is_empty() {
            return Ok(Vec::new());
        }

        // 查询 Agent 的所有**有效**技能副本（排除 Expired），按 parent_skill_id 列表过滤。
        // 排除 Expired：软删除的旧副本不算已安装，否则同步/重装的增量检测会误判「已拥有」。
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

        let id_set: std::collections::HashSet<&String> = parent_skill_ids.iter().collect();
        let mut skills = page.items;
        skills.retain(|s| id_set.contains(&s.po.parent_skill_id));
        Ok(skills)
    }
}
