//! Skill 持久化对象

use common::enums::SkillStatus;
use common::enums::skill::SkillAuthorType;
use derive_builder::Builder;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::path::Path;

/// 技能描述中的「触发条件 / 内容概要」分隔符。
///
/// 约定写法：`<何时该读>必读全文：<读了有什么>`，例如
/// 「写记忆、建图谱或检索记忆前必读全文：Soul 出发的四层记忆模型…」。
/// 库内约 2/3 的技能遵循该约定；不遵循的退化为原单行摘要，不会渲染出错。
pub(crate) const TRIGGER_SEPARATOR: &str = "必读全文";

/// 技能主文件名（根引导文件）。
///
/// 契约：**只有这个文件的内容会经接口返回**；其余附件一律只暴露路径，
/// 由调用方（Agent）用 `read_file` / shell 工具按需自行读取或执行。
/// 理由：附件可能是二进制（图片 / 模板 / 压缩包），也可能很大，经接口回传
/// 既撑爆上下文又读不了二进制；暴露绝对路径后 Agent 能自由组合工具处理。
pub const SKILL_MAIN_FILE: &str = common::api::SKILL_MAIN_FILE;

/// 判定 `filename` 是否为根引导文件 `skill.md`（**大小写不敏感**）。
///
/// 为什么必须忽略大小写：外部技能包（如 Anthropic Agent Skills 规范）的主文件
/// 常写作 `SKILL.md`，macOS/Windows 默认文件系统本身就大小写不敏感。若这里用
/// 严格相等，会出现「包已成功导入、文件也在磁盘上、但接口读正文返回空」——
/// 而同样的包换个小写名就正常，是典型的偶发难查。
///
/// 全仓**只此一处**判定主文件身份：判定散落到各层后，只要有一处写成 `==`
/// （历史上 `main_content()` 就是），就会出现「别处都认、唯独接口取不到正文」
/// 的半通不通状态。同时导入侧会把命中的主文件**归一化为小写**落盘
/// （见 `process_skill_package`），让 DAO 层 `skill.md` 路径拼装恒稳成立。
pub fn is_main_skill_file(filename: &str) -> bool {
    filename.eq_ignore_ascii_case(SKILL_MAIN_FILE)
}

/// 该用户角色是否有「发布技能」的资格（**仅 Admin / SuperAdmin**）。
///
/// 发布是技能进共享库的唯一闸门，因此权限定位为治理动作而非作者权利：
/// 普通成员（作者本人也不例外）只能把技能留在 Draft，由管理员或收口人 Agent
/// 审核后发布。这与「创建/编辑」的权利不同——后者是作者本人和 Agent 创建者的。
///
/// ⚠️ `None`（无角色，如 Guest / 系统内部链路）必须判**否**：
/// `UserRole::default()` 是 `SuperAdmin`，若写成 `UserRole::from(role.unwrap_or_default())`
/// 就会把「没有身份」当成超级管理员（历史上 `From<i32>` 的非法值落 default 就是这么
/// 提权的）。宁可拒绝，也不要默认放行。
pub fn can_publish_by_role(user_role: Option<i32>) -> bool {
    use common::enums::user::UserRole;
    match user_role {
        Some(role) => matches!(UserRole::from(role), UserRole::SuperAdmin | UserRole::Admin),
        None => false,
    }
}

