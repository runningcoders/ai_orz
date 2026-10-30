//! 内置默认快照
//!
//! 通过 include_str! 嵌入 default.json 与预置技能正文，无需文件系统即可使用。
//!
//! `default.json` 是**纯元信息文件**：与导出快照的 `seed.json` 同构，技能只登记
//! `package` 名，正文与附件一律不进主文件。技能包由编译期内嵌的技能文件在运行时
//! 打成，导入侧按包整包还原 —— 预置技能与导出快照走完全同一条通路，不再有
//! 「逐文件 ref_path」的专属分支。

use super::bundle::{self, SkillFileEntry};
use super::embedded;
use crate::pkg::utils::fs_perm::is_script_filename;
use std::collections::HashMap;
use std::sync::OnceLock;

const DEFAULT_JSON: &str = include_str!("default.json");

/// 获取内置默认快照
pub fn embedded_default_snapshot() -> super::defs::SeedSnapshot {
    serde_json::from_str(DEFAULT_JSON).expect("内置 default.json 解析失败（编译期检查）")
}

/// 内置默认快照的技能包（key = 主文件里登记的 `package` 名）
///
/// 技能正文仍以 md 形式躺在仓库里（`seed/skills/<ID>/`，可 diff、可人工修订），
/// 这里按技能分组打成 zip：既不往仓库塞二进制产物，又让预置技能与导出快照的包
/// 格式完全一致（同一个 `pack_skill_zip`），导入侧无需分叉。
pub fn embedded_default_skill_packages() -> &'static HashMap<String, Vec<u8>> {
    static PACKAGES: OnceLock<HashMap<String, Vec<u8>>> = OnceLock::new();
    PACKAGES.get_or_init(|| {
        let mut grouped: HashMap<String, Vec<SkillFileEntry>> = HashMap::new();
        for (path, content) in embedded::embedded_skill_files() {
            // 路径形如 `skills/<技能 ID>/<包内相对路径>`；ID 与 default.json 的技能 ID 同源
            let Some(rest) = path.strip_prefix(embedded::SKILL_REF_PREFIX) else {
                continue;
            };
            let Some((skill_id, rel)) = rest.split_once('/') else {
                continue;
            };
            grouped
                .entry(skill_id.to_string())
                .or_default()
                .push(SkillFileEntry {
                    path: rel.to_string(),
                    // 当前内嵌表全是 UTF-8 文本；将来注册 include_bytes! 的二进制附件时
                    // 条目同样走这里，字节原样进包，不做解码。
                    bytes: content.as_bytes().to_vec(),
                    executable: is_script_filename(rel),
                });
        }

        let mut packages = HashMap::with_capacity(grouped.len());
        for (skill_id, entries) in grouped {
            let zip =
                bundle::pack_skill_zip(&entries).expect("内置技能包打包失败（内存写入不应失败）");
            packages.insert(bundle::skill_package_name(&skill_id), zip);
        }
        packages
    })
}
