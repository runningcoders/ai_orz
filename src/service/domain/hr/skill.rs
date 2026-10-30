//! Skill 管理具体方法实现

use crate::models::skill::{SKILL_MAIN_FILE, Skill, SkillPo};
use crate::pkg::RequestContext;
use crate::pkg::utils::fetch_remote_content::{FetchOptions, fetch_remote_content};
use crate::pkg::utils::fs_perm::{is_script_filename, mark_executable};
use crate::service::dao::skill::{SkillQuery, SkillSearch};
use crate::service::domain::hr::{
    CreateSkillParams, HrDomainImpl, SkillFileImport, SkillManage, UpdateSkillParams,
};
use common::constants::utils::current_timestamp_ms;
use common::enums::SkillStatus;
use common::enums::UserRole;
use common::enums::skill::SkillAuthorType;
use common::error::{Result, bail_err, ensure_err, err};
use std::path::{Component, Path};

/// Skill 访问意图：读（文件列表 / 文件内容） vs 写（元数据更新 / 文件写入 / 删除 / 恢复）。
///
/// 仅在本文件内部用于 `ensure_skill_access` 的 Agent 上下文分支：
/// Agent 写操作严格限定自己的记录；Agent 读操作放行 Published 共享技能。
#[derive(Clone, Copy)]
enum SkillAccessIntent {
    Read,
    Write,
}

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
        // 资源级权限校验（管理员 / 作者 / Agent 创建者 / Agent 上下文仅限自身记录）
        // 0. 快照发布前原值（N2 回写判定需要更新前 status：handler 传入的 po
        //    已是改后值，domain 入口自查最稳，handler 层零改动）
        let before = self
            .skill_dal
            .get_po_by_id(ctx.clone(), params.skill.po.id.clone())
            .await?;

        self.ensure_skill_access(&ctx, &params.skill.po, SkillAccessIntent::Write)
            .await?;

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

        // 4. 发布回写（N2 修复）：Agent 自有根技能「非 Published → Published」
        //    转换完成后，为作者 Agent 补建 1 条指向已发布技能的本地工作副本
        //    （Draft / parent=源 id / author=作者 Agent）。回写失败仅告警降级，
        //    不阻断发布主流程（副本缺失可由幂等补偿函数重放补齐，N5 复用）；
        //    源不存在（before=None，如并发删除）不触发。
        if let Some(before_po) = before.as_ref() {
            let is_publish_transition = before_po.status != SkillStatus::Published
                && params.skill.po.status == SkillStatus::Published;
            let is_agent_root_skill = matches!(params.skill.po.author_type, SkillAuthorType::Agent)
                && params.skill.po.parent_skill_id.is_empty();
            if is_publish_transition
                && is_agent_root_skill
                && let Err(e) = self
                    .skill_dal
                    .publish_writeback_copy(
                        ctx.clone(),
                        &params.skill.po.id,
                        &params.skill.po.author_id,
                    )
                    .await
            {
                log_warn!(
                    &ctx,
                    "skill_writeback",
                    skill_id = %params.skill.po.id,
                    error = ?e,
                    "发布回写失败，已降级（不阻断发布主流程，可由补偿函数重放补齐）"
                );
            }
        }

        Ok(())
    }

    async fn delete_skill(&self, ctx: RequestContext, id: &str) -> Result<()> {
        // 先拿到 po 做资源级权限判断（po 不存在则删空操作，无副作用不泄漏）
        if let Some(po) = self
            .skill_dal
            .get_po_by_id(ctx.clone(), id.to_string())
            .await?
        {
            self.ensure_skill_access(&ctx, &po, SkillAccessIntent::Write)
                .await?;
        }
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

    async fn list_expired_for_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Vec<Skill>> {
        let ctx = ctx.to_builder().agent_id(agent_id).build();
        self.skill_dal.list_expired_for_agent(ctx, agent_id).await
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

    async fn sync_installed_copies(
        &self,
        ctx: RequestContext,
        source_skill_id: &str,
    ) -> Result<usize> {
        // 找到该源技能的全部已安装副本（parent_skill_id 过滤；上限放宽避免漏同步）
        let copies = self
            .skill_dal
            .query(
                ctx.clone(),
                SkillQuery {
                    parent_skill_id: Some(source_skill_id.to_string()),
                    pagination: common::api::PaginationParams {
                        limit: Some(1000),
                        offset: None,
                    },
                    ..Default::default()
                },
            )
            .await?;

        let mut updated = 0usize;
        for copy in copies.items {
            // 副本的 author 即归属 Agent；防御性跳过非 Agent 作者的 parent 记录
            if !matches!(copy.po.author_type, SkillAuthorType::Agent) {
                continue;
            }
            // 复用 DAL install_to_agent 的幂等「原地更新」分支：元数据覆盖 +
            // 字节级文件 diff + 向量索引按需刷新 + Expired 副本恢复为 Draft，
            // 不产生重复行。同步后源技能必为 Published，满足其前置校验。
            //
            // 计数语义：只统计实际发生变更的副本——install_to_agent 的原地
            // 更新分支在「无差异」时不写库不推进 updated_at，用前后时间戳对比
            // 区分「处理过」与「真变更」，避免 toast 虚报。
            let before_updated_at = copy.po.updated_at;
            let refreshed = self
                .skill_dal
                .install_to_agent(ctx.clone(), source_skill_id, &copy.po.author_id)
                .await?;
            if refreshed.po.updated_at > before_updated_at {
                updated += 1;
            }
        }

        Ok(updated)
    }

    async fn restore_skill(&self, ctx: RequestContext, skill_id: &str) -> Result<Skill> {
        let Some(mut skill) = self
            .skill_dal
            .get_by_id(ctx.clone(), skill_id.to_string())
            .await?
        else {
            bail_err!(NotFound, "Skill {} not found", skill_id);
        };

        ensure_err!(
            matches!(skill.po.status, SkillStatus::Expired),
            Conflict,
            "Skill {} 状态非 Expired（当前={:?}），无需恢复",
            skill.po.id,
            skill.po.status
        );

        self.ensure_skill_access(&ctx, &skill.po, SkillAccessIntent::Write)
            .await?;

        skill.po.status = SkillStatus::Draft;
        skill.po.modifier_id = ctx.uid().to_string();
        skill.po.updated_at = current_timestamp_ms();

        self.skill_dal.update(ctx, &skill).await?;
        Ok(skill)
    }

    async fn list_skill_files(
        &self,
        ctx: RequestContext,
        skill_id: &str,
    ) -> Result<Option<Vec<crate::models::skill::SkillFile>>> {
        let Some(po) = self
            .skill_dal
            .get_po_by_id(ctx.clone(), skill_id.to_string())
            .await?
        else {
            return Ok(None);
        };

        // 权限检查：管理员 / 作者 / Agent 创建者（作者为 Agent 时）；
        // Agent 上下文额外放行 Published 共享技能（只读）
        self.ensure_skill_access(&ctx, &po, SkillAccessIntent::Read)
            .await?;

        let files = self.skill_dal.list_files(&po)?;
        Ok(Some(files))
    }

    async fn get_skill_file_content(
        &self,
        ctx: RequestContext,
        skill_id: &str,
        filename: &str,
    ) -> Result<Option<String>> {
        let Some(po) = self
            .skill_dal
            .get_po_by_id(ctx.clone(), skill_id.to_string())
            .await?
        else {
            return Ok(None);
        };

        // 权限检查：管理员 / 作者 / Agent 创建者（作者为 Agent 时）；
        // Agent 上下文额外放行 Published 共享技能（只读）
        self.ensure_skill_access(&ctx, &po, SkillAccessIntent::Read)
            .await?;

        // 接口只直出「根引导文件」：skill.md 说明这份技能怎么用，必须一次拿全。
        // 其余附件（二进制模板 / 脚本 / 大文件）一律不经接口回传——先用
        // list_skill_files 拿到 abs_path，再用 read_file / shell 工具按需访问。
        // 经接口传二进制既读不了（read_to_string 直接报错）又撑爆上下文。
        if !filename.eq_ignore_ascii_case(SKILL_MAIN_FILE) {
            let abs = self
                .skill_dal
                .file_abs_path(&po, filename)
                .display()
                .to_string();
            bail_err!(
                InvalidRequest,
                "技能附件内容不通过接口返回：请先用 list_skill_files 取得路径，再用 read_file 工具按需读取（{}）。\
                 仅根引导文件 {} 支持接口直读；脚本类附件可用 shell 工具按同一路径执行。",
                abs,
                SKILL_MAIN_FILE
            );
        }

        let content = self.skill_dal.read_file(&po, filename)?;
        Ok(Some(content))
    }

    async fn read_skill_file_bytes(
        &self,
        ctx: RequestContext,
        skill_id: &str,
        filename: &str,
    ) -> Result<Option<Vec<u8>>> {
        let Some(po) = self
            .skill_dal
            .get_po_by_id(ctx.clone(), skill_id.to_string())
            .await?
        else {
            return Ok(None);
        };

        self.ensure_skill_access(&ctx, &po, SkillAccessIntent::Read)
            .await?;

        // 按字节读：附件可能是图片 / 二进制可执行件，走 read_to_string 必然失败。
        let abs = self.skill_dal.file_abs_path(&po, filename);
        let bytes = std::fs::read(&abs)
            .map_err(|e| err!(NotFound, "技能文件不存在或不可读: {}", filename).with_source(e))?;
        Ok(Some(bytes))
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

        // 权限检查：管理员 / 作者 / Agent 创建者（作者为 Agent 时）；
        // Agent 上下文写操作仅限自身记录
        self.ensure_skill_access(&ctx, &po, SkillAccessIntent::Write)
            .await?;

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
    /// Skill 资源访问权限判定（文件内容读写 / 元数据更新 / 删除统一入口）。
    ///
    /// **放行条件（满足任一即 Return Ok）：**
    /// 0. Agent 上下文（ctx.agent_id 存在，工具调用）：
    ///    - Write：仅允许操作 author_id == agent_id 的记录（自己的技能副本 / 自建技能），
    ///      不参与下方用户侧条件（含 Admin bypass）——Agent 只能进化自己，不得借宿主身份
    ///      触达同创建者的其他 Agent 副本或改写 Published 源；
    ///    - Read：自己的记录，或 Published 共享技能（与 search_skill 的暴露面一致，
    ///      保障「隐藏技能按需读」能力）。
    /// 1. 用户角色 ≥ Admin（SuperAdmin(0) / Admin(1)，并查集 Admin owns Member）
    /// 2. 请求用户就是技能作者（po.author_id == uid）
    /// 3. 技能作者类型是 Agent，且请求用户是该 Agent 的创建者（Agent.created_by == uid）
    ///
    /// 其他情形 → 返回 ErrorCode::Forbidden（HTTP 403）+ 解释性错误信息。
    ///
    /// Context 补充原则符合性：Agent 查询仅在"不是管理员+不是作者"时才执行，
    /// 属于"下游明确需要，且当前上下文未包含"的场景下才专门查询。
    async fn ensure_skill_access(
        &self,
        ctx: &RequestContext,
        po: &SkillPo,
        intent: SkillAccessIntent,
    ) -> Result<()> {
        let uid = ctx.uid();

        // ---- ⓪ Agent 上下文收紧（独立短路，不走用户侧条件） ----
        if let Some(agent_id) = ctx.agent_id() {
            if po.author_id == *agent_id {
                return Ok(());
            }
            if matches!(intent, SkillAccessIntent::Read) && po.status == SkillStatus::Published {
                return Ok(());
            }
            bail_err!(
                Forbidden,
                "Agent {} 无权{} Skill {}（作者={}，状态={:?}）。\
                 Agent 上下文下写操作仅限 author_id 为自身的记录，读操作仅限自身记录或 Published 技能",
                agent_id,
                if matches!(intent, SkillAccessIntent::Write) {
                    "修改"
                } else {
                    "访问"
                },
                po.id,
                po.author_id,
                po.status,
            );
        }

        // ---- ① Admin Bypass ----
        if let Some(role) = ctx.user_role() {
            match UserRole::from(role) {
                UserRole::SuperAdmin | UserRole::Admin => return Ok(()),
                UserRole::Member => {} // 走后续检查
            }
        }

        // ---- ② 作者本人 ----
        if !uid.is_empty() && po.author_id == uid {
            return Ok(());
        }

        // ---- ③ Agent 创建者 ----
        if matches!(po.author_type, SkillAuthorType::Agent)
            && !po.author_id.is_empty()
            && let Some(agent) = self
                .agent_dal
                .find_by_id(ctx.clone(), &po.author_id)
                .await?
            && agent.po.created_by == uid
        {
            return Ok(());
        }

        // ---- 拒绝 ----
        bail_err!(
            Forbidden,
            "无权访问 Skill {}（作者类型={:?}，作者={}）。\
             可访问者：技能作者 / Agent 创建者（当作者类型为 Agent 时）/ 管理员",
            po.id,
            po.author_type,
            po.author_id,
        );
    }

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
                executable: None,
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
    ///
    /// 全程带 [`ImportBudget`] 累计校验：文件数与合计字节在**写入过程中**累加，
    /// 而非只看请求体——zip 解包后的条目数远大于请求里的 1 个 URL/附件，
    /// 只校验入口会让「1 个 zip 塞 5000 个文件」畅通无阻。
    fn process_skill_package(&self, po: &SkillPo, imports: &[SkillFileImport]) -> Result<()> {
        let mut budget = ImportBudget::default();
        for import in imports {
            if import.source_abs_path.is_some() {
                // PATH 分支
                self.process_path_source(po, import, &mut budget)?;
            } else if import.content_bytes.is_some() {
                // BYTES 分支
                self.process_bytes_source(po, import, &mut budget)?;
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
        import: &SkillFileImport,
        budget: &mut ImportBudget,
    ) -> Result<()> {
        let src_path = import.source_abs_path.as_ref().expect("PATH 分支必有路径");
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
            self.unpack_zip_archive(po, &mut archive, budget)?;
        } else {
            // 单文件
            let target = resolve_target(
                &import.target_path,
                Some(src_path),
                None,
                &import.suggested_name,
            )?;
            let size = std::fs::metadata(src_path).map(|m| m.len()).unwrap_or(0);
            budget.record(size)?;

            if !target.eq_ignore_ascii_case(SKILL_MAIN_FILE) {
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
            self.apply_executable_bit(&target_abs, &target, import.executable);
        }
        Ok(())
    }

    /// BYTES 分支：源内容在内存里，直接 write。
    fn process_bytes_source(
        &self,
        po: &SkillPo,
        import: &SkillFileImport,
        budget: &mut ImportBudget,
    ) -> Result<()> {
        let bytes = import.content_bytes.as_ref().expect("BYTES 分支必有内容");
        if bytes.len() >= 4 && bytes[0..4] == [0x50, 0x4B, 0x03, 0x04] {
            let cursor = std::io::Cursor::new(bytes);
            let mut archive = zip::ZipArchive::new(cursor)
                .map_err(|e| err!(InvalidRequest, "zip 解包失败: {}", e))?;
            self.unpack_zip_archive(po, &mut archive, budget)?;
        } else {
            let target = resolve_target(
                &import.target_path,
                None,
                Some(bytes),
                &import.suggested_name,
            )?;
            budget.record(bytes.len() as u64)?;

            let is_main = target.eq_ignore_ascii_case(SKILL_MAIN_FILE);
            if !is_main {
                validate_skill_import_target_path(&target)?;
            }
            if is_main {
                let content = String::from_utf8_lossy(bytes);
                self.skill_dal.write_file(po, &target, &content)?;
            } else {
                self.skill_dal.write_file_bytes(po, &target, bytes)?;
                let abs = self.skill_dal.file_abs_path(po, &target);
                self.apply_executable_bit(&abs, &target, import.executable);
            }
        }
        Ok(())
    }

    /// zip 解包：skill.md 必须在根目录，其他文件逐一校验+写入。
    fn unpack_zip_archive<R: std::io::Read + std::io::Seek>(
        &self,
        po: &SkillPo,
        archive: &mut zip::ZipArchive<R>,
        budget: &mut ImportBudget,
    ) -> Result<()> {
        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| err!(InvalidRequest, "zip 读取条目失败: {}", e))?;
            let name = file.name().to_string();

            if name.ends_with('/') {
                continue;
            }

            // zip 条目自带 unix mode 时按其 x 位判定，否则按扩展名兜底
            let zip_executable = file.unix_mode().is_some_and(|mode| mode & 0o111 != 0);

            if name.ends_with("/skill.md") {
                bail_err!(
                    InvalidRequest,
                    "skill.md 必须在 zip 根目录，不应有目录前缀: {}",
                    name
                );
            }

            budget.record(file.size())?;

            if name == SKILL_MAIN_FILE {
                let mut buf = Vec::new();
                std::io::Read::read_to_end(&mut file, &mut buf)
                    .map_err(|e| err!(InvalidRequest, "zip skill.md 读取失败: {}", e))?;
                let content = String::from_utf8_lossy(&buf);
                self.skill_dal.write_file(po, SKILL_MAIN_FILE, &content)?;
            } else {
                validate_skill_import_target_path(&name)?;
                let mut buf = Vec::new();
                std::io::Read::read_to_end(&mut file, &mut buf)
                    .map_err(|e| err!(InvalidRequest, "zip 条目 {} 读取失败: {}", name, e))?;
                self.skill_dal.write_file_bytes(po, &name, &buf)?;
                if zip_executable || is_script_filename(&name) {
                    mark_executable(&self.skill_dal.file_abs_path(po, &name));
                }
            }
        }
        Ok(())
    }

    /// 落盘后补可执行位（PATH / BYTES 单文件通路共用）。
    ///
    /// `explicit` 来自导入源（seed 快照往返的 `executable` 标记），优先于扩展名推断；
    /// 为 None 时退回扩展名推断，保证脚本类附件导入即可直接执行。
    fn apply_executable_bit(&self, abs_path: &Path, target: &str, explicit: Option<bool>) {
        let should_mark = explicit.unwrap_or_else(|| is_script_filename(target));
        if should_mark {
            mark_executable(abs_path);
        }
    }
}

/// 单技能文件数上限（zip 解包 / 附件 / 文本合计）。
const MAX_SKILL_FILES: usize = 100;
/// 单技能合计字节上限（10MB，zip 解包后累计）。
const MAX_SKILL_TOTAL_BYTES: u64 = 10 * 1024 * 1024;

/// 导入预算累加器：文件数与合计字节在**写入过程中**累计，超限立即拒绝。
///
/// 为什么不能在入口只校验 imports 长度：一个 zip 条目可以是成百上千个文件，
/// 入口计数永远是 1；同理 content_bytes 之外的 PATH 分支体积要读元数据才知道。
#[derive(Debug, Default)]
struct ImportBudget {
    files: usize,
    bytes: u64,
}

impl ImportBudget {
    fn record(&mut self, size: u64) -> Result<()> {
        self.files += 1;
        self.bytes = self.bytes.saturating_add(size);
        if self.files > MAX_SKILL_FILES {
            bail_err!(
                InvalidRequest,
                "单技能文件数超限（上限 {} 个）：请拆分技能或合并附件",
                MAX_SKILL_FILES
            );
        }
        if self.bytes > MAX_SKILL_TOTAL_BYTES {
            bail_err!(
                InvalidRequest,
                "单技能合计大小超限（上限 {} 字节）：附件过大请改用外部链接",
                MAX_SKILL_TOTAL_BYTES
            );
        }
        Ok(())
    }
}

/// 校验 target_path 安全性：仅允许 `[a-zA-Z0-9._-/]`，禁止绝对路径、`..`、`\`、控制字符。
///
/// 白名单为什么必要：光挡 `..` 挡不住 `scripts/;rm -rf` 这类注入到 shell 的
/// 文件名——技能附件是要给 Agent 用 shell 工具执行的，文件名最终会进命令行，
/// 所以字符集必须收敛到「路径里真正可能用到的那几个」。
pub(crate) fn validate_skill_import_target_path(target_path: &str) -> Result<()> {
    if target_path.trim().is_empty() {
        bail_err!(InvalidRequest, "Skill import target_path 不能为空");
    }

    // 字符白名单：字母数字（含中文等非 ASCII）+ `. _ - /`。
    //
    // 目的是挡住会注入 shell 命令行的元字符（`;` `$()` 反引号 空格 引号），
    // 不是要禁止非 ASCII——中文附件名不是注入风险，而整包导入（seed 技能包）
    // 若因中文名被拒，整个技能就导入不了。
    if !target_path
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'))
    {
        bail_err!(
            InvalidRequest,
            "Skill import target_path 含非法字符（仅允许字母数字与 . _ - /）: {}",
            target_path
        );
    }

    if target_path.contains("//") {
        bail_err!(
            InvalidRequest,
            "Skill import target_path 不能含连续斜杠: {}",
            target_path
        );
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
///
/// 规则本体在 `common::api::skill::default_target_path`（前端上传预填共用同一份），
/// 这里只做委托，避免前后端归位结果漂移。
fn apply_default_organization(name: &str) -> String {
    common::api::default_target_path(name)
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