/// 主文件挑选结果（`pick_main_file` 的返回值，读写两侧共用一套口径）。
///
/// 为什么单独定义一个enum，而不是让调用方各自 `find`：
/// **归一化只在写入时起作用，磁盘上早先装好的旧包名字不会自动跟着改**
/// （历史上的包可能有 `SKILL.md`、`main.md`、`README.md`）。读取侧必须能按磁盘
/// 实况认出主文件，否则「文件也在磁盘上、但接口 content 恒空」的老毛病会在这些
/// 旧包上原样复现——而它们恰恰是所有环境里最容易被忽略的一批（Linux 大小写敏感
/// FS 上是必现，macOS 上因为 FS 不敏感反而查不出来）。
#[derive(Debug, PartialEq, Eq)]
pub enum MainFilePick<'a> {
    /// 根目录有正主（`skill.md` 的任意大小写变体）。
    /// 写入侧应归位成小写常量落盘，读取侧按返回的原名读即可。
    Main(&'a str),
    /// 没有正主、但根目录**恰好一个** `.md` → 兜底认它。
    /// 这是外部/手工打的技能包的常见形态（主文件叫 `main.md` / `README.md`）。
    Fallback(&'a str),
    /// 根目录没有 md：纯附件包（往已有技能追加文件时的合法形态）。
    None,
    /// 根目录有多个 md：歧义不可自动解。
    /// 读取侧**不猜**（退化为无正文），导入侧报错要求打包方显式提供 `skill.md`——
    /// 任选其一等于静默猜错，宁可让入口拿到明确报错。
    Ambiguous(Vec<&'a str>),
}

/// 在技能根目录的条目名集合里挑选主文件（`names` 为相对根目录的条目名，含子目录前缀）。
///
/// 三档规则（顺序敏感）：
/// 1. 存在 `skill.md`（忽略大小写）→ [`MainFilePick::Main`]；
/// 2. 无正主、根目录恰好一个 `.md`（子目录里的不算，扩展名忽略大小写）→ [`MainFilePick::Fallback`]；
/// 3. 其余 → 无 md 为 [`MainFilePick::None`]，多个 md 为 [`MainFilePick::Ambiguous`]。
pub fn pick_main_file(names: &[String]) -> MainFilePick<'_> {
    if let Some(main) = names
        .iter()
        .find(|n| is_main_skill_file(n))
        .map(String::as_str)
    {
        return MainFilePick::Main(main);
    }

    match root_markdown_names(names).as_slice() {
        [] => MainFilePick::None,
        [only] => MainFilePick::Fallback(only),
        many => MainFilePick::Ambiguous(many.to_vec()),
    }
}

/// 根级 Markdown 文件名（子目录条目不算；扩展名忽略大小写，如 `README.MD`）。
///
/// 拆出来是为了让「兜底挑选」与「报错时列出候选 / 校验指定名是否存在」共用同一
/// 个「什么算根级 md」的口径——各处自己写一遍 `.filter(...)` 迟早会长歪。
pub fn root_markdown_names(names: &[String]) -> Vec<&str> {
    names
        .iter()
        .filter(|n| {
            !n.contains('/')
                && Path::new(n)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        })
        .map(String::as_str)
        .collect()
}

/// 技能附属文件信息（纯数据结构，DAO 层使用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFile {
    /// 文件名（相对于技能目录，含子目录前缀如 `scripts/run.sh`）
    pub filename: String,
    /// 文件大小（字节）
    pub file_size: u64,
    /// 文件内容（**仅根 `skill.md` 预读**；其余附件恒为 None）
    pub content: Option<String>,
    /// 文件在磁盘上的绝对路径。
    ///
    /// 供 Agent 用 `read_file` / shell 工具按路径自行读取或执行（位于
    /// `base_data_path` 作用域内，已过 fs 工具的路径校验）。
    pub abs_path: std::path::PathBuf,
    /// 是否为根引导文件（这份技能的正文入口）。
    ///
    /// 由 DAO 扫盘时**按磁盘实况**标注（解析逻辑见 [`pick_main_file`]）：历史
    /// 包的主文件可能叫 `SKILL.md` / `main.md` / `README.md`，只按 `filename`
    /// 字面判定的话，这些旧包会「列得出来、却拿不到正文」。
    ///
    /// 为什么不用 Option：接口是只读投影，非主就是 false，没有第三种状态。
    pub is_main: bool,
    /// 是否带可执行位（脚本类附件，可用 shell 工具直接执行）
    pub executable: bool,
}

