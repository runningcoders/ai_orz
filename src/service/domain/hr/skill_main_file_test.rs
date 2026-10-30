//! 根引导文件（`skill.md`）识别契约测试（护栏）
//!
//! 背景：外部技能包（如 Anthropic Agent Skills 规范）的主文件常写作 **`SKILL.md`**，
//! macOS/Windows 文件系统本身又大小写不敏感，于是历史上出现过这类「半通不通」状态：
//! 包导入成功、磁盘上文件也在、`list_skill_files` 也列得出来，但 **`get_skill` 的
//! `content` 恒为空**——因为判定主文件身份的地方口径不一致（有的忽略大小写、
//! 有的严格 `==`），而 DAO 层 `read_main_content` 又按小写常量直拼路径。
//!
//! 本文件固化四条契约：
//! - ① 主文件身份识别**忽略大小写**，且全仓只有 `is_main_skill_file` 一处判定；
//! - ② 导入时把命中的主文件**归一化为小写 `skill.md` 落盘**，让新包的磁盘形态唯一；
//! - ③ 名字不叫 `skill.md` 的 md 文件**不认作主文件**（显式边界：将来若放宽为
//!   「根目录唯一 md 兜底当主文件」，需连带改这里）；
//! - ④ **读取侧按磁盘实况解析主文件**：归一化只作用于新写入的包，磁盘上早已装好的
//!   旧包（`SKILL.md` / `README.md`）不会自动改名，读取侧必须能认出来。
//!
//! ③④ 看似矛盾，实则互补且必须成对存在：只归一化不兼容 ⇒ 旧包永远读不到正文；
//! 只兼容不归一化 ⇒ 磁盘形态各异，任何拼串的地方都要再做一次解析。
//! 两者共用同一套挑选规则 `pick_main_file`，避免读写两侧口径分叉（本 bug 类的根因）。

use super::skill_test::{create_test_skill, init_test_env};
use super::{CreateSkillParams, SkillFileImport, UpdateSkillParams};
use crate::models::skill::{SKILL_MAIN_FILE, Skill, SkillPo, is_main_skill_file};
use crate::service::domain::system::seed::bundle::{SkillFileEntry, pack_skill_zip};
use common::enums::skill::SkillAuthorType;
use sqlx::SqlitePool;

const MAIN_BODY: &str = "---\ntitle: 诊断技能\ndescription: 来自包内 frontmatter 的描述\ntags: [alpha, beta]\ncategory: diag\n---\n\n# 正文\n这里是技能正文。\n";

/// 打一个只含主文件的技能包（文件名由调用方指定，用于验证大小写变体）。
fn package_with_main(main_name: &str) -> Vec<u8> {
    pack_skill_zip(&[SkillFileEntry {
        path: main_name.to_string(),
        bytes: MAIN_BODY.as_bytes().to_vec(),
        executable: false,
    }])
    .unwrap()
}

/// 纯函数口径：任何大小写变体都必须判定为主文件。
#[test]
fn main_file_identity_is_case_insensitive() {
    for variant in ["skill.md", "SKILL.md", "Skill.md", "SKILL.MD"] {
        assert!(
            is_main_skill_file(variant),
            "{variant} 应判定为主文件：外部包常用大写 SKILL.md"
        );
    }

    // 反向：子目录里的同名文件与别的 md 都不是根引导文件
    assert!(
        !is_main_skill_file("docs/skill.md"),
        "带目录前缀的同名文件不是根引导文件"
    );
    assert!(!is_main_skill_file("main.md"));
    assert!(!is_main_skill_file("notes.md"));
}

