//! Skill DAL 模块
//!
//! 技能数据访问层：组合 DAO 完成业务级操作，组装完整 Skill 实体（PO + 文件），
//! 并维护「元数据行 / 文件系统 / 向量库」三处数据面的**写入顺序**。
//!
//! 文件组织（按业务消费面拆分；`SkillDal` trait 仍是单一事实源）：
//! - `crud.rs`    —— 创建 / 更新 / 删除 / 查询 / 列表
//! - `files.rs`   —— 技能目录与文件读写（DAO 透传，不做额外编排）
//! - `search.rs`  —— 向量 + FTS5 混合召回、三态匹配、聚合排序
//! - `vector.rs`  —— 向量文本口径、单条重索引、全量重建
//! - `content.rs` —— 内容源摄取（URL / zip / 附件 / bytes → 三存储一次写完）
//! - `install.rs` —— 安装到 Agent、发布回写副本、Agent 副本检索
//!
//! ⚠️ trait 对同一类型只能有一个 `impl` 块（E0119），因此业务实现分散在各文件的
//! **固有 impl** 中，本文件用 `skill_dal_impl!` 生成唯一的 trait impl 做 1:1 转发。

use crate::models::skill::{Skill, SkillFile, SkillFileImport, SkillPo};
use crate::pkg::background_task::TaskProgressCounter;
use crate::pkg::request_context::RequestContext;
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::model_provider::ModelProviderDao;
use crate::service::dao::skill::{self, SkillDao, SkillQuery, SkillSearch, SkillVectorDao};
use common::enums::SkillStatus;
use common::enums::VectorRebuildMode;
use common::error::Result;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

mod content;
mod crud;
mod files;
mod install;
mod search;
mod vector;

// ==================== 单例管理 ====================

static SKILL_DAL: OnceLock<Arc<dyn SkillDal>> = OnceLock::new();

/// 获取 Skill DAL 单例
pub fn dal() -> Arc<dyn SkillDal> {
    SKILL_DAL.get().cloned().unwrap()
}

/// 初始化 Skill DAL（使用全局单例 DAO）
pub fn init() {
    let _ = SKILL_DAL.set(new(
        skill::dao(),
        skill::vector_dao(),
        crate::service::dao::cortex::dao(),
        crate::service::dao::model_provider::dao(),
    ));
}

/// 创建 Skill DAL（返回 trait 对象）
pub fn new(
    skill_dao: Arc<dyn SkillDao + Send + Sync>,
    skill_vector_dao: Arc<dyn SkillVectorDao + Send + Sync>,
    cortex_dao: Arc<dyn CortexDao + Send + Sync>,
    model_provider_dao: Arc<dyn ModelProviderDao + Send + Sync>,
) -> Arc<dyn SkillDal> {
    Arc::new(SkillDalImpl {
        skill_dao,
        skill_vector_dao,
        cortex_dao,
        model_provider_dao,
    })
}

// ==================== DAL 接口 ====================

/// Skill DAL 接口
#[async_trait::async_trait]
pub trait SkillDal: Send + Sync {
    /// 创建新技能（仅数据库行 + 元信息向量）
    ///
    /// 不接收内容源：正文尚未落盘，向量只能覆盖 name / description / tags。
    /// 带文件导入的创建一律走 [`SkillDal::create_with_content`]，那里向量只算一次。
    async fn create(&self, ctx: RequestContext, po: &SkillPo) -> Result<()>;

    /// 创建技能并同步完成**所有数据面写入**：元数据行 → 内容源落盘 → 向量索引。
    ///
    /// 为什么必须合成一个方法：一个技能实体横跨 sqlite / 文件系统 / 向量库三个存储，
    /// 拆开调用就必然出现「向量按尚未落盘的正文计算」——
    /// `create` 时正文还不存在，只能拿元信息 embed，随后补写正文再 embed 一次：
    /// 两次 embedding 开销，且第一次必被第二次覆盖。
    ///
    /// 内部顺序固定，向量只在正文落盘之后算一次：
    /// `摄取文件 → 读回主文件合并 frontmatter → insert(row) → upsert(vector)`
    ///
    /// 返回**生效后的 PO**：frontmatter 可能补上 `name` / `description` / `tags` / `category`。
    ///
    /// 失败语义：内容源处理失败时整目录回滚（行尚未写入），不留「有行、无文件」的孤儿技能。
    async fn create_with_content(
        &self,
        ctx: RequestContext,
        po: SkillPo,
        imports: &[SkillFileImport],
        remote_source: Option<&str>,
    ) -> Result<SkillPo>;

