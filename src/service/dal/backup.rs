//! Backup DAL 模块
//!
//! 职责：数据备份与恢复 - 将数据目录压缩成 tar.gz 归档，
//! 排除 `backups/` 和 `logs/` 子目录；通过 `_index.json` 索引管理备份元信息。

use crate::config;
use crate::pkg::RequestContext;
use common::error::{err, Result};
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use chrono::Utc;
use flate2::write::GzEncoder;
use flate2::Compression;
use md5::{Digest, Md5};
use tar::Builder;

// ==================== 数据结构 ====================

/// 单个备份的元信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackupInfo {
    /// 备份版本号（单调递增）
    pub version: u64,
    /// ISO8601 格式时间戳
    pub timestamp: String,
    /// 归档文件名，例如 `v1_20260717_153000.tar.gz`
    pub file_name: String,
    /// 归档文件字节数
    pub size_bytes: u64,
    /// 归档文件 MD5（十六进制小写）
    pub md5: String,
}

/// 备份索引文件（`_index.json`）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackupIndex {
    /// 索引结构版本
    pub schema_version: u32,
    /// 所有备份记录
    pub backups: Vec<BackupInfo>,
}

impl Default for BackupIndex {
    fn default() -> Self {
        Self {
            schema_version: 1,
            backups: Vec::new(),
        }
    }
}

// ==================== 单例管理 ====================

static BACKUP_DAL: OnceLock<Arc<dyn BackupDal + Send + Sync>> = OnceLock::new();

/// 获取 Backup DAL 单例
pub fn dal() -> Arc<dyn BackupDal + Send + Sync> {
    BACKUP_DAL.get().cloned().unwrap()
}

/// 初始化 Backup DAL
pub fn init() {
    let _ = BACKUP_DAL.set(Arc::new(BackupDalFsImpl));
}

// ==================== DAL 接口 ====================

/// Backup DAL 接口
#[async_trait::async_trait]
pub trait BackupDal: Send + Sync {
    /// 创建一份新备份，返回其元信息
    async fn create_backup(&self, ctx: RequestContext) -> Result<BackupInfo>;

    /// 列出所有备份（按 version 降序）
    async fn list_backups(&self, ctx: RequestContext) -> Result<Vec<BackupInfo>>;

    /// 删除指定版本的备份
    async fn delete_backup(&self, ctx: RequestContext, version: u64) -> Result<()>;

    /// 生成指定版本的恢复脚本（bash）
    async fn generate_restore_script(
        &self,
        ctx: RequestContext,
        version: u64,
    ) -> Result<String>;
}

// ==================== DAL 实现 ====================

/// 基于文件系统的 Backup DAL 实现
struct BackupDalFsImpl;

/// 备份索引文件名（存放在 `backups/` 目录下）
const INDEX_FILE_NAME: &str = "_index.json";
/// 备份子目录名
const BACKUP_DIR_NAME: &str = "backups";
/// 日志子目录名（备份时排除）
const LOGS_DIR_NAME: &str = "logs";
/// 顶层需要排除的子目录
const EXCLUDED_TOP_DIRS: &[&str] = &[BACKUP_DIR_NAME, LOGS_DIR_NAME];

#[async_trait::async_trait]
impl BackupDal for BackupDalFsImpl {
    async fn create_backup(&self, ctx: RequestContext) -> Result<BackupInfo> {
        let _ = ctx;
        let data_dir = config::get().base_data_path();
        let backups_dir = data_dir.join(BACKUP_DIR_NAME);

        // 确保备份目录存在
        std::fs::create_dir_all(&backups_dir)?;

        // 读取现有索引，计算下一个版本号
        let mut index = read_index(&backups_dir)?;
        let next_version = index.backups.iter().map(|b| b.version).max().unwrap_or(0) + 1;

        // 生成时间戳与文件名
        let now = Utc::now();
        let timestamp = now.to_rfc3339();
        let file_name = format!("v{}_{}.tar.gz", next_version, now.format("%Y%m%d_%H%M%S"));
        let backup_file_path = backups_dir.join(&file_name);

        // 创建 tar.gz：tar::Builder -> GzEncoder -> File
        let file = std::fs::File::create(&backup_file_path)?;
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);

