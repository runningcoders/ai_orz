//! 技能向量：文本口径、单条重索引、全量重建。
//!
//! ⚠️ 口径（`SKILL_VECTOR_TEXT_VERSION = 2`）：向量化文本 = `name + description +
//! tags` **再加主文件正文**（截断到 `MAX_VECTOR_BODY_CHARS`）。正文在磁盘上、不在
//! PO 里，所以不能把 PO 直接交给 `embed_entity` —— 用 `SkillVectorDoc` 包装把拼好的
//! 文本喂给统一 embedding 通道，`content_hash` 也随之变成整段文本的哈希。
//!
//! 口径一变必须 +1 版本号：它参与 collection 元数据比较，否则「provider 没变」会让
//! `rebuild_vectors` 直接短路，存量技能永远停在旧口径上。

use super::SkillDalImpl;
use crate::models::skill::SkillPo;
use crate::models::vector::{VectorIndexParams, VectorPayload, Vectorizable};
use crate::pkg::background_task::TaskProgressCounter;
use crate::pkg::request_context::RequestContext;
use crate::service::dal::VECTOR_REBUILD_PAGE_SIZE;
use crate::service::dao::skill::SkillQuery;
use common::api::PaginationParams;
use common::enums::VectorRebuildMode;
use common::error::Result;

/// 主文件正文进入向量的最大字符数。
///
/// embedding 模型的输入有 token 上限（常见 8192），技能正文动辄上万字，整段塞进去
/// 会被服务端截断或直接 400。取前 4000 **字符**（按 `chars()` 数，不按字节，避免把
/// 中文 / emoji 从中间劈开）：技能的方法论与适用场景基本都在开头，尾部多为例句、
/// 变更记录，截掉对语义召回影响最小。
pub(crate) const MAX_VECTOR_BODY_CHARS: usize = 4000;

/// 技能向量化文本 = 元信息 + 主文件正文（正文截断到 [`MAX_VECTOR_BODY_CHARS`]）。
///
/// 抽成纯函数便于单测截断与空正文拼接口径；读盘由调用方负责。
pub(crate) fn compose_vector_text(meta: &str, body: Option<&str>) -> String {
    let Some(body) = body else {
        return meta.to_string();
    };
    let body = body.trim();
    if body.is_empty() {
        return meta.to_string();
    }
    let mut truncated: String = body.chars().take(MAX_VECTOR_BODY_CHARS).collect();
    if body.chars().count() > MAX_VECTOR_BODY_CHARS {
        truncated.push('…');
    }
    format!("{meta}\n{truncated}")
}

/// 技能向量文本口径版本：`vectorize_text` 的组成一变就 +1。
///
/// 它参与 collection 元数据比较。否则 `rebuild_vectors` 会因「provider 没变」直接
/// 短路，存量技能就永远停在旧口径上 —— 例如 2026-11-12 之前向量只含
/// `name + description + tags`，不含主文件正文。
pub(crate) const SKILL_VECTOR_TEXT_VERSION: u32 = 2;

/// collection 元数据标识：`{provider_id}#vt{N}`。
///
/// 复用 `model_provider_id` 槽位：provider 与文本口径任意一个变化都需要整集重建，
/// 合成一个键比较即可，不必为一个维度去扩 `VectorStore` trait。该槽位只有技能
/// 重建自己读写，不影响其他实体的 collection。
pub(crate) fn collection_tag(provider_id: &str) -> String {
    format!("{provider_id}#vt{SKILL_VECTOR_TEXT_VERSION}")
}

/// 技能向量文档：文本可注入（元信息 + 正文），payload 沿用 PO。
///
/// `SkillPo::vectorize_text()` 只有 `name + description + tags`，正文在磁盘上、不在
/// PO 里，所以不能直接把 PO 交给 `embed_entity`。这里用一层轻量包装把「拼好的文本」
/// 喂给统一 embedding 通道，`content_hash` 也随之变成整段文本的哈希 —— 正文一改，
/// hash 必变，重索引判断才成立。
struct SkillVectorDoc<'a> {
    po: &'a SkillPo,
    text: String,
}

impl Vectorizable for SkillVectorDoc<'_> {
    fn vectorize_text(&self) -> String {
        self.text.clone()
    }

    fn vector_collection() -> &'static str {
        "skills"
    }

    fn vector_id(&self) -> &str {
        &self.po.id
    }

    fn vector_payload(&self) -> VectorPayload {
        self.po.vector_payload()
    }
}