/// Skill 持久化对象
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Default)]
pub struct SkillPo {
    /// 技能ID: "name-slug--hash"
    pub id: String,
    /// 技能显示名称
    pub name: String,
    /// 技能描述：什么时候用这个技能
    pub description: String,
    /// JSON 数组：标签列表
    pub tags: String,
    /// 单一分类
    pub category: String,
    /// 父技能ID（继承来源，技能树演进）
    pub parent_skill_id: String,
    /// 创建人ID（用户ID 或 Agent ID）
    pub author_id: String,
    /// 作者类型
    pub author_type: SkillAuthorType,
    /// 最后修改人ID（用户ID 或 Agent ID）
    pub modifier_id: String,
    /// 技能状态
    pub status: SkillStatus,
    /// 创建时间戳（毫秒）
    pub created_at: i64,
    /// 更新时间戳（毫秒）
    pub updated_at: i64,
    /// 相对 base_data_path 的技能目录路径
    pub content_path: String,
}

impl SkillPo {
    /// 创建新的 SkillPo
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        name: String,
        description: String,
        tags: Vec<String>,
        category: String,
        parent_skill_id: String,
        author_id: String,
        author_type: SkillAuthorType,
        content_path: String,
    ) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        SkillPo {
            id,
            name,
            description,
            tags: serde_json::to_string(&tags).unwrap_or_default(),
            category,
            parent_skill_id,
            author_id: author_id.clone(),
            author_type,
            modifier_id: author_id,
            status: SkillStatus::default(),
            created_at: now,
            updated_at: now,
            content_path,
        }
    }

    /// 解析 tags 为 Vec<String>
    pub fn parse_tags(&self) -> Vec<String> {
        serde_json::from_str(&self.tags).unwrap_or_default()
    }

    /// 格式化技能摘要用于 Prompt 注入
    /// 只输出名称、id 和描述，不包含完整 skill.md 内容，避免 Token 膨胀。
    /// id 必须注入：Agent 副本 id 是安装时生成的 UUID，无法从名称推知，
    /// 缺失时 Agent 每次读全文都需先 search_skill / list_skills 多绕一轮；
    /// 有了 id 可直接 get_skill(skill_id) 一步按需读取全文（读自身副本无条件放行）。
    pub fn to_prompt_summary(&self) -> String {
        // 触发条件前置：约定 description 用「…必读全文：…」把「何时该读」与「读了有什么」
        // 隔开，这里按该约定切成两行渲染。
        //
        // 为什么要拆：单行摘要里触发条件埋在长句中，模型扫不到——实测全库 trace 里
        // get_skill / search_skill 等技能类工具调用次数为 0，而「记忆认知」技能明确写着
        // 「写记忆、建图谱或检索记忆前必读全文」，save_short_term_memory 却调了 2800+ 次
        // 一次全文都没读。拆成两行让「何时读」可竖向扫读，是触发条件能被看到的前提。
        //
        // 不拆数据模型：description 保持自由文本，无分隔符的技能退化为原单行格式。
        match self.description.split_once(TRIGGER_SEPARATOR) {
            Some((trigger, rest)) => {
                let trigger = trigger.trim();
                // 分隔符后可能紧跟全角/半角冒号与空格
                let rest = rest.trim_start_matches(['：', ':', ' ', '\n']).trim();
                if trigger.is_empty() {
                    return format!("- {}（id: {}）：{}", self.name, self.id, self.description);
                }
                if rest.is_empty() {
                    return format!(
                        "- {}（id: {}）\n  ⚠️ 读全文的时机：{}",
                        self.name, self.id, trigger
                    );
                }
                format!(
                    "- {}（id: {}）\n  ⚠️ 读全文的时机：{}\n  全文含：{}",
                    self.name, self.id, trigger, rest
                )
            }
            None => format!("- {}（id: {}）：{}", self.name, self.id, self.description),
        }
    }

    /// 获取标签列表
    pub fn get_tags(&self) -> Vec<String> {
        if self.tags.is_empty() {
            return Vec::new();
        }
        serde_json::from_str(&self.tags).unwrap_or_default()
    }
}

// ==================== Skill 业务聚合实体 ====================

use crate::models::vector::{SearchMatchInfo, VectorPayload, Vectorizable};