    /// 更新技能并同步完成**所有数据面写入**。
    ///
    /// 与 [`SkillDal::create_with_content`] 同构，额外承担 `file_deletes`：
    /// 文件删除也是数据面写入的一部分，放在这里才能保证「先落盘、后删、最后算向量」
    /// 的顺序与 create 一致。返回生效后的 PO。
    async fn update_with_content(
        &self,
        ctx: RequestContext,
        skill: Skill,
        imports: &[SkillFileImport],
        remote_source: Option<&str>,
        file_deletes: &[&str],
    ) -> Result<SkillPo>;

    /// 根据 ID 获取完整技能（PO + 文件列表）
    async fn get_by_id(&self, ctx: RequestContext, id: String) -> Result<Option<Skill>>;

    /// 根据 ID 获取 PO 数据（不需要文件时用这个）
    async fn get_po_by_id(&self, ctx: RequestContext, id: String) -> Result<Option<SkillPo>>;

    /// 通用综合查询（返回完整 Skill 实体，包含 PO + 文件列表）
    async fn query(
        &self,
        ctx: RequestContext,
        query: SkillQuery,
    ) -> Result<common::api::PagedResult<Skill>>;

    /// 按状态查询（返回完整 Skill 实体）
    async fn list_by_status(
        &self,
        ctx: RequestContext,
        status: common::enums::SkillStatus,
    ) -> Result<Vec<Skill>>;

    /// 按分类查询（返回完整 Skill 实体）
    async fn list_by_category(&self, ctx: RequestContext, category: &str) -> Result<Vec<Skill>>;

    /// 按作者查询（返回完整 Skill 实体）
    async fn list_by_author(&self, ctx: RequestContext, author_id: &str) -> Result<Vec<Skill>>;

    /// 获取 Agent 的所有技能（返回完整 Skill 实体）
    async fn list_for_agent(&self, ctx: RequestContext, agent_id: &str) -> Result<Vec<Skill>>;

    /// 获取 Agent 当前所有标记为 Expired 的技能副本（用于详情页「已过期技能」
    /// 虚拟分组展示 + 「恢复」按钮）。与 list_for_agent 的结果集天然互斥。
    async fn list_expired_for_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Vec<Skill>>;

    /// 搜索技能（名称/描述/标签）
    async fn search(
        &self,
        ctx: RequestContext,
        search: SkillSearch,
    ) -> Result<common::api::PagedResult<Skill>>;

    /// 更新技能元数据（不影响文件）
    /// 更新技能（仅数据库）
    async fn update(&self, ctx: RequestContext, skill: &Skill) -> Result<()>;

    /// 删除技能（删除数据库记录 + 文件目录）
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 将已发布技能安装到 Agent（原子操作：复制文件 + 创建数据库记录）
    /// 返回安装后新创建的完整 Skill 业务实体
    async fn install_to_agent(
        &self,
        ctx: RequestContext,
        source_skill_id: &str,
        agent_id: &str,
    ) -> Result<Skill>;

    /// 读取技能主文件内容（skill.md）
    fn read_main_content(&self, skill: &SkillPo) -> Result<String>;

    /// 写入技能主文件内容（skill.md）
    fn write_main_content(&self, skill: &SkillPo, content: &str) -> Result<()>;

    /// 列出技能的所有文件（小文件自动预读内容）
    fn list_files(&self, skill: &SkillPo) -> Result<Vec<SkillFile>>;

    /// 读取指定文件内容
    fn read_file(&self, skill: &SkillPo, filename: &str) -> Result<String>;

    /// 写入文件内容
    fn write_file(&self, skill: &SkillPo, filename: &str, content: &str) -> Result<()>;

    /// 写入文件 bytes
    fn write_file_bytes(&self, skill: &SkillPo, filename: &str, bytes: &[u8]) -> Result<()>;

    fn skill_dir(&self, skill: &SkillPo) -> PathBuf;

    fn file_abs_path(&self, skill: &SkillPo, filename: &str) -> PathBuf;

    /// 删除技能目录内的指定文件（禁删 skill.md 主文件；canonicalize 校验在 skill_dir 下）。
    ///
    /// 不存在的文件视为成功（幂等）。
    fn delete_file(&self, skill: &SkillPo, filename: &str) -> Result<()>;

    /// 查询技能的向量索引内容哈希（判断是否需要重索引）
    async fn get_vector_content_hash(
        &self,
        ctx: RequestContext,
        skill_id: &str,
    ) -> Result<Option<String>>;