impl SkillDalImpl {
    pub(super) async fn get_vector_content_hash(
        &self,
        ctx: RequestContext,
        skill_id: &str,
    ) -> Result<Option<String>> {
        let row = self.skill_vector_dao.get_vector_row(ctx, skill_id).await?;
        Ok(row.map(|r| r.meta.content_hash))
    }

    pub(super) async fn reindex_vector(&self, ctx: RequestContext, po: &SkillPo) -> Result<()> {
        let text = self.skill_vector_text(po);
        let new_hash = sha256::digest(&text);
        if self
            .get_vector_content_hash(ctx.clone(), &po.id)
            .await?
            .as_deref()
            == Some(&new_hash)
        {
            return Ok(());
        }

        match self.build_vector_params(ctx.clone(), po, &text).await {
            Ok(Some(params)) => {
                if let Err(e) = self
                    .skill_vector_dao
                    .upsert_vector(ctx.clone(), &po.id, &params)
                    .await
                {
                    log_warn!(
                        &ctx,
                        "vector_index",
                        skill_id = %po.id,
                        error = ?e,
                        "技能正文向量索引写入失败，已降级"
                    );
                }
            }
            Ok(None) => {
                log_info!(
                    &ctx,
                    "vector_index",
                    skill_id = %po.id,
                    "无可用 Embedding Provider，跳过技能正文向量索引（该技能暂无向量，配置默认 embedding provider 后由全量重建补齐）"
                );
            }
            Err(e) => {
                log_warn!(
                    &ctx,
                    "vector_index",
                    skill_id = %po.id,
                    error = ?e,
                    "技能正文向量化失败，已降级"
                );
            }
        }
        Ok(())
    }

    pub(super) async fn rebuild_vectors(
        &self,
        ctx: RequestContext,
        progress: &TaskProgressCounter,
        mode: VectorRebuildMode,
    ) -> Result<()> {
        // 1. 获取当前启用的 Embedding Provider
        let Some(provider) = self
            .model_provider_dao
            .get_default_embedding_provider(ctx.clone())
            .await?
        else {
            log_debug!(
                &ctx,
                "rebuild_vectors",
                "无可用 Embedding Provider，跳过向量索引"
            );
            return Ok(());
        };
        let current_provider_id = provider.id.clone();
        // 元数据里存的是 `provider#vtN`：provider 与向量文本口径必须同时一致才跳过，
        // 否则口径升级（如正文开始进向量）后存量技能会被永久短路在旧向量上。
        let current_tag = collection_tag(&current_provider_id);

        // 2. 集合口径（provider + 文本版本）是否与当前一致
        let collection_name = "skills";
        let stored_tag = ctx
            .vector_store()
            .get_collection_model_provider_id(collection_name)
            .await?;
        let tag_matches = stored_tag.as_deref() == Some(current_tag.as_str());

        // 3. 定动作：只有「口径一致 + Missing」可以不清空集合逐条比对。
        //    口径不一致意味着存量向量是用别的模型/别的文本口径算的，逐条比对毫无意义
        //    （hash 必然全不等，代价等同全量），而且清空顺带清掉已删技能的孤儿行，
        //    所以这里退化为全量重建。
        let incremental = mode == VectorRebuildMode::Missing && tag_matches;
        if !mode.ignores_idempotent_skip() && tag_matches {
            log_info!(
                &ctx,
                "rebuild_vectors",
                collection = %collection_name,
                provider_id = %current_provider_id,
                tag = %current_tag,
                "向量索引 provider 与文本口径均一致，跳过重建"
            );
            return Ok(());
        }
        if !incremental {
            log_info!(
                &ctx,
                "rebuild_vectors",
                collection = %collection_name,
                mode = %mode.as_str(),
                tag_matches,
                "清空后全量重算（强制重建或集合口径已变化）"
            );
            self.skill_vector_dao.clear_collection(ctx.clone()).await?;
        } else {
            log_info!(
                &ctx,
                "rebuild_vectors",
                collection = %collection_name,
                "查漏补缺：不清空集合，逐条比对 content_hash，只补缺失或已变条目"
            );
        }

        // 4. 分页查全量技能并逐条重新索引
        // 排序键 updated_at 可变，但「被更新的技能」其向量已由常规写路径 upsert，
        // 偏移漂移最多导致重复处理（幂等），不会漏行。
        let mut offset = 0usize;
        loop {
            let page = self
                .query(
                    ctx.clone(),
                    SkillQuery {
                        pagination: PaginationParams {
                            limit: Some(VECTOR_REBUILD_PAGE_SIZE),
                            offset: Some(offset),
                        },
                        ..Default::default()
                    },
                )
                .await?;
            let fetched = page.items.len();
            if fetched == 0 {
                break;
            }
            for skill in &page.items {
                // ⚠️ 重建必须用「元信息 + 正文」的文本，不能把 `&skill.po` 直接交给
                // `embed_entity`：那只有 `name + description + tags`，一次全量重建会把
                // 所有技能的正文从向量里抹掉，恰好抵消 vt 版本升级的意义。
                let text = self.skill_vector_text(&skill.po);

                // 查漏补缺：hash 与存量一致就跳过（不重算、不覆盖）。
                // 这是「不清空集合」能省下 embedding 费用的唯一原因 —— 没有这步，
                // Missing 与 Force 的开销完全相同，只是少了 clear。
                if incremental {
                    let new_hash = sha256::digest(&text);
                    let existing = self
                        .get_vector_content_hash(ctx.clone(), &skill.po.id)
                        .await?;
                    if existing.as_deref() == Some(new_hash.as_str()) {
                        continue;
                    }
                    log_info!(
                        &ctx,
                        "rebuild_vectors",
                        skill_id = %skill.po.id,
                        had_vector = existing.is_some(),
                        "查漏补缺：向量缺失或内容已变，重算该条"
                    );
                }

                let doc = SkillVectorDoc {
                    po: &skill.po,
                    text,
                };
                match self
                    .cortex_dao
                    .embed_entity(ctx.clone(), &provider, &doc)
                    .await
                {
                    Ok(vec_params) => {
                        if let Err(e) = self
                            .skill_vector_dao
                            .upsert_vector(ctx.clone(), &skill.po.id, &vec_params)
                            .await
                        {
                            log_warn!(
                                &ctx,
                                "rebuild_vectors",
                                skill_id = %skill.po.id,
                                error = ?e,
                                "技能向量索引重建失败"
                            );
                        }
                    }
                    Err(e) => {
                        log_warn!(
                            &ctx,
                            "rebuild_vectors",
                            skill_id = %skill.po.id,
                            error = ?e,
                            "技能向量化失败，跳过"
                        );
                    }
                }
            }
            offset += fetched;
            progress.advance(fetched);
            if fetched < VECTOR_REBUILD_PAGE_SIZE {
                break;
            }
        }

        // 5. 更新元数据
        //
        // 查漏补缺也会盖口径章：整轮比对下来，所有「hash 与当前文本不一致」的条目
        // 都已被重算或报错（失败仅 warn），章面上代表的仍是「这一轮按当前口径全量
        // 核对过」。个别失败条目靠再点一次查漏补缺修复 —— Missing 不看章，不会被短路。
        ctx.vector_store()
            .set_collection_model_provider_id(collection_name, &current_tag)
            .await?;

        Ok(())
    }