/// 技能完整业务实体（PO + 文件系统内容 + 搜索元信息）
///
/// - SkillPo：数据库持久化元数据
/// - files：关联文件列表（小文件预读，大文件按需加载）
/// - search_match：搜索匹配元信息（仅搜索结果有值）
#[derive(Debug, Clone, Builder)]
#[builder(default)]
#[derive(Default)]
pub struct Skill {
    /// 数据库持久化元数据
    pub po: SkillPo,
    /// 关联文件列表
    pub files: Vec<SkillFile>,
    /// ✅ 搜索匹配元信息（可选）
    /// - 普通查询返回：None
    /// - 搜索返回：Some(包含匹配类型、距离、命中等元信息)
    pub search_match: Option<SearchMatchInfo>,
}

/// ✅ 实现 Vectorizable Trait（统一向量化行为）
impl Vectorizable for Skill {
    fn vectorize_text(&self) -> String {
        // Skill 向量化：名称 + 描述 + 标签
        let tags = self.po.parse_tags().join(" ");
        format!("{} {} {}", self.po.name, self.po.description, tags)
    }

    fn vector_collection() -> &'static str {
        "skills"
    }

    fn vector_id(&self) -> &str {
        self.po.vector_id()
    }

    fn vector_payload(&self) -> VectorPayload {
        self.po.vector_payload()
    }
}

/// ✅ SkillPo 也实现 Vectorizable（DAL 层直接使用 PO）
impl Vectorizable for SkillPo {
    fn vectorize_text(&self) -> String {
        // SkillPo 向量化：名称 + 描述 + 标签
        let tags = self.parse_tags().join(" ");
        format!("{} {} {}", self.name, self.description, tags)
    }

    fn vector_collection() -> &'static str {
        "skills"
    }

    fn vector_id(&self) -> &str {
        &self.id
    }

    fn vector_payload(&self) -> VectorPayload {
        VectorPayload {
            tags: Some(self.tags.clone()),
            entity_type: Some(self.category.clone()),
            ..Default::default()
        }
    }
}

/// Skill 文件导入统一结构 = (去哪放) × (从哪来)。
///
/// 所有外部来源（手填文本 / URL 下载 / 附件上传 / seed 快照）由 Handler 适配翻译
/// 为此结构，DAL 的内容摄取流水线统一处理。
///
/// 来源优先级：`source_abs_path` 路径优先 > `content_bytes` 内存内容。
///
/// 注意「路径优先」已**不再等于 0 拷贝 rename**：源文件通常是 Attachment / Artifact
/// 这类被 DB 行持有、且可被消息与任务引用的实体，导入一律 copy 并保留原位。
#[derive(Debug, Clone, Default)]
pub struct SkillFileImport {
    /// 最终在技能目录里的相对路径；None 时走 5 级降级推断链。
    ///
    /// 来源是 **zip 技能包**时语义不同：它表示「指定包内哪个条目为主文件」，
    /// 命中条目归位写成小写 `skill.md`；单文件来源里仍是落盘目标名。
    pub target_path: Option<String>,
    /// 来源 A：磁盘绝对路径（优先级高；导入时 copy 落盘，源文件保留原位）
    pub source_abs_path: Option<std::path::PathBuf>,
    /// 来源 B：内存内容 bytes（次优先级；无路径时用，省临时文件）
    pub content_bytes: Option<Vec<u8>>,
    /// 弱线索名（附件 original_name；target=None 时推断用）
    pub suggested_name: Option<String>,
    /// 显式可执行位（seed 快照往返用，保全无扩展名的二进制可执行附件）。
    ///
    /// None 时仍按扩展名推断（py/js/sh/ts）；Some(true/false) 直接覆盖推断结果。
    pub executable: Option<bool>,
}

