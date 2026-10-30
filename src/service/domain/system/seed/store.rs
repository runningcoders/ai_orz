//! 文件系统 CRUD
//!
//! seeds/ 目录下管理所有 .zip 快照压缩包（内含 seed.json 主文件 + skills/*.zip 技能包）。
//! 路径基于 AppConfig.base_data_path 拼接

use crate::pkg::paths;
use crate::service::domain::system::seed::bundle;
use common::error::{Error, Result};
use std::path::{Path, PathBuf};

/// 获取 seeds/ 目录路径（基于 AppConfig.base_data_path）
pub fn seeds_dir() -> PathBuf {
    let base = crate::config::get().base_data_path();
    paths::seeds_dir(&base)
}

/// 校验文件名安全性（防止路径穿越攻击）
///
/// 返回规范化后的文件名（默认附加 .zip 后缀）
pub fn validate_seed_filename(name: &str) -> Result<String> {
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(Error::bad_request(format!("无效文件名: {}", name)));
    }
    if name.ends_with(".zip") {
        return Ok(name.to_string());
    }
    Ok(format!("{}.zip", name))
}

/// 列出 seeds/ 目录下所有快照压缩包（.zip）
pub async fn list_files(dir: &Path) -> Result<Vec<common::api::seed::SeedFileInfo>> {
    if !dir.exists() {
        std::fs::create_dir_all(dir)?;
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    let mut entries = tokio::fs::read_dir(dir)
        .await
        .map_err(|e| Error::internal(format!("读取 seeds 目录失败: {}", e)))?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| Error::internal(format!("读取目录项失败: {}", e)))?
    {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("zip") {
            continue;
        }

        let metadata = entry
            .metadata()
            .await
            .map_err(|e| Error::internal(format!("读取文件元信息失败: {}", e)))?;
        let name = entry.file_name().to_string_lossy().to_string();
        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        let is_default = name.starts_with("default");

        files.push(common::api::seed::SeedFileInfo {
            name,
            size: metadata.len(),
            modified_at,
            is_default,
        });
    }

    files.sort_by_key(|x| std::cmp::Reverse(x.modified_at));
    Ok(files)
}

/// 读取快照主文件内容（压缩包内的 seed.json 文本）
pub async fn read_file(dir: &Path, name: &str) -> Result<common::api::seed::GetSeedFileResponse> {
    let file_name = validate_seed_filename(name)?;
    let path = dir.join(&file_name);
    let raw = tokio::fs::read(&path)
        .await
        .map_err(|e| Error::not_found(format!("快照文件不存在: {} ({})", file_name, e)))?;

    let content = bundle::read_manifest(&raw)?;
    let size = content.len() as u64;
    Ok(common::api::seed::GetSeedFileResponse {
        name: file_name,
        content,
        size,
    })
}

/// 读取快照压缩包原始字节
pub async fn read_raw(dir: &Path, name: &str) -> Result<Vec<u8>> {
    let file_name = validate_seed_filename(name)?;
    let path = dir.join(&file_name);
    tokio::fs::read(&path)
        .await
        .map_err(|e| Error::not_found(format!("快照文件不存在: {} ({})", file_name, e)))
}

/// 写入快照压缩包
pub async fn write_file(dir: &Path, name: &str, bytes: &[u8]) -> Result<u64> {
    if !dir.exists() {
        std::fs::create_dir_all(dir)?;
    }
    let file_name = validate_seed_filename(name)?;
    let path = dir.join(&file_name);
    let size = bytes.len() as u64;
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|e| Error::internal(format!("写入快照文件失败: {}", e)))?;
    Ok(size)
}

/// 删除快照文件
pub async fn delete_file(dir: &Path, name: &str) -> Result<()> {
    let file_name = validate_seed_filename(name)?;
    let path = dir.join(&file_name);
    if !path.exists() {
        return Err(Error::not_found(format!("快照文件不存在: {}", file_name)));
    }
    tokio::fs::remove_file(&path)
        .await
        .map_err(|e| Error::internal(format!("删除快照文件失败: {}", e)))?;
    Ok(())
}
