//! 技能附件访问契约测试（护栏）
//!
//! 契约：**接口只直出根 `skill.md`**；其余附件一律只暴露 `abs_path`，
//! 由 Agent 用 `read_file` / shell 工具按路径按需访问。
//!
//! 这些用例是「为什么」的固化记录，改契约前先看这里：
//! - 附件可以是二进制（图片 / 模板 / zip），经接口用 `read_to_string` 读必然报错
//! - 附件可以很大，回传内容会撑爆模型上下文
//! - 技能包里的脚本要能被 shell 工具执行，前提是路径可达 + 可执行位不丢

use super::skill_test::{create_test_skill, init_test_env};
use super::{CreateSkillParams, SkillFileImport, UpdateSkillParams};
use crate::models::skill::{SKILL_MAIN_FILE, Skill, SkillFile};
use crate::pkg::utils::fs_perm::is_executable;
use common::error::ErrorCode;
use sqlx::SqlitePool;

/// 带根引导文件的技能（create_test_skill 默认无文件，需显式给 skill.md）
fn skill_with_main(name: &str) -> Skill {
    let mut skill = create_test_skill(name);
    skill.files.push(SkillFile {
        filename: SKILL_MAIN_FILE.to_string(),
        file_size: 8,
        content: Some("# 引导\n".to_string()),
        abs_path: std::path::PathBuf::new(),
        executable: false,
    });
    skill
}

/// 附件只暴露路径，不随接口回传内容；根 skill.md 仍直出内容。
#[sqlx::test]
async fn attachments_expose_path_instead_of_content(pool: SqlitePool) {
    let (domain, ctx, _temp) = init_test_env(pool.clone());

    let skill = skill_with_main("AttachmentContract");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();

    domain
        .skill_manage()
        .update_skill(
            ctx.clone(),
            UpdateSkillParams {
                skill: &skill,
                imports: vec![
                    SkillFileImport {
                        target_path: Some("scripts/deploy.sh".to_string()),
                        source_abs_path: None,
                        content_bytes: Some(b"#!/bin/sh\necho hi\n".to_vec()),
                        suggested_name: None,
                        executable: None,
                    },
                    SkillFileImport {
                        target_path: Some("assets/logo.png".to_string()),
                        source_abs_path: None,
                        // 非法 UTF-8：接口若试图回传内容必然失败
                        content_bytes: Some(vec![0x89, 0x50, 0x4E, 0x47, 0xFF, 0xFE]),
                        suggested_name: None,
                        executable: None,
                    },
                ],
                file_deletes: Vec::new(),
                remote_source: None,
            },
        )
        .await
        .unwrap();

    let files = domain
        .skill_manage()
        .list_skill_files(ctx.clone(), skill.id())
        .await
        .unwrap()
        .unwrap();

    // 主文件：内容直出（它是「怎么用这份技能」的说明书）
    let main = files
        .iter()
        .find(|f| f.filename == SKILL_MAIN_FILE)
        .expect("缺少根 skill.md");
    assert!(main.content.is_some(), "根 skill.md 必须直出内容");

    // 附件：只给路径，不给内容
    for name in ["scripts/deploy.sh", "assets/logo.png"] {
        let f = files
            .iter()
            .find(|f| f.filename == name)
            .unwrap_or_else(|| panic!("缺少附件 {name}"));
        assert_eq!(f.content, None, "附件 {name} 不应回传内容");
        assert!(f.abs_path.exists(), "附件 {name} 路径必须可访问");
        assert!(f.abs_path.is_absolute(), "给 Agent 的路径必须是绝对路径");
    }

    // 脚本附件带可执行位（Agent 可直接 ./deploy.sh 或交给 shell 工具）
    let sh = files
        .iter()
        .find(|f| f.filename == "scripts/deploy.sh")
        .unwrap();
    assert!(sh.executable, "脚本附件应带可执行位");
    assert!(is_executable(&std::fs::metadata(&sh.abs_path).unwrap()));
}