        // 递归添加数据目录内容（排除 backups/ 和 logs/）
        append_dir_recursive(&mut builder, &data_dir, &data_dir, EXCLUDED_TOP_DIRS)?;

        // 关闭 builder 与 encoder，写出 gzip 尾部
        let encoder = builder.into_inner()?;
        let mut file = encoder.finish()?;
        use std::io::Write;
        file.flush()?;

        // 计算文件大小与 MD5
        let metadata = std::fs::metadata(&backup_file_path)?;
        let size_bytes = metadata.len();
        let md5 = compute_file_md5(&backup_file_path)?;

        let info = BackupInfo {
            version: next_version,
            timestamp,
            file_name,
            size_bytes,
            md5,
        };

        // 更新索引（追加新记录）
        index.backups.push(info.clone());
        write_index(&backups_dir, &index)?;

        Ok(info)
    }

    async fn list_backups(&self, ctx: RequestContext) -> Result<Vec<BackupInfo>> {
        let _ = ctx;
        let data_dir = config::get().base_data_path();
        let backups_dir = data_dir.join(BACKUP_DIR_NAME);

        if !backups_dir.exists() {
            return Ok(Vec::new());
        }

        let index = read_index(&backups_dir)?;
        if index.backups.is_empty() {
            // 防御性：索引为空时扫描 backups 目录重建
            let rebuilt = rebuild_index(&backups_dir)?;
            return Ok(rebuilt);
        }

        // 按 version 降序返回
        let mut backups = index.backups;
        backups.sort_by(|a, b| b.version.cmp(&a.version));
        Ok(backups)
    }

    async fn delete_backup(&self, ctx: RequestContext, version: u64) -> Result<()> {
        let _ = ctx;
        let data_dir = config::get().base_data_path();
        let backups_dir = data_dir.join(BACKUP_DIR_NAME);

        let mut index = read_index(&backups_dir)?;
        let pos = index
            .backups
            .iter()
            .position(|b| b.version == version)
            .ok_or_else(|| err!(ResourceNotFound, "Backup version {} not found", version))?;

        let info = index.backups.remove(pos);
        let file_path = backups_dir.join(&info.file_name);
        if file_path.exists() {
            std::fs::remove_file(&file_path)?;
        }

        write_index(&backups_dir, &index)?;
        Ok(())
    }

    async fn generate_restore_script(
        &self,
        ctx: RequestContext,
        version: u64,
    ) -> Result<String> {
        let _ = ctx;
        let data_dir = config::get().base_data_path();
        let backups_dir = data_dir.join(BACKUP_DIR_NAME);

        let index = read_index(&backups_dir)?;
        let info = index
            .backups
            .iter()
            .find(|b| b.version == version)
            .ok_or_else(|| err!(ResourceNotFound, "Backup version {} not found", version))?;

        let backup_file_path = backups_dir.join(&info.file_name);
        let data_dir_str = data_dir.to_string_lossy();

        let script = format!(
            r#"#!/bin/bash
# ai_orz 数据恢复脚本 - 恢复到版本 v{version}
# ⚠️ 警告：此操作将覆盖当前所有数据！

set -e

BACKUP_FILE="{backup_file}"
DATA_DIR="{data_dir}"

echo "请先停止 ai_orz 服务..."

# 备份当前数据
if [ -d "$DATA_DIR" ]; then
    mv "$DATA_DIR" "${{DATA_DIR}}.bak.$(date +%Y%m%d%H%M%S)"
fi

# 创建数据目录并解压
mkdir -p "$DATA_DIR"
tar -xzf "$BACKUP_FILE" -C "$DATA_DIR"

echo "恢复完成，请重启 ai_orz 服务"
"#,
            version = version,
            backup_file = backup_file_path.to_string_lossy(),
            data_dir = data_dir_str,
        );

        Ok(script)
    }
}

// ==================== 辅助函数 ====================

/// 读取备份索引文件。如果文件不存在，返回默认空索引。
fn read_index(backups_dir: &Path) -> Result<BackupIndex> {
    let index_path = backups_dir.join(INDEX_FILE_NAME);
    if !index_path.exists() {
        return Ok(BackupIndex::default());
    }
    let content = std::fs::read_to_string(&index_path)?;
    let index: BackupIndex = serde_json::from_str(&content)
        .map_err(|e| err!(Internal, "Failed to parse backup index: {}", e))?;
    Ok(index)
}

