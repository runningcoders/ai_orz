//! Skill 内容源摄取：一次调用完成「元数据行 + 文件系统 + 向量库」三处写入
//!
//! 为什么这层在 DAL 而不在 Domain：一个技能实体横跨三个存储，写它们的**顺序**
//! 本身就是数据层契约（向量必须读得到已落盘的正文）。放在 Domain 里意味着
//! 「写完正文要记得回来补向量」只能靠每个入口自觉，且 `create` 阶段必然先用
//! 「尚未落盘的正文」算一次废向量。合并进 DAL 后顺序只有一处定义，
//! 向量只算一次。
//!
//! Domain 保留的仍是业务规则：资源级权限、`Published` 跃迁拦截、发布独立入口。

use super::SkillDalImpl;
use crate::models::skill::{
    MainFilePick, SKILL_MAIN_FILE, Skill, SkillFileImport, SkillPo, is_main_skill_file,
    pick_main_file, root_markdown_names, validate_skill_file_target,
};
use crate::pkg::RequestContext;
use crate::pkg::utils::fetch_remote_content::{FetchOptions, fetch_remote_content};
use crate::pkg::utils::fs_perm::{is_script_filename, mark_executable};
use common::constants::utils::current_timestamp_ms;
use common::error::{Result, bail_err, err};
use std::path::Path;

impl SkillDalImpl {
    /// [`SkillDal::create_with_content`] 的实现体（trait 侧由 `mod.rs` 的宏转发到此）。
    ///
    /// 拆成固有方法只因为 trait impl 必须集中一处（E0119）；顺序契约全部在这里。
    pub(super) async fn create_with_content(
        &self,
        ctx: RequestContext,
        po: SkillPo,
        imports: &[SkillFileImport],
        remote_source: Option<&str>,
    ) -> Result<SkillPo> {
        // 1. 文件先落盘。技能目录由 `po.content_path` 推导，**不依赖数据库行**，
        //    所以可以先把「重活」干完，再让行落库成为唯一的提交点。
        if let Err(e) = self.ingest_content(&po, imports, remote_source).await {
            // 行还没写，磁盘上可能已落了半包文件：整目录回滚（目录在创建前不存在，
            // 删掉是安全的）。这比「先写行、失败再软删」干净——`delete` 是软删除，
            // 会留下一个可被 `restore_skill` 复活的空技能。
            let _ = self.skill_dao.delete_skill_dir(&po);
            return Err(e);
        }

        // 2. frontmatter 合并（纯计算，不写库）：正文已在磁盘上才读得到。
        let effective = self.merge_frontmatter(&po);

        // 3. 行落库 = 提交点。失败时回滚磁盘，绝不留「有目录、无行」的孤儿。
        if let Err(e) = self.skill_dao.insert(ctx.clone(), &effective).await {
            let _ = self.skill_dao.delete_skill_dir(&effective);
            return Err(e);
        }

        // 4. 向量唯一一次：此时正文与元信息都已定型。
        self.reindex_vector(ctx, &effective).await?;
        Ok(effective)
    }

    pub(super) async fn update_with_content(
        &self,
        ctx: RequestContext,
        skill: Skill,
        imports: &[SkillFileImport],
        remote_source: Option<&str>,
        file_deletes: &[&str],
    ) -> Result<SkillPo> {
        // 1. 内容源摄取（覆盖写 + 追加）。注意**不能**在失败时删目录：
        //    这是更新，目录里有用户既有文件。
        self.ingest_content(&skill.po, imports, remote_source)
            .await?;

        // 2. frontmatter 合并（纯计算）
        let effective = self.merge_frontmatter(&skill.po);

        // 3. 行落库。不走 `self.update()`：那条路会立刻按「新元信息 + 磁盘上的旧正文」
        //    算一次向量，等正文写完再算第二次，第一次纯属浪费。
        self.skill_dao.update(ctx.clone(), &effective).await?;

        // 4. 文件删除：放在落盘之后、算向量之前，保证向量看到的是最终文件集合。
        for filename in file_deletes {
            self.skill_dao.delete_file(&effective, filename)?;
        }

        // 5. 向量唯一一次
        self.reindex_vector(ctx, &effective).await?;
        Ok(effective)
    }
}