/// ①+② 包内主文件写作 `SKILL.md` 时：归一化落盘，`get_skill` 能直出正文。
#[sqlx::test]
async fn upper_case_main_file_is_normalized_and_readable(pool: SqlitePool) {
    let (domain, ctx, _temp) = init_test_env(pool.clone());
    let skill = create_test_skill("PkgUpperCase");

    domain
        .skill_manage()
        .create_skill(
            ctx.clone(),
            CreateSkillParams {
                skill: &skill,
                imports: vec![SkillFileImport {
                    target_path: None,
                    source_abs_path: None,
                    content_bytes: Some(package_with_main("SKILL.md")),
                    suggested_name: None,
                    executable: None,
                }],
                remote_source: None,
            },
        )
        .await
        .unwrap();

    let loaded = domain
        .skill_manage()
        .get_skill(ctx.clone(), skill.id())
        .await
        .unwrap()
        .unwrap();

    // 落盘名必须是小写常量：DAO 层 read_main_content 按它直拼路径
    let main = loaded
        .files
        .iter()
        .find(|f| is_main_skill_file(&f.filename))
        .expect("包内 SKILL.md 应被导入并识别为主文件");
    assert_eq!(
        main.filename, SKILL_MAIN_FILE,
        "主文件应归一化写成小写 skill.md，否则读取侧按常量拼路径会落空"
    );

    // 这正是用户报的症状：接口过去返回空，现在必须直出全文
    let content = loaded
        .main_content()
        .expect("get_skill 的 content 不应为空：主文件内容是接口的直出契约");
    assert!(content.contains("这里是技能正文。"), "正文必须完整可读");
}

/// ② 归一化后「按常量名读取」的通道也必须可用（Linux 大小写敏感 FS 上的关键回归）。
#[sqlx::test]
async fn normalized_main_file_readable_by_canonical_name(pool: SqlitePool) {
    let (domain, ctx, _temp) = init_test_env(pool.clone());
    let skill = create_test_skill("PkgCanonicalRead");

    domain
        .skill_manage()
        .create_skill(
            ctx.clone(),
            CreateSkillParams {
                skill: &skill,
                imports: vec![SkillFileImport {
                    target_path: None,
                    source_abs_path: None,
                    content_bytes: Some(package_with_main("SKILL.MD")),
                    suggested_name: None,
                    executable: None,
                }],
                remote_source: None,
            },
        )
        .await
        .unwrap();

    let content = domain
        .skill_manage()
        .get_skill_file_content(ctx.clone(), skill.id(), SKILL_MAIN_FILE)
        .await
        .unwrap()
        .unwrap();
    assert!(
        content.contains("这里是技能正文。"),
        "按 canonical 名 skill.md 必须能读到正文"
    );
}

/// ③ zip 包根目录没有 `skill.md` 时，**唯一的** `.md` 被兜底认作主文件并归位。
///
/// 这是外部/手工包的常见形态（主文件叫 `main.md` / `README.md`）。
#[sqlx::test]
async fn zip_fallback_picks_lone_markdown_as_main(pool: SqlitePool) {
    let (domain, ctx, _temp) = init_test_env(pool.clone());
    let skill = create_test_skill("PkgFallback");

    let zip_bytes = pack_skill_zip(&[
        SkillFileEntry {
            path: "main.md".to_string(),
            bytes: MAIN_BODY.as_bytes().to_vec(),
            executable: false,
        },
        SkillFileEntry {
            path: "assets/note.md".to_string(),
            bytes: "# 子目录 md 不参与兜底\n".as_bytes().to_vec(),
            executable: false,
        },
    ])
    .unwrap();

    domain
        .skill_manage()
        .create_skill(
            ctx.clone(),
            CreateSkillParams {
                skill: &skill,
                imports: vec![SkillFileImport {
                    target_path: None,
                    source_abs_path: None,
                    content_bytes: Some(zip_bytes),
                    suggested_name: None,
                    executable: None,
                }],
                remote_source: None,
            },
        )
        .await
        .unwrap();

    let loaded = domain
        .skill_manage()
        .get_skill(ctx.clone(), skill.id())
        .await
        .unwrap()
        .unwrap();

    let content = loaded
        .main_content()
        .expect("根目录唯一的 main.md 应被兜底认作主文件并归位");
    assert!(
        content.contains("这里是技能正文。")
            && loaded.files.iter().all(|f| f.filename != "main.md"),
        "兜底认主后应改名归位为小写 skill.md，磁盘上不再保留原名（否则读取侧按常量拼路径仍会落空）"
    );
    // 子目录 md 不参与兜底，也不该被误伤
    assert!(
        loaded.files.iter().any(|f| f.filename == "assets/note.md"),
        "子目录 md 不参与主文件兜底，应保持原位"
    );
}

