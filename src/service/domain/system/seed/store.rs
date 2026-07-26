//! 文件系统 CRUD

use std::path::Path;
use common::error::Result;

pub async fn list_files(_dir: &Path) -> Result<Vec<common::api::seed::SeedFileInfo>> {
    unimplemented!("将在 Task 4 实现")
}

pub async fn read_file(_dir: &Path, _name: &str) -> Result<common::api::seed::GetSeedFileResponse> {
    unimplemented!("将在 Task 4 实现")
}

pub async fn write_file(_dir: &Path, _name: &str, _content: &str) -> Result<u64> {
    unimplemented!("将在 Task 4 实现")
}

pub async fn delete_file(_dir: &Path, _name: &str) -> Result<()> {
    unimplemented!("将在 Task 4 实现")
}

/// 校验路径安全性（防止路径穿越攻击）
pub fn validate_seed_filename(_name: &str) -> Result<String> {
    unimplemented!("将在 Task 4 实现")
}

/// 获取 seeds/ 目录路径（基于 AppConfig.base_data_path）
pub fn seeds_dir() -> std::path::PathBuf {
    crate::config::get().base_data_path().join("seeds")
}