/// 校验技能内文件相对路径的安全性：仅允许 `[a-zA-Z0-9._-/]`，
/// 禁止绝对路径、`..`、`\`、控制字符。
///
/// 白名单为什么必要：光挡 `..` 挡不住 `scripts/;rm -rf` 这类注入到 shell 的
/// 文件名——技能附件是要给 Agent 用 shell 工具执行的，文件名最终会进命令行，
/// 所以字符集必须收敛到「路径里真正可能用到的那几个」。
///
/// 字符集允许非 ASCII：中文附件名不是注入风险，而整包导入（seed 技能包）
/// 若因中文名被拒，整个技能就导入不了。
pub fn validate_skill_file_target(target_path: &str) -> common::error::Result<()> {
    use common::error::bail_err;

    if target_path.trim().is_empty() {
        bail_err!(InvalidRequest, "Skill 文件路径不能为空");
    }

    if !target_path
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'))
    {
        bail_err!(
            InvalidRequest,
            "Skill 文件路径含非法字符（仅允许字母数字与 . _ - /）: {}",
            target_path
        );
    }

    if target_path.contains("//") {
        bail_err!(
            InvalidRequest,
            "Skill 文件路径不能含连续斜杠: {}",
            target_path
        );
    }

    let path = Path::new(target_path);
    if path.is_absolute() {
        bail_err!(InvalidRequest, "Skill 文件路径不能是绝对路径");
    }
    if path.components().next().is_none() {
        bail_err!(InvalidRequest, "Skill 文件路径不能为空");
    }
    if target_path.contains('\\') {
        bail_err!(InvalidRequest, "Skill 文件路径不能包含反斜杠路径分隔符");
    }
    if target_path.ends_with('/') {
        bail_err!(InvalidRequest, "Skill 文件路径不能指向目录");
    }

    for component in path.components() {
        match component {
            std::path::Component::Normal(_) => {}
            std::path::Component::CurDir
            | std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => {
                bail_err!(InvalidRequest, "Skill 文件路径包含非法路径片段");
            }
        }
    }

    Ok(())
}

impl Skill {
    /// 从 PO 快速创建实体
    pub fn from_po(po: SkillPo) -> Self {
        Self {
            po,
            files: Vec::new(),
            search_match: None,
        }
    }

    /// 从 PO 快速创建实体（保留 from 别名用于兼容性）
    pub fn from(po: SkillPo) -> Self {
        Self::from_po(po)
    }

    /// 获取技能 ID
    pub fn id(&self) -> &str {
        &self.po.id
    }

    /// 获取技能名称
    pub fn name(&self) -> &str {
        &self.po.name
    }

    /// 获取主文件内容（如果已加载）
    ///
    /// 优先用 DAO 标注的 `is_main`：旧包的主文件名字可能不合 `skill.md` 的字面，
    /// 只看名字会把它们漏掉。手工构造（测试 / fixture）的实体没有该标记，
    /// 退化为按名字判定，两条路都成立。
    pub fn main_content(&self) -> Option<&str> {
        self.files
            .iter()
            .find(|f| f.is_main || is_main_skill_file(&f.filename))
            .and_then(|f| f.content.as_deref())
    }