/// ③ 边界：根级有多个 `.md` 且没有正主时**报错要求提供 skill.md**。
///
/// 歧义是打包方的信息缺失，必须在入口说清——静默降级会重演「导入成功却读不到正文」
/// 的老毛病（调用方拿到成功响应，只能事后排查）。
#[sqlx::test]
async fn ambiguous_root_markdowns_rejected_with_guidance(pool: SqlitePool) {
    let (domain, ctx, _temp) = init_test_env(pool.clone());
    let skill = create_test_skill("PkgAmbiguous");

    let zip_bytes = pack_skill_zip(&[
        SkillFileEntry {
            path: "main.md".to_string(),
            bytes: "# 候选一\n".as_bytes().to_vec(),
            executable: false,
        },
        SkillFileEntry {
            path: "README.md".to_string(),
            bytes: "# 候选二\n".as_bytes().to_vec(),
            executable: false,
        },
    ])
    .unwrap();

    let err = domain
        .skill_manage()
        .create_skill(
            ctx.clone(),
            CreateSkillParams {
                skill: &skill,
                imports: vec![SkillFileImport {
                    target_path: None,
                    source_abs_path: None,
                    content_bytes: Some(zip_bytes),
                    suggested_name: None,
                    executable: None,
                }],
                remote_source: None,
            },
        )
        .await
        .expect_err("多个根级 md 无法确定主文件时必须报错");

    let msg = err.to_string();
    assert!(
        msg.contains(SKILL_MAIN_FILE),
        "报错必须指明要提供哪个文件名，实际: {msg}"
    );
    assert!(
        msg.contains("main.md") && msg.contains("README.md"),
        "报错应列出冲突候选，便于打包方定位，实际: {msg}"
    );

    // 且必须原子：失败后不应留下半成品技能记录
    assert!(
        domain
            .skill_manage()
            .get_skill(ctx.clone(), skill.id())
            .await
            .unwrap()
            .is_some(),
        "本次解包发生在 create 之后，技能行存在但不应带文件"
    );
    let files = domain
        .skill_manage()
        .list_skill_files(ctx.clone(), skill.id())
        .await
        .unwrap()
        .unwrap_or_default();
    assert!(
        files.is_empty(),
        "歧义包应整体拒绝，不应落任何半成品文件，实际: {files:?}"
    );
}

/// ③ 边界：纯附件包（零个 md）不要求主文件——用于往已有技能追加文件时必须放行。
#[sqlx::test]
async fn assets_only_package_is_allowed(pool: SqlitePool) {
    let (domain, ctx, _temp) = init_test_env(pool.clone());
    let skill = create_test_skill("PkgAssetsOnly");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();

    let zip_bytes = pack_skill_zip(&[SkillFileEntry {
        path: "assets/logo.png".to_string(),
        bytes: vec![0x89, 0x50, 0x4E, 0x47],
        executable: false,
    }])
    .unwrap();

    domain
        .skill_manage()
        .update_skill(
            ctx.clone(),
            UpdateSkillParams {
                skill: &skill,
                imports: vec![SkillFileImport {
                    target_path: None,
                    source_abs_path: None,
                    content_bytes: Some(zip_bytes),
                    suggested_name: None,
                    executable: None,
                }],
                file_deletes: Vec::new(),
                remote_source: None,
            },
        )
        .await
        .expect("纯附件包不应因缺少主文件被拒");

    let files = domain
        .skill_manage()
        .list_skill_files(ctx.clone(), skill.id())
        .await
        .unwrap()
        .unwrap();
    assert!(
        files.iter().any(|f| f.filename == "assets/logo.png"),
        "附件应正常落盘，实际: {files:?}"
    );
}

/// 直接在技能目录里摆一组「历史遗留」文件：**绕过导入侧**，模拟磁盘上早已存在的旧包。
///
/// 为什么不能走 create_skill 导入来构造：导入侧会归一化主文件名，造出来的永远是
/// 小写 `skill.md`，覆盖不到「旧包名字不合现行契约」这一档场景。
fn lay_legacy_files(temp: &std::path::Path, content_path: &str, files: &[(&str, &str)]) {
    let dir = temp.join(content_path);
    std::fs::create_dir_all(&dir).unwrap();
    for (name, body) in files {
        let target = dir.join(name);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(target, body).unwrap();
    }
}

