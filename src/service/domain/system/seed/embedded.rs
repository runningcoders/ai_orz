//! 编译期内嵌文件读取模块
//!
//! 通过 include_str! 将 seed/skills/ 下的预置技能文件嵌入二进制，
//! 无需文件系统即可在首次初始化时导入技能内容。
//!
//! 约定：文件夹名与技能 ID 一致（如 `TEMPLATE_TOOL_MANAGEMENT/skill.md`），
//! 便于从 default.json 的 `content_path` 直接定位文件系统目录。
//!
//! 新增预置技能文件时，在此处添加对应的 include_str! 静态变量和 match 分支。

/// 内嵌文件路径前缀（`skills/<技能 ID>/<包内相对路径>`）
pub const SKILL_REF_PREFIX: &str = "skills/";

/// 内嵌的预置技能文件注册表
///
/// 每个条目：(相对路径, 文件内容)
/// 相对路径以 "skills/" 开头，与 SkillFileDef.ref_path 约定一致
static EMBEDDED_SKILL_FILES: &[(&str, &str)] = &[
    (
        "skills/TEMPLATE_TOOL_MANAGEMENT/skill.md",
        include_str!("skills/TEMPLATE_TOOL_MANAGEMENT/skill.md"),
    ),
    (
        "skills/TEMPLATE_SKILL_MANAGEMENT/skill.md",
        include_str!("skills/TEMPLATE_SKILL_MANAGEMENT/skill.md"),
    ),
    (
        "skills/TEMPLATE_MEMORY_COGNITION/skill.md",
        include_str!("skills/TEMPLATE_MEMORY_COGNITION/skill.md"),
    ),
    (
        "skills/TEMPLATE_ONTOLOGY_COGNITION/skill.md",
        include_str!("skills/TEMPLATE_ONTOLOGY_COGNITION/skill.md"),
    ),
    (
        "skills/TEMPLATE_COMMUNICATION/skill.md",
        include_str!("skills/TEMPLATE_COMMUNICATION/skill.md"),
    ),
    (
        "skills/TEMPLATE_PROJECT_MANAGEMENT/skill.md",
        include_str!("skills/TEMPLATE_PROJECT_MANAGEMENT/skill.md"),
    ),
    (
        "skills/GIT_BRANCH_WORKFLOW/skill.md",
        include_str!("skills/GIT_BRANCH_WORKFLOW/skill.md"),
    ),
    (
        "skills/TEMPLATE_AGENT_RECRUITMENT/skill.md",
        include_str!("skills/TEMPLATE_AGENT_RECRUITMENT/skill.md"),
    ),
    (
        "skills/TEMPLATE_USER_RECEPTION/skill.md",
        include_str!("skills/TEMPLATE_USER_RECEPTION/skill.md"),
    ),
    (
        "skills/TEMPLATE_SELF_EVOLUTION/skill.md",
        include_str!("skills/TEMPLATE_SELF_EVOLUTION/skill.md"),
    ),
    (
        "skills/TEMPLATE_PROJECT_CONTEXT_COGNITION/skill.md",
        include_str!("skills/TEMPLATE_PROJECT_CONTEXT_COGNITION/skill.md"),
    ),
    (
        "skills/TEMPLATE_PROBLEM_SOLVING/skill.md",
        include_str!("skills/TEMPLATE_PROBLEM_SOLVING/skill.md"),
    ),
];

/// 全部内嵌技能文件条目（`(相对路径, 文件内容)`）
///
/// 默认快照的技能包由此在运行时按技能分组打成 zip：正文仍以 md 形式留在仓库里
/// 可读可 diff，产物不入库，包格式与导出快照一致。
pub fn embedded_skill_files() -> &'static [(&'static str, &'static str)] {
    EMBEDDED_SKILL_FILES
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每个预置技能目录都必须内嵌根引导文件，且正文带该技能的特征标记
    ///
    /// 表驱动替代原先 12 个单技能用例：新增/删除预置技能时改这张表即可。漏注册
    /// 会在这里先炸——比初始化时报「缺少技能包」更早、也更直观。
    #[test]
    fn every_embedded_skill_has_main_file_with_expected_marker() {
        const MAIN: &str = "skill.md";
        let markers: &[(&str, Option<&str>)] = &[
            ("TEMPLATE_TOOL_MANAGEMENT", None),
            ("TEMPLATE_SKILL_MANAGEMENT", None),
            ("TEMPLATE_MEMORY_COGNITION", None),
            ("TEMPLATE_COMMUNICATION", None),
            ("TEMPLATE_PROJECT_MANAGEMENT", None),
            ("GIT_BRANCH_WORKFLOW", Some("Git Branch Workflow")),
            ("TEMPLATE_AGENT_RECRUITMENT", Some("Agent 招聘")),
            ("TEMPLATE_USER_RECEPTION", Some("用户接待")),
            ("TEMPLATE_SELF_EVOLUTION", Some("自我进化")),
            ("TEMPLATE_ONTOLOGY_COGNITION", Some("本体认知")),
            ("TEMPLATE_PROJECT_CONTEXT_COGNITION", Some("项目上下文认知")),
            ("TEMPLATE_PROBLEM_SOLVING", Some("问题求解策略")),
        ];

        for (skill_id, marker) in markers {
            let path = format!("{}{}/{}", SKILL_REF_PREFIX, skill_id, MAIN);
            let content = embedded_skill_files()
                .iter()
                .find(|(p, _)| *p == path)
                .map(|(_, c)| *c)
                .unwrap_or_else(|| panic!("预置技能 {skill_id} 的 {MAIN} 未在 embedded.rs 注册"));
            assert!(!content.trim().is_empty(), "预置技能 {skill_id} 正文为空");
            if let Some(marker) = marker {
                assert!(
                    content.contains(marker),
                    "预置技能 {skill_id} 正文缺少特征标记 {marker}"
                );
            }
        }

        assert_eq!(
            embedded_skill_files().len(),
            markers.len(),
            "内嵌文件条目数与预置技能清单不一致（多注册/漏注册）"
        );
    }
}
