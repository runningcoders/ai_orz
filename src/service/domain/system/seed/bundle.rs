//! 快照压缩包（bundle）打包 / 解包
//!
//! 快照产物是一个 zip：`seed.json`（主文件：组织 / 用户 / Provider / Agent / 技能元信息）
//! + `skills/<skill_id>.zip`（每个技能一个技能包，保真附件字节与可执行位）。
//!
//! **为什么不再把技能内容塞进主文件**：附件可以是图片 / 二进制可执行件，
//! 按 UTF-8 读必然失败（旧实现静默降级成空），base64 内嵌又让主文件膨胀到不可读、
//! 且丢掉 unix mode。整包压缩既保真又让主文件保持人类可读可 diff。

use crate::service::domain::system::seed::defs::SeedSnapshot;
use common::error::{Error, Result};
use std::collections::HashMap;
use std::io::{Cursor, Read, Write};

/// 主文件在压缩包内的固定条目名
pub const SEED_MANIFEST: &str = "seed.json";
/// 技能包在压缩包内的目录前缀
pub const SKILL_PACKAGE_DIR: &str = "skills/";

/// 技能包名（由 skill_id 确定性推导，主文件与打包侧共用）
pub fn skill_package_name(skill_id: &str) -> String {
    format!("{}{}.zip", SKILL_PACKAGE_DIR, skill_id)
}

/// 待打包的技能文件条目
pub struct SkillFileEntry {
    /// 相对技能目录的路径（zip 内条目名，如 `skill.md`、`scripts/run.sh`）
    pub path: String,
    /// 文件字节（原样保真，不做 UTF-8 解码）
    pub bytes: Vec<u8>,
    /// 是否带可执行位（写入 zip 的 unix mode）
    pub executable: bool,
}

/// 技能包（名称 = 压缩包内相对路径，bytes = 技能 zip）
pub struct SkillPackage {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// 把一个技能的文件列表打成 zip（保留 unix mode，供导入侧还原可执行位）
pub fn pack_skill_zip(entries: &[SkillFileEntry]) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut buf));
        for entry in entries {
            let mode = if entry.executable { 0o755 } else { 0o644 };
            writer
                .start_file(
                    entry.path.clone(),
                    zip::write::SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated)
                        .unix_permissions(mode),
                )
                .map_err(|e| Error::internal(format!("技能包写入条目失败: {}", e)))?;
            writer
                .write_all(&entry.bytes)
                .map_err(|e| Error::internal(format!("技能包写入内容失败: {}", e)))?;
        }
        writer
            .finish()
            .map_err(|e| Error::internal(format!("技能包封口失败: {}", e)))?;
    }
    Ok(buf)
}

/// 组装整体压缩包：主文件 + 各技能包
pub fn build_bundle(snapshot: &SeedSnapshot, packages: &[SkillPackage]) -> Result<Vec<u8>> {
    let manifest = serde_json::to_string_pretty(snapshot)
        .map_err(|e| Error::internal(format!("快照序列化失败: {}", e)))?;

    let mut buf = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut buf));
        let opts = || {
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(0o644)
        };
        writer
            .start_file(SEED_MANIFEST, opts())
            .map_err(|e| Error::internal(format!("快照主文件写入失败: {}", e)))?;
        writer
            .write_all(manifest.as_bytes())
            .map_err(|e| Error::internal(format!("快照主文件写入失败: {}", e)))?;

        for pkg in packages {
            writer
                .start_file(pkg.name.clone(), opts())
                .map_err(|e| Error::internal(format!("技能包 {} 写入失败: {}", pkg.name, e)))?;
            writer
                .write_all(&pkg.bytes)
                .map_err(|e| Error::internal(format!("技能包 {} 写入失败: {}", pkg.name, e)))?;
        }
        writer
            .finish()
            .map_err(|e| Error::internal(format!("快照压缩包封口失败: {}", e)))?;
    }
    Ok(buf)
}

/// 拆包：返回主文件快照 + 技能包 map（key = 压缩包内相对路径）
///
/// 不落盘、不按 zip 内条目名建目录——技能包字节交给导入侧走既有 zip 导入通路
/// （那边有文件名白名单与预算护栏），规避 zip slip。
pub fn read_bundle(bytes: &[u8]) -> Result<(SeedSnapshot, HashMap<String, Vec<u8>>)> {
    let manifest = read_manifest(bytes)?;
    let snapshot: SeedSnapshot = serde_json::from_str(&manifest)
        .map_err(|e| Error::bad_request(format!("快照主文件解析失败: {}", e)))?;

    let mut packages = HashMap::new();
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| Error::bad_request(format!("快照压缩包解包失败: {}", e)))?;
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| Error::bad_request(format!("压缩包条目读取失败: {}", e)))?;
        let name = file.name().to_string();
        if file.is_dir() || name == SEED_MANIFEST {
            continue;
        }
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)
            .map_err(|e| Error::bad_request(format!("压缩包条目 {} 读取失败: {}", name, e)))?;
        packages.insert(name, buf);
    }

    Ok((snapshot, packages))
}

/// 只读主文件（预览 / diff 用，不加载技能包）
pub fn read_manifest(bytes: &[u8]) -> Result<String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| Error::bad_request(format!("快照压缩包解包失败: {}", e)))?;
    let mut file = archive
        .by_name(SEED_MANIFEST)
        .map_err(|_| Error::bad_request(format!("压缩包内缺少 {}", SEED_MANIFEST)))?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)
        .map_err(|e| Error::bad_request(format!("{} 读取失败: {}", SEED_MANIFEST, e)))?;
    String::from_utf8(buf)
        .map_err(|e| Error::bad_request(format!("{} 不是合法 UTF-8: {}", SEED_MANIFEST, e)))
}