    /// 按磁盘上的主文件正文重建该技能的向量索引（幂等）。
    ///
    /// 调用时机：**内容写盘之后**。`create` / `update` 发生在文件落盘之前，那时读不到
    /// 正文，向量只能覆盖元信息；正文写完后补这一次，向量才真正代表「这个技能做什么、
    /// 怎么做」。带内容源的写入已收敛到 [`SkillDal::create_with_content`] /
    /// [`SkillDal::update_with_content`]，它们内部就在这里收口；本方法供纯正文写盘
    /// （如编辑器保存）后单独调用。
    ///
    /// 内部按内容 hash 比对：文本没变就不调 embedding（不产生额外调用与费用）。
    /// 无默认 Embedding Provider / 向量写入失败都只记日志并返回 `Ok` —— 向量是增强项，
    /// 不能反过来让技能写入失败。
    async fn reindex_vector(&self, ctx: RequestContext, po: &SkillPo) -> Result<()>;

    /// 按 tag 查询已发布技能（用于技能包安装）
    async fn list_published_by_tag(&self, ctx: RequestContext, tag: &str) -> Result<Vec<Skill>>;

    /// 列出所有已发布技能的 distinct tags
    async fn list_tags(&self, ctx: RequestContext) -> Result<Vec<String>>;

    /// 查询指定 Agent 已有的技能副本（通过 author_id 和 parent_skill_id 列表）
    /// 如果 parent_skill_ids 为空，返回空 Vec
    async fn find_agent_skill_copies(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        parent_skill_ids: &[String],
    ) -> Result<Vec<Skill>>;

    /// 发布完成后的「回写自身技能」补偿建副本（N2 修复）。
    ///
    /// 语义：Agent 自有根技能发布为 Published 后，为作者 Agent 生成 1 条指向
    /// 已发布技能的本地工作副本（Draft / parent=源 id / author=作者 Agent /
    /// content 全量拷贝 / 补向量索引）。
    ///
    /// 关键语义（N5 存量补偿将复用本方法，由单测锁定）：
    /// - 幂等查重：该 Agent 名下已存在指向本源且状态 != Expired 的副本 → 跳过
    ///   返回 `Ok(None)`，重复发布/补偿重放不产生重复行（Expired 副本允许重建，
    ///   与 install_to_agent 的「过期副本原地恢复」语义一致）；
    /// - 纯建副本路径：直调 DAO `install_to_agent`（不经 DAL 入口——后者对
    ///   「Agent 安装自有技能」有返回本体的幂等守卫，语义不符）；
    /// - 向量索引补偿：DAO 直建副本不会自动建索引，此处补一次（best-effort，
    ///   失败仅 warn 降级，不向上抛错）。
    async fn publish_writeback_copy(
        &self,
        ctx: RequestContext,
        source_skill_id: &str,
        agent_id: &str,
    ) -> Result<Option<Skill>>;

    /// 🔄 重建技能向量索引
    ///
    /// 单条失败不影响整体，用 `log_warn!` 记录；每页处理完通过 `progress` 上报条数。
    /// 三种模式见 [`VectorRebuildMode`]：
    ///
    /// - `Auto`：provider 与向量文本口径（`{provider}#vt{N}`）都一致就**整体跳过**；
    /// - `Missing`：**查漏补缺**。不清空集合，逐条比对 `content_hash`，只重算缺失或
    ///   内容已变的条目（口径不一致时旧向量不可信，退化为清空后全量重算）；
    /// - `Force`：清空集合后逐条重算。
    ///
    /// 手动「重建索引」不能用 `Auto`：它短路时一条都不看，而集合里仍可能缺条目
    /// （写入时无 provider、`upsert` 失败被降级吞掉）—— 用户会看到「完成但 0 条」。
    async fn rebuild_vectors(
        &self,
        ctx: RequestContext,
        progress: &TaskProgressCounter,
        mode: VectorRebuildMode,
    ) -> Result<()>;
}

// ==================== DAL 实现 ====================

/// Skill DAL 基础实现
pub struct SkillDalImpl {
    pub(super) skill_dao: Arc<dyn SkillDao + Send + Sync>,
    pub(super) skill_vector_dao: Arc<dyn SkillVectorDao + Send + Sync>,
    pub(super) cortex_dao: Arc<dyn CortexDao + Send + Sync>,
    pub(super) model_provider_dao: Arc<dyn ModelProviderDao + Send + Sync>,
}