    /// 获取指定文件名的内容（如果已加载）
    pub fn file_content(&self, filename: &str) -> Option<&str> {
        self.files
            .iter()
            .find(|f| {
                f.filename == filename
                    || (f.is_main || is_main_skill_file(&f.filename))
                        && is_main_skill_file(filename)
            })
            .and_then(|f| f.content.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 发布门槛：只有 Admin(1) / SuperAdmin(0) 通过。
    ///
    /// 最要紧的是两个「否」：`None`（无角色）与非法值必须拒绝——
    /// `UserRole::default()` 是 SuperAdmin，任何「unwrap_or_default」式写法都会
    /// 把匿名/未认证调用提权成超级管理员。
    #[test]
    fn can_publish_by_role_requires_admin_or_above() {
        assert!(can_publish_by_role(Some(0)), "SuperAdmin 应可发布");
        assert!(can_publish_by_role(Some(1)), "Admin 应可发布");
        assert!(!can_publish_by_role(Some(2)), "Member 不可发布");
        assert!(!can_publish_by_role(Some(99)), "非法角色按最小权限拒绝");
        assert!(!can_publish_by_role(None), "无角色不得默认成管理员");
    }

    fn po(description: &str) -> SkillPo {
        SkillPo {
            name: "记忆认知".to_string(),
            id: "skill-1".to_string(),
            description: description.to_string(),
            ..Default::default()
        }
    }

    /// 有「必读全文」分隔符 → 拆成「何时读 / 全文含」两行，触发条件可竖扫
    #[test]
    fn summary_splits_trigger_from_content() {
        let s = po("写记忆、建图谱前必读全文：四层记忆模型与图谱模式").to_prompt_summary();
        assert!(s.contains("⚠️ 读全文的时机：写记忆、建图谱前"), "{s}");
        assert!(s.contains("全文含：四层记忆模型与图谱模式"), "{s}");
        // 分隔符本身不应再出现在渲染结果里（已被语义标签取代）
        assert!(!s.contains(TRIGGER_SEPARATOR), "{s}");
    }

    /// 无分隔符 → 退化为原单行格式，不能渲染出错或丢内容
    #[test]
    fn summary_falls_back_to_single_line_without_trigger() {
        let s = po("通用接待 SOP：接待四步").to_prompt_summary();
        assert_eq!(s, "- 记忆认知（id: skill-1）：通用接待 SOP：接待四步");
    }

    /// 只有触发条件、没有内容概要 → 不渲染空的「全文含：」行
    #[test]
    fn summary_omits_empty_content_line() {
        let s = po("开始沉淀前必读全文").to_prompt_summary();
        assert!(s.contains("读全文的时机：开始沉淀前"), "{s}");
        assert!(!s.contains("全文含"), "{s}");
    }

    /// 分隔符在开头（无触发条件内容）→ 退化为单行，避免出现空标签
    #[test]
    fn summary_degrades_when_trigger_is_empty() {
        let s = po("必读全文：只有内容没有时机").to_prompt_summary();
        assert_eq!(s, "- 记忆认知（id: skill-1）：必读全文：只有内容没有时机");
    }

    fn names(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// 正主在（任意大小写）→ Main，且返回磁盘上的**原名**（读取侧要拿它去拼路径）
    #[test]
    fn pick_prefers_exact_main_and_keeps_original_name() {
        assert_eq!(
            pick_main_file(&names(&["skill.md", "README.md"])),
            MainFilePick::Main("skill.md")
        );
        assert_eq!(
            pick_main_file(&names(&["SKILL.md", "assets/a.png"])),
            MainFilePick::Main("SKILL.md"),
            "历史包里的 SKILL.md 必须被认出来，否则旧包永远读不到正文"
        );
        assert_eq!(
            pick_main_file(&names(&["assets/a.png", "Skill.MD"])),
            MainFilePick::Main("Skill.MD")
        );
    }

    /// 无正主 + 根目录恰好一个 md → Fallback（外部/手工包的常见形态）
    #[test]
    fn pick_falls_back_to_the_lone_root_markdown() {
        assert_eq!(
            pick_main_file(&names(&["main.md", "assets/note.md", "scripts/run.sh"])),
            MainFilePick::Fallback("main.md"),
            "子目录里的 md 不参与兜底"
        );
        assert_eq!(
            pick_main_file(&names(&["README.MD"])),
            MainFilePick::Fallback("README.MD"),
            "扩展名大小写同样要忽略"
        );
    }

    /// 多个根级 md → Ambiguous：读取侧不猜、导入侧报错
    #[test]
    fn pick_reports_ambiguity_instead_of_guessing() {
        assert_eq!(
            pick_main_file(&names(&["main.md", "README.md"])),
            MainFilePick::Ambiguous(vec!["main.md", "README.md"])
        );
        assert_eq!(
            pick_main_file(&names(&["sub/skill.md", "assets/a.png"])),
            MainFilePick::None,
            "子目录里的 skill.md 不是根引导文件，不能拿来当主文件"
        );
    }

    /// 零 md（纯附件包）→ None：往已有技能追加文件时合法
    #[test]
    fn pick_returns_none_for_assets_only() {
        assert_eq!(
            pick_main_file(&names(&["assets/logo.png", "scripts/run.sh"])),
            MainFilePick::None
        );
        assert_eq!(pick_main_file(&[]), MainFilePick::None);
    }
}