/// ④ 旧包主文件写作 `SKILL.md`（Linux 大小写敏感 FS 上的必现缺陷）：正文必须可读。
#[sqlx::test]
async fn legacy_upper_case_main_file_is_readable(pool: SqlitePool) {
    let (domain, ctx, temp) = init_test_env(pool.clone());
    let skill = create_test_skill("LegacyUpperCase");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();
    lay_legacy_files(
        temp.path(),
        &skill.po.content_path,
        &[("SKILL.md", "# 旧包正文\n"), ("assets/logo.png", "png")],
    );

    let loaded = domain
        .skill_manage()
        .get_skill(ctx.clone(), skill.id())
        .await
        .unwrap()
        .unwrap();

    let main = loaded
        .files
        .iter()
        .find(|f| f.filename == "SKILL.md")
        .expect("旧包文件应照原名列出来");
    assert!(main.is_main, "旧包主文件必须被标为主文件，否则正文取不到");
    assert!(
        loaded
            .main_content()
            .unwrap_or_default()
            .contains("旧包正文"),
        "旧包 SKILL.md 的正文必须直出"
    );
    // 「按 canonical 名读」这条通道同样要成立（Agent 只会按 skill.md 这个名字来读）
    let by_canonical = domain
        .skill_manage()
        .get_skill_file_content(ctx.clone(), skill.id(), SKILL_MAIN_FILE)
        .await
        .unwrap()
        .unwrap();
    assert!(by_canonical.contains("旧包正文"));
}

/// ④ 旧包没有 `skill.md`、根目录只有一个 md（手工包的常见形态）→ 读取侧兜底认主。
#[sqlx::test]
async fn legacy_lone_markdown_is_read_as_main(pool: SqlitePool) {
    let (domain, ctx, temp) = init_test_env(pool.clone());
    let skill = create_test_skill("LegacyLoneMd");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();
    lay_legacy_files(
        temp.path(),
        &skill.po.content_path,
        &[
            ("README.md", "# 合集说明\n"),
            ("assets/note.md", "# 子目录 md 不参与兜底\n"),
        ],
    );

    let loaded = domain
        .skill_manage()
        .get_skill(ctx.clone(), skill.id())
        .await
        .unwrap()
        .unwrap();
    assert!(
        loaded
            .main_content()
            .unwrap_or_default()
            .contains("合集说明"),
        "旧包里唯一的根级 md 应被兜底认作主文件"
    );

    let listed = domain
        .skill_manage()
        .list_skill_files(ctx.clone(), skill.id())
        .await
        .unwrap()
        .unwrap();
    let marked_main = |name: &str| listed.iter().any(|f| f.filename == name && f.is_main);
    assert!(marked_main("README.md"), "命中的那个才是主文件");
    assert!(
        !marked_main("assets/note.md"),
        "子目录 md 不参与兜底，不应被标为主文件"
    );
}

/// ④ 旧包根目录有多个 md → 读取侧**不猜**（无正文），也不得报错。
#[sqlx::test]
async fn legacy_ambiguous_main_files_are_not_guessed(pool: SqlitePool) {
    let (domain, ctx, temp) = init_test_env(pool.clone());
    let skill = create_test_skill("LegacyAmbiguous");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();
    lay_legacy_files(
        temp.path(),
        &skill.po.content_path,
        &[("main.md", "# 候选一\n"), ("README.md", "# 候选二\n")],
    );

    let loaded = domain
        .skill_manage()
        .get_skill(ctx.clone(), skill.id())
        .await
        .unwrap()
        .unwrap();
    assert!(
        loaded.main_content().is_none(),
        "读取侧不能在多个候选里静默猜一个是主文件；宁可无正文，也不猜错"
    );
    assert_eq!(
        loaded.files.len(),
        2,
        "文件本身照原样列出，只是都不承担正文角色"
    );
}

/// ④ 边界：旧包 `SKILL.md` 存在时再写正文，磁盘必须收敛成唯一主文件。
///
/// 不收敛的代价：两个文件都会被判成主文件，而 `main_content()` 取排序第一个
/// （大写排在前）⇒ 用户改完正文读到的还是**旧内容**。
#[sqlx::test]
async fn writing_main_content_collapses_legacy_case_variant(pool: SqlitePool) {
    let (domain, ctx, temp) = init_test_env(pool.clone());
    let skill = create_test_skill("LegacyRewrite");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();
    lay_legacy_files(
        temp.path(),
        &skill.po.content_path,
        &[("SKILL.md", "# 旧正文\n")],
    );

    // 用大写名写入也应收敛到 canonical 名，而不是再造一个变体
    domain
        .skill_manage()
        .update_skill_file_content(ctx.clone(), skill.id(), "SKILL.md", "# 新正文\n", None)
        .await
        .unwrap();

    let dir = temp.path().join(&skill.po.content_path);
    let names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(
        names,
        vec![SKILL_MAIN_FILE.to_string()],
        "主文件在磁盘上必须唯一且恒为小写 skill.md，实际: {names:?}"
    );

    let loaded = domain
        .skill_manage()
        .get_skill(ctx.clone(), skill.id())
        .await
        .unwrap()
        .unwrap();
    assert!(
        loaded.main_content().unwrap_or_default().contains("新正文"),
        "写完之后读到的必须是新正文（残留变体若还在，会读到旧的那个）"
    );
}