/// 接口只放行根引导文件；读附件返回 InvalidRequest 并带路径提示。
#[sqlx::test]
async fn file_content_api_serves_main_file_only(pool: SqlitePool) {
    let (domain, ctx, _temp) = init_test_env(pool.clone());

    let skill = skill_with_main("MainOnlyReading");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();
    domain
        .skill_manage()
        .update_skill(
            ctx.clone(),
            UpdateSkillParams {
                skill: &skill,
                imports: vec![SkillFileImport {
                    target_path: Some("scripts/run.sh".to_string()),
                    source_abs_path: None,
                    content_bytes: Some(b"echo hi".to_vec()),
                    suggested_name: None,
                    executable: None,
                }],
                file_deletes: Vec::new(),
                remote_source: None,
            },
        )
        .await
        .unwrap();

    // 根引导文件：正常直读
    let content = domain
        .skill_manage()
        .get_skill_file_content(ctx.clone(), skill.id(), SKILL_MAIN_FILE)
        .await
        .unwrap()
        .unwrap();
    assert!(!content.is_empty());

    // 附件：拒绝，且错误信息里给出可自行访问的路径
    let err = domain
        .skill_manage()
        .get_skill_file_content(ctx.clone(), skill.id(), "scripts/run.sh")
        .await
        .expect_err("附件不应通过接口读内容");
    assert_eq!(err.code_enum(), ErrorCode::InvalidRequest);
    let msg = format!("{err}");
    assert!(msg.contains("scripts/run.sh"), "错误应给出附件路径: {msg}");
    assert!(
        msg.contains("read_file"),
        "错误应指引用 read_file 工具: {msg}"
    );

    // 系统内部导出通路仍可读任意文件（seed 快照必须自包含），且是按**字节**读
    let raw = domain
        .skill_manage()
        .read_skill_file_bytes(ctx.clone(), skill.id(), "scripts/run.sh")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(raw, b"echo hi");
}

/// 安装到 Agent 的副本必须带上子目录附件（历史上只拷顶层，assets/ scripts/ 全丢）。
#[sqlx::test]
async fn agent_copy_keeps_subdirectory_attachments(pool: SqlitePool) {
    let (domain, ctx, _temp) = init_test_env(pool.clone());

    let skill = skill_with_main("CopySubdir");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();
    domain
        .skill_manage()
        .update_skill(
            ctx.clone(),
            UpdateSkillParams {
                skill: &skill,
                imports: vec![SkillFileImport {
                    target_path: Some("scripts/deploy.sh".to_string()),
                    source_abs_path: None,
                    content_bytes: Some(b"#!/bin/sh\necho hi\n".to_vec()),
                    suggested_name: None,
                    executable: None,
                }],
                file_deletes: Vec::new(),
                remote_source: None,
            },
        )
        .await
        .unwrap();

    // 发布后安装到 Agent（install_to_agent 要求源为 Published）
    let mut published = skill.clone();
    published.po.status = common::enums::SkillStatus::Published;
    domain
        .skill_manage()
        .update_skill(
            ctx.clone(),
            UpdateSkillParams {
                skill: &published,
                imports: Vec::new(),
                file_deletes: Vec::new(),
                remote_source: None,
            },
        )
        .await
        .unwrap();

    let copy = domain
        .skill_manage()
        .install_to_agent(ctx.clone(), skill.id(), "agent-1")
        .await
        .unwrap();

    // 副本作者是 Agent，用户侧只有 Admin 能越过作者限制查看
    let admin_ctx = ctx.to_builder().user_role(1).build();
    let files = domain
        .skill_manage()
        .list_skill_files(admin_ctx, copy.id())
        .await
        .unwrap()
        .unwrap();

    let copied = files
        .iter()
        .find(|f| f.filename == "scripts/deploy.sh")
        .expect("副本丢失子目录附件 scripts/deploy.sh");
    assert!(copied.abs_path.exists(), "副本附件路径不可访问");
    assert!(
        copied.executable,
        "副本脚本应保留可执行位，否则 Agent 无法执行"
    );
}