    /// 技能向量化文本：元信息 + 主文件正文。
    ///
    /// 正文读不到（新建技能尚未落盘 / 无主文件 / 目录缺失）时只返回元信息，
    /// 不报错也不告警 —— 这是 `create` 的正常时序，不是异常。
    pub(super) fn skill_vector_text(&self, po: &SkillPo) -> String {
        let meta = po.vectorize_text();
        let body = self.skill_dao.read_main_content(po).ok();
        compose_vector_text(&meta, body.as_deref())
    }

    /// 用给定文本构建向量参数；无 Embedding Provider 时返回 `Ok(None)`。
    ///
    /// 文本由调用方先算好再传入：`update` / `reindex_vector` 都要先算 hash 做比对，
    /// 传进来可以避免同一份正文被读两次盘、拼两次串。
    pub(super) async fn build_vector_params(
        &self,
        ctx: RequestContext,
        po: &SkillPo,
        text: &str,
    ) -> Result<Option<VectorIndexParams>> {
        let Some(provider) = self
            .model_provider_dao
            .get_default_embedding_provider(ctx.clone())
            .await?
        else {
            return Ok(None);
        };
        let doc = SkillVectorDoc {
            po,
            text: text.to_string(),
        };
        let params = self.cortex_dao.embed_entity(ctx, &provider, &doc).await?;
        Ok(Some(params))
    }

    /// 尝试为技能构建向量索引参数（用于 create/update/install 索引场景）
    ///
    /// 任何中间步骤失败都会向上抛错；调用方决定是否 warn 降级。
    /// 返回 `Ok(None)` 表示无 Embedding Provider 配置（合法场景）。
    pub(super) async fn try_build_skill_vector_params(
        &self,
        ctx: RequestContext,
        po: &SkillPo,
    ) -> Result<Option<VectorIndexParams>> {
        let text = self.skill_vector_text(po);
        self.build_vector_params(ctx, po, &text).await
    }
}