// ==================== Trait 桥接 ====================
//
// E0119：同一 trait 对同一具体类型只能有一个 `impl` 块，因此业务实现分散为
// 各子模块的**固有 impl**，这里生成唯一 trait impl 做 1:1 转发。
// 固有方法优先于 trait 方法参与解析，`SkillDalImpl::search` 不会递归到本 impl。
//
// 同步方法（文件透传）单列一段：`async_trait` 不给它们加生命周期改写，
// 也不能混在 `async fn` 里生成。

macro_rules! skill_dal_impl {
    (
        sync: $($sname:ident($($sarg:ident: $sty:ty),*) -> $sret:ty),* $(,)? ;
        async: $($aname:ident($($aarg:ident: $aty:ty),*) -> $aret:ty),* $(,)?
    ) => {
        #[async_trait::async_trait]
        impl SkillDal for SkillDalImpl {
            $(
                fn $sname(&self, $($sarg: $sty),*) -> $sret {
                    SkillDalImpl::$sname(self, $($sarg),*)
                }
            )*
            $(
                async fn $aname(&self, $($aarg: $aty),*) -> $aret {
                    SkillDalImpl::$aname(self, $($aarg),*).await
                }
            )*
        }
    };
}

skill_dal_impl! {
    sync:
        read_main_content(skill: &SkillPo) -> Result<String>,
        write_main_content(skill: &SkillPo, content: &str) -> Result<()>,
        list_files(skill: &SkillPo) -> Result<Vec<SkillFile>>,
        read_file(skill: &SkillPo, filename: &str) -> Result<String>,
        write_file(skill: &SkillPo, filename: &str, content: &str) -> Result<()>,
        write_file_bytes(skill: &SkillPo, filename: &str, bytes: &[u8]) -> Result<()>,
        skill_dir(skill: &SkillPo) -> PathBuf,
        file_abs_path(skill: &SkillPo, filename: &str) -> PathBuf,
        delete_file(skill: &SkillPo, filename: &str) -> Result<()>;

    async:
        create(ctx: RequestContext, po: &SkillPo) -> Result<()>,
        create_with_content(
            ctx: RequestContext,
            po: SkillPo,
            imports: &[SkillFileImport],
            remote_source: Option<&str>
        ) -> Result<SkillPo>,
        update_with_content(
            ctx: RequestContext,
            skill: Skill,
            imports: &[SkillFileImport],
            remote_source: Option<&str>,
            file_deletes: &[&str]
        ) -> Result<SkillPo>,
        get_by_id(ctx: RequestContext, id: String) -> Result<Option<Skill>>,
        get_po_by_id(ctx: RequestContext, id: String) -> Result<Option<SkillPo>>,
        query(ctx: RequestContext, query: SkillQuery) -> Result<common::api::PagedResult<Skill>>,
        list_by_status(ctx: RequestContext, status: SkillStatus) -> Result<Vec<Skill>>,
        list_by_category(ctx: RequestContext, category: &str) -> Result<Vec<Skill>>,
        list_by_author(ctx: RequestContext, author_id: &str) -> Result<Vec<Skill>>,
        list_for_agent(ctx: RequestContext, agent_id: &str) -> Result<Vec<Skill>>,
        list_expired_for_agent(ctx: RequestContext, agent_id: &str) -> Result<Vec<Skill>>,
        search(ctx: RequestContext, search: SkillSearch)
            -> Result<common::api::PagedResult<Skill>>,
        update(ctx: RequestContext, skill: &Skill) -> Result<()>,
        delete(ctx: RequestContext, id: &str) -> Result<()>,
        install_to_agent(ctx: RequestContext, source_skill_id: &str, agent_id: &str) -> Result<Skill>,
        get_vector_content_hash(ctx: RequestContext, skill_id: &str) -> Result<Option<String>>,
        reindex_vector(ctx: RequestContext, po: &SkillPo) -> Result<()>,
        list_published_by_tag(ctx: RequestContext, tag: &str) -> Result<Vec<Skill>>,
        list_tags(ctx: RequestContext) -> Result<Vec<String>>,
        find_agent_skill_copies(
            ctx: RequestContext,
            agent_id: &str,
            parent_skill_ids: &[String]
        ) -> Result<Vec<Skill>>,
        publish_writeback_copy(ctx: RequestContext, source_skill_id: &str, agent_id: &str)
            -> Result<Option<Skill>>,
        rebuild_vectors(
            ctx: RequestContext,
            progress: &TaskProgressCounter,
            mode: VectorRebuildMode
        ) -> Result<()>,
}

#[cfg(test)]
mod content_test;
#[cfg(test)]
mod crud_test;
#[cfg(test)]
mod install_test;
#[cfg(test)]
mod search_test;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod vector_test;