/// 导入预算：附件数量与合计大小超限必须拒绝（写入过程中累计，不是只看请求体）。
#[sqlx::test]
async fn import_budget_rejects_oversized_packages(pool: SqlitePool) {
    let (domain, ctx, _temp) = init_test_env(pool.clone());

    let skill = create_test_skill("BudgetGuard");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();

    // 文件数超限（上限 100）
    let too_many: Vec<SkillFileImport> = (0..101)
        .map(|i| SkillFileImport {
            target_path: Some(format!("assets/f{i}.bin")),
            source_abs_path: None,
            content_bytes: Some(vec![0u8; 8]),
            suggested_name: None,
            executable: None,
        })
        .collect();
    let err = domain
        .skill_manage()
        .update_skill(
            ctx.clone(),
            UpdateSkillParams {
                skill: &skill,
                imports: too_many,
                file_deletes: Vec::new(),
                remote_source: None,
            },
        )
        .await
        .expect_err("101 个文件应被拒绝");
    assert_eq!(err.code_enum(), ErrorCode::InvalidRequest);

    // 单文件超限（上限 10MB）
    let too_large = domain
        .skill_manage()
        .update_skill(
            ctx.clone(),
            UpdateSkillParams {
                skill: &skill,
                imports: vec![SkillFileImport {
                    target_path: Some("assets/big.bin".to_string()),
                    source_abs_path: None,
                    content_bytes: Some(vec![0u8; 10 * 1024 * 1024 + 1]),
                    suggested_name: None,
                    executable: None,
                }],
                file_deletes: Vec::new(),
                remote_source: None,
            },
        )
        .await
        .expect_err("超 10MB 的附件应被拒绝");
    assert_eq!(too_large.code_enum(), ErrorCode::InvalidRequest);
}

