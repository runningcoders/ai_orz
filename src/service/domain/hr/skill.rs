//! Skill 管理具体方法实现

use crate::models::skill::{
    SKILL_MAIN_FILE, Skill, SkillPo, is_main_skill_file, validate_skill_file_target,
};
use crate::pkg::RequestContext;
use crate::service::dao::skill::{SkillQuery, SkillSearch};
use crate::service::domain::hr::{CreateSkillParams, HrDomainImpl, SkillManage, UpdateSkillParams};
use common::constants::utils::current_timestamp_ms;
use common::enums::SkillStatus;
use common::enums::UserRole;
use common::enums::skill::SkillAuthorType;
use common::error::{Result, bail_err, ensure_err, err};

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
        // 元数据行 + 内容源落盘 + 向量索引一次成型。
        //
        // 顺序契约（行 → 文件 → 向量）定义在 DAL 内部，Domain 不再分两步调用：
        // 拆开调用时 create 会先按「尚未落盘的正文」算一次向量，正文写完后必须
        // 再算一次，第一次纯属浪费且会让 hash 记录短暂对应旧内容。
        self.skill_dal
            .create_with_content(
                ctx,
                params.skill.po.clone(),
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
        // 0. 快照发布前原值（状态跃迁判定需要更新前 status：handler 传入的 po
        //    已是改后值，domain 入口自查最稳，handler 层零改动）
        let before = self
            .skill_dal
            .get_po_by_id(ctx.clone(), params.skill.po.id.clone())
            .await?;

        self.ensure_skill_access(&ctx, &params.skill.po, SkillAccessIntent::Write)
            .await?;

        // 0.5 发布已收敛到独立入口（`publish_skill`）：这里只拦「跃迁」，不再回写。
        //     两个理由：
        //     - 发布带副作用（Agent 根技能要回写工作副本），塞在通用 update 里会让
        //       「改个描述」的请求也触碰发布逻辑；
        //     - 单一入口后 update 侧必须拒绝跃迁，否则可以绕过回写把技能刷成 Published。
        //     只拦**跃迁**而非「值」：系统 seed 走 update 刷新预置技能内容时，技能本来
        //     就是 Published（状态不变），按值拦会误伤整条预置同步链路。
        //     ⚠ 必须放在写库**之前**：放后面的话状态已落库，报错也改不回去（拒绝
        //     变成「失败但生效」，比不做还坏）。
        if let Some(before_po) = before.as_ref()
            && before_po.status != SkillStatus::Published
            && params.skill.po.status == SkillStatus::Published
        {
            bail_err!(
                InvalidRequest,
                "更新接口不允许把技能改为 Published；发布请调用 publish_skill 接口"
            );
        }

        // 1. 元数据行 + 内容源落盘 + 文件删除 + 向量索引一次成型。
        //
        // 为什么整体交付给 DAL：正文写盘与向量重算必须相邻（向量读的是磁盘上的
        // 正文），`update_with_content` 内部固定为「行 → 落盘 → 删文件 → 算向量」，
        // 中间不会出现「按旧正文算过一次向量」的浪费。
        self.skill_dal
            .update_with_content(
                ctx,
                params.skill.clone(),
                &params.imports,
                params.remote_source,
                &params.file_deletes,
            )
            .await?;

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

    /// 发布技能（Draft → Published）：状态跃迁 + Agent 根技能回写工作副本。
    ///
    /// 与 `update_skill` 的分工：update 只改元数据 / 内容，**不允许**「非 Published
    /// → Published」的跃迁（见 `update_skill` 的 0.5 守卫）；发布的一切副作用都在这里。
    ///
    /// 审核收口语义：Agent 产出的草稿由「收口人」发布——收口人可以是用户管理员
    /// （任意技能），也可以是装了 `admin` 工具包的 Agent（仅 Agent 创作的技能）。
    /// 普通 Agent 不需要、也不应该为了发布自己的草稿去装 admin 包。
    async fn publish_skill(&self, ctx: RequestContext, skill_id: &str) -> Result<Skill> {
        let Some(mut skill) = self
            .skill_dal
            .get_by_id(ctx.clone(), skill_id.to_string())
            .await?
        else {
            bail_err!(NotFound, "Skill {} not found", skill_id);
        };

        // ① 权限分两条路：
        //
        //    **用户上下文** → 仅 Admin / SuperAdmin 可发布（作者本人也不例外）。
        //    发布是技能进共享库的闸门，属治理动作；普通成员的技能只能留在 Draft，
        //    由管理员或收口人 Agent 审核后发布。注意这里**不放行** `ensure_skill_access`
        //    的作者分支——「作者能改」与「作者能发布」是两种权利。
        //
        //    **Agent 上下文** → 收口人审核语义：只允许发布**由 Agent 创作**的技能
        //    （自己产出的、以及别的 Agent 产出的草稿），用户创作的技能不碰。「发布
        //    别人的技能」是用户管理员及以上的权限，不下放给 Agent。
        //
        //    谁能**调到**这个工具是另一道门：`publish_skill` 只挂 `admin` 工具包，
        //    装在哪些 Agent 上由 `install_tool_pack` 决定，没装的 Agent 工具表里根本
        //    没有它。而 `ctx.agent_id()` 只在内部工具调用链路里注入（中间件不读请求头）。
        match ctx.agent_id() {
            Some(agent_id) => ensure_err!(
                matches!(skill.po.author_type, SkillAuthorType::Agent),
                Forbidden,
                "Agent {} 只能发布由 Agent 创作的技能（当前作者类型={:?}，作者={}）；\
                 用户创作的技能请由用户管理员发布",
                agent_id,
                skill.po.author_type,
                skill.po.author_id
            ),
            None => {
                ensure_err!(
                    crate::models::skill::can_publish_by_role(ctx.user_role()),
                    Forbidden,
                    "仅管理员及以上可以发布技能（当前角色={:?}）；\
                     普通成员请将草稿提交给管理员或收口人 Agent 审核发布",
                    ctx.user_role()
                );
                self.ensure_skill_access(&ctx, &skill.po, SkillAccessIntent::Write)
                    .await?
            }
        }

        // ② Agent 安装副本（parent 非空）不能发布：它只是源技能的本地工作副本，
        //    发布必须回到根技能，否则共享库里会出现一条没有来源的重复技能。
        ensure_err!(
            !(matches!(skill.po.author_type, SkillAuthorType::Agent)
                && !skill.po.parent_skill_id.is_empty()),
            InvalidRequest,
            "Skill {} 是 Agent 安装副本，不能直接发布；请发布其根技能",
            skill.po.id
        );

        // ③ 幂等：已 Published 直接返回（重复点击不刷新 updated_at、不重复回写）
        if skill.po.status == SkillStatus::Published {
            return Ok(skill);
        }

        // ④ 过期技能必须先 restore：不允许「Expired → Published」直接复活，
        //    否则 restore 的显式把关就被绕过了。
        ensure_err!(
            !matches!(skill.po.status, SkillStatus::Expired),
            Conflict,
            "Skill {} 已过期（Expired），请先调用 restore_skill 恢复为 Draft 再发布",
            skill.po.id
        );

        skill.po.status = SkillStatus::Published;
        skill.po.modifier_id = ctx.uid().to_string();
        skill.po.updated_at = current_timestamp_ms();
        self.skill_dal.update(ctx.clone(), &skill).await?;

        // ⑤ 发布回写（N2）：Agent 自有根技能 Draft→Published 后，为作者 Agent 补建
        //    一条指向已发布技能的本地工作副本（Draft / parent=源 id / author=Agent）。
        //    失败仅告警降级，不阻断发布主流程（副本缺失可由幂等补偿重放补齐）。
        if matches!(skill.po.author_type, SkillAuthorType::Agent)
            && skill.po.parent_skill_id.is_empty()
            && let Err(e) = self
                .skill_dal
                .publish_writeback_copy(ctx.clone(), &skill.po.id, &skill.po.author_id)
                .await
        {
            log_warn!(
                &ctx,
                "skill_writeback",
                skill_id = %skill.po.id,
                error = ?e,
                "发布回写失败，已降级（不阻断发布主流程，可由补偿函数重放补齐）"
            );
        }

        // ⑥ 返回磁盘实况（files 由 DAL 现扫，避免调用方拿到更新前快照）
        self.skill_dal
            .get_by_id(ctx, skill.po.id.clone())
            .await?
            .ok_or_else(|| err!(NotFound, "Skill {} not found", skill.po.id))
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
        if !is_main_skill_file(filename) {
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
        validate_skill_file_target(filename)?;

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
}
