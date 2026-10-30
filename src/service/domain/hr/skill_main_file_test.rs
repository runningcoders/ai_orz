//! 根引导文件（`skill.md`）识别契约测试（护栏）
//!
//! 背景：外部技能包（如 Anthropic Agent Skills 规范）的主文件常写作 **`SKILL.md`**，
//! macOS/Windows 文件系统本身又大小写不敏感，于是历史上出现过这类「半通不通」状态：
//! 包导入成功、磁盘上文件也在、`list_skill_files` 也列得出来，但 **`get_skill` 的
//! `content` 恒为空**——因为判定主文件身份的地方口径不一致（有的忽略大小写、
//! 有的严格 `==`），而 DAO 层 `read_main_content` 又按小写常量直拼路径。
//!
//! 本文件固化三条契约：
//! - ① 主文件身份识别**忽略大小写**，且全仓只有 `is_main_skill_file` 一处判定；
//! - ② 导入时把命中的主文件**归一化为小写 `skill.md` 落盘**，让按常量拼路径的
//!   DAO 层在 Linux（大小写敏感 FS）上也恒能命中；
//! - ③ 名字不叫 `skill.md` 的 md 文件**不认作主文件**（显式边界：将来若放宽为
//!   「根目录唯一 md 兜底当主文件」，需连带改这里）。

use super::skill_test::{create_test_skill, init_test_env};
use super::{CreateSkillParams, SkillFileImport};
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

/// ③ 边界：根级有多个 `.md` 候选时**不猜**主文件（多个 md 任选 = 静默猜错）。
#[sqlx::test]
async fn fallback_skips_when_multiple_root_markdowns(pool: SqlitePool) {
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

    assert!(
        loaded.main_content().is_none(),
        "根目录多个 .md 时不应猜主文件：任选其一会把真正的主内容埋成附件"
    );
    assert_eq!(
        loaded.files.len(),
        2,
        "两个候选文件都应原样落盘，调用方需显式提供 skill.md"
    );
}

/// ⑤ 边界固化：绕开 zip 的单文件导入里，显式 `target_path` 非 skill.md 时不改其名。
///
/// 兜底规则只作用于 **zip 包**（包结构由打包方决定）；单文件场景里调用方既然明确
/// 指定了目标名，就不该被偷偷改写。
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

    assert!(
        loaded.main_content().is_none(),
        "不叫 skill.md 的 md 不应被当成主文件（当前契约：主文件名固定）"
    );
    assert!(
        loaded.files.iter().any(|f| f.filename == "main.md"),
        "文件本身应正常落盘，只是不承担「正文直出」角色"
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
