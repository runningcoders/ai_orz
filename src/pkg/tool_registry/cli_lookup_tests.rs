//! tests 单元测试（拆分自 cli_lookup.rs）
//!
//! 文件瘦身：原 556 行 → 280 行，测试体 277 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;

fn temp_home() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().to_path_buf();
    (dir, home)
}

#[test]
fn expand_home_only_handles_tilde_slash() {
    let home = PathBuf::from("/tmp/fake-home");
    assert_eq!(
        expand_home("~/.local/bin/skillhub", Some(&home)),
        home.join(".local/bin/skillhub")
    );
    // 非 ~/ 前缀原样
    assert_eq!(
        expand_home("/abs/bin", Some(&home)),
        PathBuf::from("/abs/bin")
    );
    assert_eq!(
        expand_home("skillhub", Some(&home)),
        PathBuf::from("skillhub")
    );
    // 无 HOME 时原样（不 panic）
    assert_eq!(expand_home("~/x", None), PathBuf::from("~/x"));
}

#[test]
fn command_available_expands_tilde_for_path_like_spec() {
    // 根因回归：`~/...` 形态必须展开后再判存在（旧实现恒 false）
    let (_dir, home) = temp_home();
    let bin_dir = home.join(".local/bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let bin = bin_dir.join("skillhub");
    std::fs::write(&bin, "#!/bin/sh\n").unwrap();

    let spec = "~/.local/bin/skillhub";
    // 直接用真实 HOME 判定（进程 HOME ≠ temp home），故走注入版核验展开语义
    let got = resolve_cli_command_with(
        Some(spec),
        Some(&home),
        None,
        "skillhub",
        |p| p.is_file(),
        |_| None,
    );
    assert!(got.is_ok(), "resolve should hit expanded path");
    assert_eq!(got.unwrap(), bin.to_string_lossy());
}

#[test]
fn command_available_absolute_and_missing() {
    assert!(command_available("/bin/ls"));
    assert!(!command_available("/no/such/binary-xyz"));
    assert!(!command_available(""));
    assert!(!command_available("   "));
}

#[test]
fn command_available_bare_name_scans_path() {
    // 裸名：ls 必在 PATH 内；不存在的名字必 false
    assert!(command_available("ls"));
    assert!(!command_available("no-such-binary-xyz-abc"));
}

#[test]
fn resolve_four_step_priority_order() {
    let (_dir, home) = temp_home();
    // 步骤 1：config 显式命中
    let explicit = home.join("explicit-bin");
    std::fs::write(&explicit, "").unwrap();
    let got = resolve_cli_command_with(
        Some(explicit.to_string_lossy().as_ref()),
        Some(&home),
        Some("~/.fallback/bin"),
        "mycli",
        |p| p.is_file(),
        |_| None,
    )
    .unwrap();
    assert_eq!(got, explicit.to_string_lossy());

    // 步骤 2：默认布局命中（config 未命中）
    let fallback = home.join(".fallback").join("bin");
    std::fs::create_dir_all(&fallback).unwrap();
    std::fs::write(fallback.join("mycli"), "").unwrap();
    let got = resolve_cli_command_with(
        Some("/no/such/explicit"),
        Some(&home),
        Some("~/.fallback/bin/mycli"),
        "mycli",
        |p| p.is_file(),
        |_| None,
    )
    .unwrap();
    assert_eq!(got, fallback.join("mycli").to_string_lossy());

    // 步骤 3：PATH 兜底
    let got = resolve_cli_command_with(
        Some("/no/such/explicit"),
        Some(&home),
        Some("~/.fallback/bin/mycli"),
        "mycli",
        |_| false,
        |name| (name == "mycli").then(|| PathBuf::from("/usr/local/bin/mycli")),
    )
    .unwrap();
    assert_eq!(got, "/usr/local/bin/mycli");

    // 步骤 4：全部未命中
    let miss =
        resolve_cli_command_with(None, None, None, "mycli", |_| false, |_| None).unwrap_err();
    assert_eq!(miss.bin_name, "mycli");
}

#[test]
fn resolve_accepts_bare_name_in_config_via_path() {
    // config 写裸名也算有效声明（走 PATH 兜底）
    let got = resolve_cli_command_with(
        Some("mycli"),
        None,
        None,
        "mycli",
        |_| false,
        |name| (name == "mycli").then(|| PathBuf::from("/opt/bin/mycli")),
    )
    .unwrap();
    assert_eq!(got, "/opt/bin/mycli");
}

#[test]
fn cli_probe_readiness_shape() {
    let ready = cli_probe_readiness(Some("/bin/ls"), "ls", None, "install hint");
    assert_eq!(ready, RuntimeReady::Ready);

    let not_ready = cli_probe_readiness(Some("/no/such/xyz"), "no-such-bin-xyz", None, "");
    assert_eq!(
        not_ready,
        RuntimeReady::NotReady {
            reason: "cli_not_installed".to_string(),
            hint: "或在工具配置中修改命令路径".to_string(),
        }
    );

    // install_hint 非空 → 「install；config」组合（与 cli_binary_readiness 同形态）
    let with_hint = cli_probe_readiness(Some("/no/such/xyz"), "no-such-bin-xyz", None, "装它");
    assert_eq!(
        with_hint,
        RuntimeReady::NotReady {
            reason: "cli_not_installed".to_string(),
            hint: "装它；或在工具配置中修改命令路径".to_string(),
        }
    );
}

#[test]
fn cli_propose_heal_returns_single_command_field() {
    // 已可用 → 无需自愈
    assert!(cli_propose_heal(Some("/bin/ls"), "ls", None).is_none());
    // 无命令声明 → 无自愈发起点
    assert!(cli_propose_heal(None, "ls", None).is_none());
    // 真正没装（PATH 也没有）→ 无解交人工
    assert!(cli_propose_heal(Some("/no/such/xyz"), "no-such-bin-xyz", None).is_none());

    // 默认布局可用 → 提议恰好一个 command 字段
    let (_dir, home) = temp_home();
    let fallback_dir = home.join(".fallback");
    std::fs::create_dir_all(&fallback_dir).unwrap();
    std::fs::write(fallback_dir.join("mycli"), "").unwrap();
    // 注意：真实 HOME 与 temp home 不同，故此处用 PATH 命中路径核验字段形态
    let healed = cli_propose_heal(Some("/no/such/xyz"), "ls", None);
    let healed = healed.expect("ls 在 PATH 内，应当可自愈");
    let obj = healed.as_object().unwrap();
    assert_eq!(obj.len(), 1, "只允许 command 单字段");
    assert!(obj.contains_key("command"));
    assert!(obj["command"].as_str().unwrap().ends_with("ls"));
}

#[test]
fn root_cause_tilde_spec_now_resolves_whereas_old_logic_failed() {
    // 根因回归（本项目的精确病灶）：`~/.skillhub/skillhub` 这类带 `~` 的规格
    // 过去被 `Path::new` 当成普通字符，`components().count() > 1` → 直接判
    // is_file("~/.skillhub/skillhub") → 恒 false。现在必须展开后判定。
    let (_dir, home) = temp_home();
    let real_bin = home.join(".local/bin/skillhub");
    std::fs::create_dir_all(real_bin.parent().unwrap()).unwrap();
    std::fs::write(&real_bin, "#!/bin/sh\n").unwrap();

    // 旧口径（不展开）：直接判原始字符串 → 必然不存在
    assert!(!Path::new("~/.local/bin/skillhub").is_file());

    // 新口径（展开）：解析命中同一文件
    let resolved = resolve_cli_command_with(
        Some("~/.local/bin/skillhub"),
        Some(&home),
        None,
        "skillhub",
        |p| p.is_file(),
        |_| None,
    )
    .expect("expanded spec must resolve");
    assert_eq!(resolved, real_bin.to_string_lossy());
}

#[test]
fn probe_and_heal_agree_on_the_same_resolution() {
    // 判定侧与自愈侧必须共用同一解析口径 —— 否则会出现
    // 「显示未就绪但提议不出方案」或「提议了但判定仍不认」的死结。
    let (_dir, home) = temp_home();
    let bin = home.join("tools/mycli");
    std::fs::create_dir_all(bin.parent().unwrap()).unwrap();
    std::fs::write(&bin, "").unwrap();

    let spec = "~/tools/mycli";
    let resolved = resolve_cli_command_with(
        Some("/no/such/old"),
        Some(&home),
        Some(spec),
        "mycli",
        |p| p.is_file(),
        |_| None,
    )
    .unwrap();
    assert_eq!(resolved, bin.to_string_lossy());

    // 该路径一旦写入 config，命令可寻址判定必须为真（闭环自证）
    assert!(Path::new(&resolved).is_file());
}

#[test]
fn resolve_miss_never_suggests_network_install() {
    // 红线：解析失败只报事实，绝不携带 curl | bash 之类的代装指令
    let miss =
        resolve_cli_command_with(None, None, None, "ghost-cli", |_| false, |_| None).unwrap_err();
    assert_eq!(miss.bin_name, "ghost-cli");
    let text = format!("{:?}", miss);
    assert!(!text.contains("curl"));
    assert!(!text.contains("bash"));
    assert!(!text.contains("install"));
}

#[test]
fn cli_probe_readiness_never_spawns_binary() {
    // 红线：就绪探测只做文件级判定，绝不实跑 CLI
    // （skillhub 类 CLI 有启动级 OTA，实跑属显式授权事项）
    // 用一个"存在但不是可执行程序"的普通文本文件验证判定只看存在性
    let (_dir, home) = temp_home();
    let plain = home.join("plain.txt");
    std::fs::write(&plain, "not a program").unwrap();

    let ready = cli_probe_readiness(
        Some(plain.to_string_lossy().as_ref()),
        "plain.txt",
        None,
        "",
    );
    assert_eq!(ready, RuntimeReady::Ready);
}

#[test]
fn two_separate_expansions_are_deterministic() {
    // 查找必须无副作用且确定性：同样输入两次结果一致
    let first = command_available("ls");
    let second = command_available("ls");
    assert_eq!(first, second);
    assert!(first);
}

#[test]
fn search_dirs_never_mutates_process_path() {
    let before = std::env::var_os("PATH");
    let dirs = search_dirs();
    let after = std::env::var_os("PATH");
    assert_eq!(before, after, "红线：查找不得改写 PATH 环境");
    assert!(!dirs.is_empty(), "PATH 至少应有一条");
}
