//! 编译期内嵌文件读取模块
//!
//! 通过 include_str! 将 seed/skills/ 下的预置技能文件嵌入二进制，
//! 无需文件系统即可在首次初始化时导入技能内容。
//!
//! 新增预置技能文件时，在此处添加对应的 include_str! 静态变量和 match 分支。

/// 内嵌的预置技能文件注册表
///
/// 每个条目：(相对路径, 文件内容)
/// 相对路径以 "skills/" 开头，与 SkillFileDef.ref_path 约定一致
static EMBEDDED_SKILL_FILES: &[(&str, &str)] = &[
    (
        "skills/platform_guide/skill.md",
        include_str!("skills/platform_guide/skill.md"),
    ),
    (
        "skills/memory_guide/skill.md",
        include_str!("skills/memory_guide/skill.md"),
    ),
    (
        "skills/collaboration_guide/skill.md",
        include_str!("skills/collaboration_guide/skill.md"),
    ),
];

/// 读取编译期内嵌的文件内容
///
/// # 参数
/// ref_path - 相对 seed 目录的路径（如 "skills/platform_guide/skill.md"）
///
/// # 返回
/// - Ok(String) - 文件内容
/// - Err(String) - 文件未在编译期注册
pub fn read_embedded_file(ref_path: &str) -> Result<String, String> {
    EMBEDDED_SKILL_FILES
        .iter()
        .find(|(path, _)| *path == ref_path)
        .map(|(_, content)| (*content).to_string())
        .ok_or_else(|| {
            format!(
                "编译期内嵌文件不存在: {}（请在 embedded.rs 注册）",
                ref_path
            )
        })
}

/// 列出所有编译期内嵌的 skills 目录下文件路径
///
/// 用于调试和验证编译期嵌入是否完整
pub fn list_embedded_skill_files() -> Vec<String> {
    EMBEDDED_SKILL_FILES
        .iter()
        .map(|(path, _)| path.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_embedded_file_platform_guide() {
        let content = read_embedded_file("skills/platform_guide/skill.md").unwrap();
        assert!(!content.is_empty());
    }

    #[test]
    fn test_read_embedded_file_memory_guide() {
        let content = read_embedded_file("skills/memory_guide/skill.md").unwrap();
        assert!(!content.is_empty());
    }

    #[test]
    fn test_read_embedded_file_collaboration_guide() {
        let content = read_embedded_file("skills/collaboration_guide/skill.md").unwrap();
        assert!(!content.is_empty());
    }

    #[test]
    fn test_read_embedded_file_not_found() {
        let result = read_embedded_file("skills/nonexistent/skill.md");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("不存在"));
    }

    #[test]
    fn test_list_embedded_skill_files_count() {
        let files = list_embedded_skill_files();
        assert_eq!(files.len(), 3);
        assert!(files.contains(&"skills/platform_guide/skill.md".to_string()));
        assert!(files.contains(&"skills/memory_guide/skill.md".to_string()));
        assert!(files.contains(&"skills/collaboration_guide/skill.md".to_string()));
    }
}