/// 文件名白名单：能进 shell 命令行的路径必须收敛字符集。
#[sqlx::test]
async fn import_rejects_shell_metacharacters_in_target_path(pool: SqlitePool) {
    let (domain, ctx, _temp) = init_test_env(pool.clone());

    let skill = create_test_skill("FilenameGuard");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();

    // 附件路径最终会被 Agent 拼进命令行，分号 / 空格 / `$()` 必须挡在导入侧
    for bad in [
        "scripts/run;rm -rf.sh",
        "scripts/$(whoami).sh",
        "scripts/back`tick`.sh",
        "scripts/with space.sh",
    ] {
        let err = domain
            .skill_manage()
            .update_skill(
                ctx.clone(),
                UpdateSkillParams {
                    skill: &skill,
                    imports: vec![SkillFileImport {
                        target_path: Some(bad.to_string()),
                        source_abs_path: None,
                        content_bytes: Some(b"x".to_vec()),
                        suggested_name: None,
                        executable: None,
                    }],
                    file_deletes: Vec::new(),
                    remote_source: None,
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err.code_enum(), ErrorCode::InvalidRequest, "{bad} 应被拒绝");
    }

    // 合法子目录路径仍可导入
    domain
        .skill_manage()
        .update_skill(
            ctx.clone(),
            UpdateSkillParams {
                skill: &skill,
                imports: vec![SkillFileImport {
                    target_path: Some("scripts/run_v2.sh".to_string()),
                    source_abs_path: None,
                    content_bytes: Some(b"echo ok".to_vec()),
                    suggested_name: None,
                    executable: None,
                }],
                file_deletes: Vec::new(),
                remote_source: None,
            },
        )
        .await
        .unwrap();

    // 中文文件名不是注入风险，必须放行（整包导入遇到中文附件才不会整包失败）
    domain
        .skill_manage()
        .update_skill(
            ctx.clone(),
            UpdateSkillParams {
                skill: &skill,
                imports: vec![SkillFileImport {
                    target_path: Some("参考资料/部署说明.md".to_string()),
                    source_abs_path: None,
                    content_bytes: Some("# 说明".as_bytes().to_vec()),
                    suggested_name: None,
                    executable: None,
                }],
                file_deletes: Vec::new(),
                remote_source: None,
            },
        )
        .await
        .expect("中文附件名应放行");
}

/// 整包导入（seed 技能包走的就是这条通路）：zip 一次还原全部文件，
/// 二进制字节与可执行位都不得丢失。
///
/// 这是「技能包必须整包搬运」的护栏：逐文件走文本导入会把二进制附件读成空，
/// 扩展名推断也救不了无扩展名的可执行件。
#[sqlx::test]
async fn zip_package_import_restores_binary_and_exec_bit(pool: SqlitePool) {
    use crate::service::domain::system::seed::bundle::{self, SkillFileEntry};

    let (domain, ctx, _temp) = init_test_env(pool.clone());

    let skill = skill_with_main("ZipPackageImport");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();

    let binary: Vec<u8> = vec![0x00, 0xFF, 0xFE, 0x01, 0x80, 0x7F];
    let package = bundle::pack_skill_zip(&[
        SkillFileEntry {
            path: SKILL_MAIN_FILE.to_string(),
            bytes: "# 引导\n".as_bytes().to_vec(),
            executable: false,
        },
        SkillFileEntry {
            path: "scripts/run.sh".to_string(),
            bytes: b"echo hi".to_vec(),
            executable: true,
        },
        SkillFileEntry {
            path: "assets/blob.bin".to_string(),
            bytes: binary.clone(),
            executable: false,
        },
        SkillFileEntry {
            // 无扩展名的可执行件：只能靠 zip unix mode 还原
            path: "bin/tool".to_string(),
            bytes: b"\x7fELF-ish".to_vec(),
            executable: true,
        },
    ])
    .unwrap();

    // 整包导入：SkillFileImport 收到 zip magic 后自动解包到技能目录
    domain
        .skill_manage()
        .update_skill(
            ctx.clone(),
            UpdateSkillParams {
                skill: &skill,
                imports: vec![SkillFileImport {
                    target_path: None,
                    source_abs_path: None,
                    content_bytes: Some(package),
                    suggested_name: None,
                    executable: None,
                }],
                file_deletes: Vec::new(),
                remote_source: None,
            },
        )
        .await
        .unwrap();

    let files = domain
        .skill_manage()
        .list_skill_files(ctx.clone(), skill.id())
        .await
        .unwrap()
        .unwrap();

    for name in [
        SKILL_MAIN_FILE,
        "scripts/run.sh",
        "assets/blob.bin",
        "bin/tool",
    ] {
        assert!(
            files.iter().any(|f| f.filename == name),
            "整包导入后应存在 {name}"
        );
    }

    // 二进制附件字节原样（不走 UTF-8 解码）
    let restored = domain
        .skill_manage()
        .read_skill_file_bytes(ctx.clone(), skill.id(), "assets/blob.bin")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(restored, binary, "二进制附件必须字节级保真");

    // 可执行位：脚本按扩展名、无扩展名的 bin/tool 按 zip unix mode
    for name in ["scripts/run.sh", "bin/tool"] {
        let f = files.iter().find(|f| f.filename == name).unwrap();
        assert!(f.executable, "{name} 应保留可执行位");
        assert!(is_executable(&std::fs::metadata(&f.abs_path).unwrap()));
    }
}

/// 导入不得吞掉源文件（PATH 分支的 copy 语义护栏）。
///
/// 背景：这里曾经「先 rename(move)、失败才 copy + remove_file(src_path)」——
/// 两条路**都会把源文件删掉**。而 Attachment / Artifact 都是有 DB 行、且可被消息
/// 与任务引用的实体，导入技能不该把它们搬空：否则同一来源二次导入、或后续再读
/// 该附件/产物都会直接落空。现已改为一律 copy 并保留原位，本用例钉死该性质。
#[sqlx::test]
async fn import_from_path_copies_and_keeps_source(pool: SqlitePool) {
    let (domain, ctx, _temp) = init_test_env(pool.clone());

    let skill = skill_with_main("PathImportKeepsSource");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();

    let src_path =
        std::env::temp_dir().join(format!("skill_import_src_{}.md", uuid::Uuid::now_v7()));
    std::fs::write(&src_path, "# 源文件内容\n").unwrap();

    domain
        .skill_manage()
        .update_skill(
            ctx.clone(),
            UpdateSkillParams {
                skill: &skill,
                imports: vec![SkillFileImport {
                    target_path: Some("references/source.md".to_string()),
                    source_abs_path: Some(src_path.clone()),
                    content_bytes: None,
                    suggested_name: None,
                    executable: None,
                }],
                file_deletes: Vec::new(),
                remote_source: None,
            },
        )
        .await
        .unwrap();

    // ① 技能里确实落到了 target_path
    let files = domain
        .skill_manage()
        .list_skill_files(ctx.clone(), skill.id())
        .await
        .unwrap()
        .unwrap();
    assert!(
        files.iter().any(|f| f.filename == "references/source.md"),
        "源文件应被导入为 references/source.md"
    );

    // ② 关键点：源文件仍在原位且内容完好（copy 而非 move）
    assert!(
        src_path.exists(),
        "导入后源文件必须仍在原位：Attachment / Artifact 是被引用实体，不能被导入动作搬空"
    );
    let kept = std::fs::read_to_string(&src_path).unwrap();
    assert_eq!(kept, "# 源文件内容\n", "源文件内容不应被截断或改动");

    std::fs::remove_file(&src_path).unwrap();
}