impl SkillDalImpl {
    /// 内容源处理流水线（create / update 共用）。
    ///
    /// 1. URL 下载到 tmp → 合并进 imports
    /// 2. 统一 `ingest_package`（zip 解包 / 5 级 target 推断 / 单文件 copy / bytes 直写）
    ///
    /// frontmatter 合并不在这里：那是纯计算（[`Self::merge_frontmatter`]），
    /// 由调用方写在「行落库」那一步，避免同一份 PO 被 UPDATE 两次。
    async fn ingest_content(
        &self,
        po: &SkillPo,
        imports: &[SkillFileImport],
        remote_source: Option<&str>,
    ) -> Result<()> {
        let mut all = imports.to_vec();
        if let Some(url) = remote_source {
            let result = fetch_remote_content(url, &FetchOptions::default()).await?;
            let tmp_path =
                std::env::temp_dir().join(format!("skill_import_{}.tmp", current_timestamp_ms()));
            std::fs::write(&tmp_path, &result.bytes)?;
            all.push(SkillFileImport {
                target_path: None,
                source_abs_path: Some(tmp_path),
                content_bytes: None,
                suggested_name: None,
                executable: None,
            });
        }

        self.ingest_package(po, &all)
    }

    /// 从最终写入的 `skill.md` 提取 YAML frontmatter，若 PO 字段为空/默认则覆盖。
    ///
    /// 读小写常量即可命中：导入侧已把任何大小写的主文件归一化成 `skill.md`。
    /// 读取失败（技能确实没有主文件）不影响创建结果，静默跳过补字段。
    ///
    /// **纯计算，不写库**：返回生效后的 PO，由调用方的「行落库」一次性写入。
    /// 上一版在这里 UPDATE 一次、调用方再 UPDATE 一次，同一份 PO 写两遍。
    ///
    /// 返回的 PO 必须用于后面的向量重建，否则会用「补齐前」的旧元信息覆盖刚写入的向量。
    fn merge_frontmatter(&self, po: &SkillPo) -> SkillPo {
        let Ok(md_content) = self.skill_dao.read_file(po, SKILL_MAIN_FILE) else {
            return po.clone();
        };
        let Some(fm) = parse_frontmatter(&md_content) else {
            return po.clone();
        };

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
        }
        updated_po
    }

    /// 统一处理技能文件包导入（PATH 分支 + BYTES 分支 + zip 解包 + 5 级 target 推断）。
    ///
    /// 全程带 [`ImportBudget`] 累计校验：文件数与合计字节在**写入过程中**累加，
    /// 而非只看请求体——zip 解包后的条目数远大于请求里的 1 个 URL/附件，
    /// 只校验入口会让「1 个 zip 塞 5000 个文件」畅通无阻。
    fn ingest_package(&self, po: &SkillPo, imports: &[SkillFileImport]) -> Result<()> {
        let mut budget = ImportBudget::default();
        for import in imports {
            if import.source_abs_path.is_some() {
                self.ingest_path_source(po, import, &mut budget)?;
            } else if import.content_bytes.is_some() {
                self.ingest_bytes_source(po, import, &mut budget)?;
            } else {
                bail_err!(
                    InvalidRequest,
                    "技能文件无内容来源（source_abs_path 和 content_bytes 均为空）"
                );
            }
        }
        Ok(())
    }

    /// PATH 分支：源文件在磁盘上。
    ///
    /// zip 源 → 逐条目解包写盘；单文件 → copy 落盘且**保留源文件**
    /// （Attachment / Artifact 是被 DB 持有的引用实体，不能被导入动作搬空）。
    fn ingest_path_source(
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
            self.unpack_zip_archive(po, &mut archive, budget, declared_main_for_package(import))?;
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

            if !is_main_skill_file(&target) {
                validate_skill_file_target(&target)?;
            }
            let target_abs = self.skill_dao.file_abs_path(po, &target);
            if let Some(parent) = target_abs.parent() {
                std::fs::create_dir_all(parent)?;
            }
            // 一律 copy，源文件必须留在原位。
            //
            // 历史实现先 try rename（当时称「0 拷贝」）、失败降级 copy + remove_file(src_path)：
            // 两条路**都会把源文件吃掉**。但 Attachment / Artifact 都是有 DB 行、且可被消息
            // 与任务引用的实体，导入技能不该把它们搬空——否则同一来源二次导入、或后续
            // 再读该附件/产物都会直接落空。体量已被 ImportBudget（≤10MB）封顶，
            // copy 的额外成本可忽略，安全优先于这一次省下的 rename。
            std::fs::copy(src_path, &target_abs)?;
            self.apply_executable_bit(&target_abs, &target, import.executable);
        }
        Ok(())
    }

    /// BYTES 分支：源内容在内存里，直接 write。
    fn ingest_bytes_source(
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
            self.unpack_zip_archive(po, &mut archive, budget, declared_main_for_package(import))?;
        } else {
            let target = resolve_target(
                &import.target_path,
                None,
                Some(bytes),
                &import.suggested_name,
            )?;
            budget.record(bytes.len() as u64)?;

            let is_main = is_main_skill_file(&target);
            if !is_main {
                validate_skill_file_target(&target)?;
            }
            if is_main {
                let content = String::from_utf8_lossy(bytes);
                self.skill_dao.write_file(po, &target, &content)?;
            } else {
                self.skill_dao.write_file_bytes(po, &target, bytes)?;
                let abs = self.skill_dao.file_abs_path(po, &target);
                self.apply_executable_bit(&abs, &target, import.executable);
            }
        }
        Ok(())
    }

    /// zip 解包：主文件必须在根目录（或由 `declared_main` 指定），其他文件逐一校验+写入。
    ///
    /// `declared_main` = 调用方 `target_path` 里**真正的指定意图**，由
    /// [`declared_main_for_package`] 过滤掉「包名回显」后给出：对**包**这个字段不再
    /// 表示「落盘到哪」，而是「包内哪个条目是主文件」（单文件导入里仍是目标名，见
    /// `resolve_target`）。有了它，多 md 的歧义包不必再硬失败——上一版只能报错要求
    /// 补 `skill.md`，对别人打好的包无从下手。
    fn unpack_zip_archive<R: std::io::Read + std::io::Seek>(
        &self,
        po: &SkillPo,
        archive: &mut zip::ZipArchive<R>,
        budget: &mut ImportBudget,
        declared_main: Option<&str>,
    ) -> Result<()> {
        // 先整体登记条目名，用于在写盘**之前**决定哪个是主文件——
        // 一旦按原名写下去就来不及归位了（返回值要去写盘分支里改名）。
        let mut names: Vec<String> = Vec::with_capacity(archive.len());
        for i in 0..archive.len() {
            let name = archive
                .by_index(i)
                .map_err(|e| err!(InvalidRequest, "zip 读取条目失败: {}", e))?
                .name()
                .to_string();
            if !name.ends_with('/') {
                names.push(name);
            }
        }
        // 调用方显式指定时，用它消歧；没指定才走「正主 → 唯一 md 兜底 → 歧义报错」
        let designated = declared_main
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(|declared| resolve_declared_main(&names, declared))
            .transpose()?;
        let fallback_main = if designated.is_some() {
            None
        } else {
            pick_fallback_main(&names)?
        };

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

            // 主文件必须在 zip 根目录：带目录前缀说明包结构不对，落盘后读取侧
            // 按根目录解析主文件（见 pick_main_file）找不到它（大小写同理，故忽略
            // 大小写判定）。
            if name.contains('/') && is_main_skill_file(name.rsplit('/').next().unwrap_or_default())
            {
                bail_err!(
                    InvalidRequest,
                    "skill.md 必须在 zip 根目录，不应有目录前缀: {}",
                    name
                );
            }

            budget.record(file.size())?;

            // 忽略大小写识别主文件、兜底认主的那个文件、或调用方指定的那个条目，
            // 并归一化写成小写 skill.md（见 normalize_main_target）
            let is_main = designated.as_deref() == Some(name.as_str())
                || fallback_main.as_deref() == Some(name.as_str())
                || is_main_skill_file(&name);
            if is_main {
                let mut buf = Vec::new();
                std::io::Read::read_to_end(&mut file, &mut buf)
                    .map_err(|e| err!(InvalidRequest, "zip skill.md 读取失败: {}", e))?;
                let content = String::from_utf8_lossy(&buf);
                self.skill_dao.write_file(po, SKILL_MAIN_FILE, &content)?;
            } else {
                validate_skill_file_target(&name)?;
                let mut buf = Vec::new();
                std::io::Read::read_to_end(&mut file, &mut buf)
                    .map_err(|e| err!(InvalidRequest, "zip 条目 {} 读取失败: {}", name, e))?;
                self.skill_dao.write_file_bytes(po, &name, &buf)?;
                if zip_executable || is_script_filename(&name) {
                    mark_executable(&self.skill_dao.file_abs_path(po, &name));
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

/// 取包来源的「指定主文件」；`target_path` 若只是**包自身文件名的回显**，视同未指定。
///
/// 前端上传时用 `default_target_path(文件名)` 预填 `target_path`，而 zip 的扩展名不在
/// 归位映射表里 ⇒ 原样回填**包名**（`pkg.zip`）。包名不可能是包内条目，照直当「指定
/// 主文件」用的话，每个从前端上传的包都会撞「指定的主文件不在包内」——把界面的预填
/// 回显当成指令，等于让 UI 的省事变成后端的硬错误。
///
/// 判据只认「指定名 == 包名」（忽略大小写）：真要指定包内条目，写的一定是条目名
/// （`main.md`），不会与包名相同。
fn declared_main_for_package(import: &SkillFileImport) -> Option<&str> {
    let declared = import
        .target_path
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())?;
    if import
        .suggested_name
        .as_deref()
        .is_some_and(|pkg| pkg.trim().eq_ignore_ascii_case(declared))
    {
        return None;
    }
    Some(declared)
}

/// 解析调用方在 `target_path` 里**指定的包内主文件**，返回它在 zip 里的条目名。
///
/// 三条硬校验（任一不满足即报错，绝不静默降级）：
/// 1. 包内必须真有这个条目（忽略大小写匹配，外部包常写 `SKILL.md` 而调用方写小写）；
/// 2. 必须在 zip 根目录——落盘后读取侧只扫根目录（见 `pick_main_file`），
///    子目录里的文件永远当不了主文件；
/// 3. 不能与包内已有的 `skill.md` 冲突：两者并存时旧的那个会被当附件落盘、却仍被
///    判成主文件（身份判定忽略大小写），`main_content()` 取排序靠前的大写 ⇒ 读到
///    旧内容。冲突是调用方的信息，必须由调用方解决，不能替他猜。
///
/// 命中的条目随后会被**归位写成小写 `skill.md`**（与唯一 md 兜底同口径）：保留原名
/// 的话，包里还有其他 md 时读取侧会判成歧义，指定了也照样读不到正文。
fn resolve_declared_main(names: &[String], declared: &str) -> Result<String> {
    let hit = names
        .iter()
        .find(|n| n.as_str() == declared || n.eq_ignore_ascii_case(declared))
        .cloned();
    let Some(hit) = hit else {
        let candidates = root_markdown_names(names);
        let hint = if candidates.is_empty() {
            "（包内根目录没有任何 Markdown 文件）".to_string()
        } else {
            format!("（包内根目录 md：{}）", candidates.join("、"))
        };
        bail_err!(
            InvalidRequest,
            "target_path 指定的主文件 {} 不在包内{}",
            declared,
            hint
        );
    };

    if hit.contains('/') {
        bail_err!(
            InvalidRequest,
            "target_path 指定的主文件必须在 zip 根目录，不应有目录前缀: {}",
            hit
        );
    }

    if let Some(existing) = names.iter().find(|n| is_main_skill_file(n))
        && *existing != hit
    {
        bail_err!(
            InvalidRequest,
            "包内已存在主文件 {}，不能再指定 {} 为主文件：请删掉包里的 {}，或去掉 target_path",
            existing,
            declared,
            existing
        );
    }

    Ok(hit)
}

/// 判定 zip 包的主文件取哪个条目；无法确定时**明确报错**要求提供正主。
///
/// 三档规则（顺序重要）：
/// 1. 根目录有 `skill.md`（忽略大小写）→ 它就是正主，无需兜底；
/// 2. 没有正主、但根目录**恰好只有一个** `.md` → 兜底认它为主文件并归位；
/// 3. 没有正主、且根目录有**多个** `.md` → 歧义不可自动解，**报错要求提供 `skill.md`**。
///
/// 第 2 档存在理由：外部/手工打的技能包主文件未必叫 `skill.md`（常见 `main.md`、
/// `README.md`）。不作兜底的话，包能导入成功、文件也在磁盘上，但 `get_skill` 的
/// 正文恒为空，只能按路径去 read_file——现象很像一个 bug，实则只是命名不合契约。
///
/// 第 3 档为什么不退回「不认主文件」：那种静默降级正是「导入成功却读到空正文」
/// 的成因。歧义是**打包方的信息缺失**，该由打包方在入口得到明确报错并补上
/// `skill.md`，不该让读取侧沉默兜着。
///
/// 「零个 md」不在此列（返回 `Ok(None)`）：纯附件包（如只含 `assets/` 的更新包）
/// 用于往已有技能追加文件时完全合法，强加主文件要求会误伤。
///
/// 同一套规则也用于**读取侧**（DAO `resolve_main_file`）：写入侧的归一化够不到早已
/// 装好的旧包，读取侧必须能按磁盘实况认出主文件。
fn pick_fallback_main(names: &[String]) -> Result<Option<String>> {
    // 口径与读取侧同源（`pick_main_file`）：新包按它归位落盘、旧包按它认出主文件，
    // 两边分叉正是「导入成功却读不到正文」的成因。
    match pick_main_file(names) {
        // 已有正主（任意大小写）→ 写盘分支会归一化，无需兜底
        MainFilePick::Main(_) => Ok(None),
        MainFilePick::Fallback(only) => Ok(Some(only.to_string())),
        MainFilePick::None => Ok(None), // 纯附件包：不要求主文件
        MainFilePick::Ambiguous(many) => bail_err!(
            InvalidRequest,
            "技能包根目录存在多个 Markdown 文件（{}），无法确定哪个是主文件：\
             请用 target_path 指定包内哪个文件是主文件，或在 zip 根目录显式提供 {}（其余说明文档请放到子目录）",
            many.join("、"),
            SKILL_MAIN_FILE
        ),
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
    Ok(normalize_main_target(&resolve_target_inner(
        target_path,
        source_abs_path,
        content_bytes,
        suggested_name,
    )?))
}

/// 命中主文件身份时归一化为小写 `skill.md`。
///
/// 外部包常写 `SKILL.md`、用户显式 target_path 也可能写成任意大小写；写入侧若不
/// 收敛，磁盘上就会出现大小写各异的主文件，而 macOS 文件系统本身不敏感，同一份
/// 操作在 macOS 正常、在 Linux 才暴露，极易漏测。
///
/// 注意归一化只覆盖 **本条写入**：磁盘上早已装好的旧包不会跟着改名，所以读取侧
/// 另有兼容逻辑（DAO `resolve_main_file` 按磁盘实况解析），两者互补而非替代。
fn normalize_main_target(target: &str) -> String {
    if is_main_skill_file(target) {
        SKILL_MAIN_FILE.to_string()
    } else {
        target.to_string()
    }
}

fn resolve_target_inner(
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
        return Ok(SKILL_MAIN_FILE.to_string());
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