/// ⑤ 边界固化：绕开 zip 的单文件导入里，显式 `target_path` 非 skill.md 时不改其名。
///
/// 兜底规则只作用于 **zip 包**（包结构由打包方决定）；单文件场景里调用方既然明确
/// 指定了目标名，就不该被偷偷改写。
///
/// 注意与③的关系：写侧不改其名，但读侧仍按「根目录唯一的 md 即主文件」认它
/// （`pick_main_file`）——两侧同一套口径，否则同一次导入会有两种解读。
#[sqlx::test]
async fn explicit_target_path_is_not_rewritten(pool: SqlitePool) {
    let (domain, ctx, _temp) = init_test_env(pool.clone());
    let skill = create_test_skill("PkgOtherName");

    domain
        .skill_manage()
        .create_skill(
            ctx.clone(),
            CreateSkillParams {
                skill: &skill,
                imports: vec![SkillFileImport {
                    target_path: Some("main.md".to_string()),
                    source_abs_path: None,
                    content_bytes: Some("# 另一种主文件命名\n".as_bytes().to_vec()),
                    suggested_name: None,
                    executable: None,
                }],
                remote_source: None,
            },
        )
        .await
        .unwrap();

    let loaded = domain
        .skill_manage()
        .get_skill(ctx.clone(), skill.id())
        .await
        .unwrap()
        .unwrap();

    // 写侧不改其名
    assert!(
        loaded.files.iter().any(|f| f.filename == "main.md"),
        "显式指定的目标名应保持原样，不被偷偷改写"
    );
    // 读侧仍按同一套口径认它：根目录唯一的 md 就是主文件（否则写/读两解）
    assert!(
        loaded
            .main_content()
            .unwrap_or_default()
            .contains("另一种主文件命名"),
        "根目录唯一的 md 应被读侧认作主文件并直出正文"
    );
}

/// ④ 元信息回写：PO 字段为空时，从主文件 frontmatter 补齐并落库。
///
/// 这是「包导入后是否会据主文件建立技能信息」的答案：**会，但只补空字段**——
/// 调用方显式给了的值不被覆盖。技能正文从不入库（`skills` 表无 content 列），
/// 入库的是 name / description / tags / category 这些元信息。
#[sqlx::test]
async fn frontmatter_fills_empty_skill_fields(pool: SqlitePool) {
    let (domain, ctx, _temp) = init_test_env(pool.clone());

    let skill = Skill::from_po(SkillPo::new(
        "frontmatter-fill--1".to_string(),
        String::new(), // name 留空 → 期望被 title 补
        String::new(), // description 留空 → 期望被 description 补
        Vec::new(),    // tags 留空
        String::new(), // category 留空
        String::new(),
        "admin".to_string(),
        SkillAuthorType::User,
        "skills/frontmatter-fill".to_string(),
    ));

    domain
        .skill_manage()
        .create_skill(
            ctx.clone(),
            CreateSkillParams {
                skill: &skill,
                imports: vec![SkillFileImport {
                    target_path: None,
                    source_abs_path: None,
                    content_bytes: Some(package_with_main("SKILL.md")),
                    suggested_name: None,
                    executable: None,
                }],
                remote_source: None,
            },
        )
        .await
        .unwrap();

    let loaded = domain
        .skill_manage()
        .get_skill(ctx.clone(), skill.id())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(loaded.po.name, "诊断技能", "frontmatter title 应回写 name");
    assert_eq!(
        loaded.po.description, "来自包内 frontmatter 的描述",
        "frontmatter description 应回写"
    );
    assert_eq!(loaded.po.category, "diag", "frontmatter category 应回写");
    assert!(
        loaded.po.parse_tags().contains(&"alpha".to_string()),
        "frontmatter tags 应回写，实际: {:?}",
        loaded.po.parse_tags()
    );
}