/// 写入备份索引文件
fn write_index(backups_dir: &Path, index: &BackupIndex) -> Result<()> {
    let index_path = backups_dir.join(INDEX_FILE_NAME);
    let content = serde_json::to_string_pretty(index)
        .map_err(|e| err!(Internal, "Failed to serialize backup index: {}", e))?;
    std::fs::write(&index_path, content)?;
    Ok(())
}

/// 递归把目录内容添加到 tar builder
///
/// - `builder`: tar Builder
/// - `root`: 数据根目录（用于计算归档内相对路径）
/// - `current`: 当前正在遍历的目录
/// - `excluded`: 仅在顶层（`current == root`）需要排除的子目录名
fn append_dir_recursive<W: std::io::Write>(
    builder: &mut Builder<W>,
    root: &Path,
    current: &Path,
    excluded: &[&str],
) -> Result<()> {
    let entries = std::fs::read_dir(current)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // 只在顶层（current == root）排除指定目录
        if current == root && excluded.contains(&name_str.as_ref()) {
            continue;
        }

        // 归档内相对路径（用正斜杠以保证跨平台解压）
        let relative = path.strip_prefix(root).unwrap_or(&path);
        let tar_path = to_tar_path(relative);

        let metadata = std::fs::symlink_metadata(&path)?;
        let file_type = metadata.file_type();

        if file_type.is_dir() {
            builder.append_dir(&tar_path, &path)?;
            append_dir_recursive(builder, root, &path, excluded)?;
        } else if file_type.is_file() {
            let mut file = std::fs::File::open(&path)?;
            builder.append_file(&tar_path, &mut file)?;
        } else if file_type.is_symlink() {
            if let Ok(target) = std::fs::read_link(&path) {
                let mut header = tar::Header::new_gnu();
                header.set_entry_type(tar::EntryType::Symlink);
                header.set_size(0);
                header.set_mode(0o777);
                header.set_mtime(system_time_to_secs(metadata.modified().ok()));
                header.set_link_name(target.as_path())?;
                header.set_cksum();
                let mut empty = std::io::empty();
                builder.append_data(&mut header, &tar_path, &mut empty)?;
            }
        }
        // 其他类型（fifo、socket 等）跳过
    }
    Ok(())
}

/// 把 `SystemTime` 转为 unix 秒数
fn system_time_to_secs(t: Option<std::time::SystemTime>) -> u64 {
    match t {
        Some(t) => t
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        None => 0,
    }
}

/// 把 `&Path` 转换为正斜杠分隔的字符串，作为 tar 内部路径
fn to_tar_path(p: &Path) -> String {
    p.components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect::<Vec<_>>()
        .join("/")
}

/// 计算文件的 MD5 哈希（十六进制小写）
fn compute_file_md5(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Md5::new();
    let mut buffer = [0u8; 8192];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    let digest = hasher.finalize();
    Ok(format!("{:x}", digest))
}

/// 扫描 `backups/` 目录下的 `.tar.gz` 文件，重建索引（防御性）
fn rebuild_index(backups_dir: &Path) -> Result<Vec<BackupInfo>> {
    let mut backups = Vec::new();
    let entries = std::fs::read_dir(backups_dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".tar.gz") {
            continue;
        }
        // 从文件名解析版本号 `v{version}_...`
        let version = name
            .strip_prefix('v')
            .and_then(|s| s.split('_').next())
            .and_then(|s| s.parse::<u64>().ok());
        let metadata = std::fs::metadata(&path)?;
        let modified = metadata
            .modified()
            .ok()
            .map(|t| {
                chrono::DateTime::<Utc>::from(t).to_rfc3339()
            })
            .unwrap_or_default();
        let md5 = compute_file_md5(&path)?;
        backups.push(BackupInfo {
            version: version.unwrap_or(0),
            timestamp: modified,
            file_name: name,
            size_bytes: metadata.len(),
            md5,
        });
    }
    backups.sort_by(|a, b| b.version.cmp(&a.version));
    Ok(backups)
}
