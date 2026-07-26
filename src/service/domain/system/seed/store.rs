//! 文件系统 CRUD
//!
//! seeds/ 目录下管理所有 .json 快照文件
//! 路径基于 AppConfig.base_data_path 拼接

use std::path::{Path, PathBuf};
use common::error::{Error, Result};

/// 获取 seeds/ 目录路径（基于 AppConfig.base_data_path）
pub fn seeds_dir() -> PathBuf {
    crate::config::get().base_data_path().join("seeds")
}

/// 校验文件名安全性（防止路径穿越攻击）
///
/// 返回规范化后的文件名（必要时附加 .json 后缀）
pub fn validate_seed_filename(name: &str) -> Result<String> {
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(Error::bad_request(format!("无效文件名: {}", name)));
    }
    let file_name = if name.ends_with(".json") {
        name.to_string()
    } else {
        format!("{}.json", name)
    };
    Ok(file_name)
}

/// 列出 seeds/ 目录下所有 .json 文件
pub async fn list_files(dir: &Path) -> Result<Vec<common::api::seed::SeedFileInfo>> {
    if !dir.exists() {
        std::fs::create_dir_all(dir)?;
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    let mut entries = tokio::fs::read_dir(dir).await
        .map_err(|e| Error::internal(format!("读取 seeds 目录失败: {}", e)))?;

    while let Some(entry) = entries.next_entry().await
        .map_err(|e| Error::internal(format!("读取目录项失败: {}", e)))?
    {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        let metadata = entry.metadata().await
            .map_err(|e| Error::internal(format!("读取文件元信息失败: {}", e)))?;
        let name = entry.file_name().to_string_lossy().to_string();
        let modified_at = metadata.modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        let is_default = name.starts_with("default") || name == "default.json";

        files.push(common::api::seed::SeedFileInfo {
            name,
            size: metadata.len(),
            modified_at,
            is_default,
        });
    }

    files.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    Ok(files)
}

/// 读取快照文件内容
pub async fn read_file(dir: &Path, name: &str) -> Result<common::api::seed::GetSeedFileResponse> {
    let file_name = validate_seed_filename(name)?;
    let path = dir.join(&file_name);
    let content = tokio::fs::read_to_string(&path).await
        .map_err(|e| Error::not_found(format!("快照文件不存在: {} ({})", file_name, e)))?;
    let size = content.len() as u64;
    Ok(common::api::seed::GetSeedFileResponse {
        name: file_name,
        content,
        size,
    })
}

/// 写入快照文件
pub async fn write_file(dir: &Path, name: &str, content: &str) -> Result<u64> {
    if !dir.exists() {
        std::fs::create_dir_all(dir)?;
    }
    let file_name = validate_seed_filename(name)?;
    let path = dir.join(&file_name);
    let size = content.len() as u64;
    tokio::fs::write(&path, content).await
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
    tokio::fs::remove_file(&path).await
        .map_err(|e| Error::internal(format!("删除快照文件失败: {}", e)))?;
    Ok(())
}
