//! 技能目录与文件读写：DAO 透传。
//!
//! ⚠️ 这里**不做**任何编排、也不产生副作用（不触发向量重建）。真正需要「写完正文
//! 顺手重算向量」的场景只有两处：`content.rs`（整包写入，末尾统一算一次）与
//! 显式调 `reindex_vector`（编辑器保存单文件后补）。
//!
//! 为什么不在这里挂钩子：`write_file` 每次落盘都会触发，而 zip 解包会连写几十个
//! 附件 —— 每个附件都算一次 embedding，既浪费又会在正文尚未写全时算出错误向量。

use super::SkillDalImpl;
use crate::models::skill::{SkillFile, SkillPo};
use common::error::Result;
use std::path::PathBuf;

impl SkillDalImpl {
    pub(super) fn read_main_content(&self, skill: &SkillPo) -> Result<String> {
        self.skill_dao.read_main_content(skill)
    }

    pub(super) fn write_main_content(&self, skill: &SkillPo, content: &str) -> Result<()> {
        self.skill_dao.write_main_content(skill, content)
    }

    pub(super) fn list_files(&self, skill: &SkillPo) -> Result<Vec<SkillFile>> {
        self.skill_dao.list_files(skill)
    }

    pub(super) fn read_file(&self, skill: &SkillPo, filename: &str) -> Result<String> {
        self.skill_dao.read_file(skill, filename)
    }

    pub(super) fn write_file(&self, skill: &SkillPo, filename: &str, content: &str) -> Result<()> {
        self.skill_dao.write_file(skill, filename, content)
    }

    pub(super) fn write_file_bytes(
        &self,
        skill: &SkillPo,
        filename: &str,
        bytes: &[u8],
    ) -> Result<()> {
        self.skill_dao.write_file_bytes(skill, filename, bytes)
    }

    pub(super) fn skill_dir(&self, skill: &SkillPo) -> PathBuf {
        self.skill_dao.skill_dir(skill)
    }

    pub(super) fn file_abs_path(&self, skill: &SkillPo, filename: &str) -> PathBuf {
        self.skill_dao.file_abs_path(skill, filename)
    }

    pub(super) fn delete_file(&self, skill: &SkillPo, filename: &str) -> Result<()> {
        self.skill_dao.delete_file(skill, filename)
    }
}
