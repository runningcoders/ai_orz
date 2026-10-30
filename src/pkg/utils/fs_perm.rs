//! 文件可执行位工具（技能包脚本附件专用）
//!
//! 技能包允许携带脚本类附件（`scripts/*.sh`、`*.py`），Agent 需要能从
//! `list_skill_files` 拿到的路径直接执行。导入（zip 解包 / 附件 copy）与
//! 副本复制两条通路都要保证可执行位不丢，因此判定与设置收敛到一处。

use std::path::Path;

/// 判断文件是否带可执行位（owner/group/other 任一 x 位）。
pub fn is_executable(metadata: &std::fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        false
    }
}

/// 给目标文件补上可执行位。
///
/// 失败只降级不报错：脚本仍可用 `bash xx.sh` / `python3 xx.py` 走显式解释器执行，
/// 不该因为权限位设置失败就让整个导入/安装回滚。
#[cfg(unix)]
pub fn mark_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = std::fs::metadata(path) {
        let mut perms = metadata.permissions();
        perms.set_mode(perms.mode() | 0o111);
        let _ = std::fs::set_permissions(path, perms);
    }
}

#[cfg(not(unix))]
pub fn mark_executable(_path: &Path) {}

/// 按扩展名判断是否为「脚本类」附件（导入时主动补可执行位）。
///
/// zip 条目即使带 unix mode，解包后也不保证落盘带 x 位；手填/附件通路更没有
/// 权限信息。按扩展名兜底，让 Agent 拿到路径就能 `./run.sh` 跑起来。
pub fn is_script_filename(filename: &str) -> bool {
    matches!(
        filename
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_lowercase()
            .as_str(),
        "sh" | "bash" | "zsh" | "py" | "js" | "mjs" | "cjs" | "ts" | "rb" | "pl"
    )
}
