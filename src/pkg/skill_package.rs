//! 技能包（zip）打包：路径 + 原样字节 + unix 可执行位
//!
//! 使用方是「产出一个技能包」的所有链路：快照导出（`system::seed`，内置技能包）、
//! 技能整包导入的测试夹具。它是纯格式工具（只依赖 `zip` crate），没有业务规则。
//!
//! ## 为什么在 pkg
//!
//! 原先长在 `system::seed::bundle`，但技能包格式不是 seed 的私有物 —— 技能域自己
//! 也要造包/收包，借用方因此形成 `hr → system` 的域间引用。格式工具下沉到依赖链
//! 底端后，两侧各自引用 pkg，域之间没有调用。
//!
//! ⚠️ 打包必须保留 unix mode 与原始字节：逐文件按文本读会把二进制附件读成空、
//! 也救不了无扩展名的可执行件。

use common::error::{Error, Result};
use std::io::{Cursor, Write};

/// 待打包的技能文件条目
pub struct SkillFileEntry {
    /// 相对技能目录的路径（zip 内条目名，如 `skill.md`、`scripts/run.sh`）
    pub path: String,
    /// 文件字节（原样保真，不做 UTF-8 解码）
    pub bytes: Vec<u8>,
    /// 是否带可执行位（写入 zip 的 unix mode）
    pub executable: bool,
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

#[cfg(test)]
mod tests {
    use super::{SkillFileEntry, pack_skill_zip};

    /// 技能包：字节保真 + unix mode 保真（二进制附件与可执行件不能被文本通道破坏）
    #[test]
    fn skill_package_preserves_bytes_and_unix_mode() {
        let binary: Vec<u8> = vec![0x00, 0xFF, 0xFE, 0x01, 0x80];
        let entries = vec![
            SkillFileEntry {
                path: "skill.md".to_string(),
                bytes: b"# main".to_vec(),
                executable: false,
            },
            SkillFileEntry {
                path: "scripts/run.sh".to_string(),
                bytes: b"echo hi".to_vec(),
                executable: true,
            },
            SkillFileEntry {
                path: "assets/blob.bin".to_string(),
                bytes: binary.clone(),
                executable: false,
            },
        ];

        let zip_bytes = pack_skill_zip(&entries).unwrap();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&zip_bytes)).unwrap();
        assert_eq!(archive.len(), 3);

        for entry in &entries {
            let mut file = archive.by_name(&entry.path).unwrap();
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut file, &mut buf).unwrap();
            assert_eq!(buf, entry.bytes, "{} 字节必须原样保真", entry.path);
            let exec = file.unix_mode().is_some_and(|m| m & 0o111 != 0);
            assert_eq!(exec, entry.executable, "{} 可执行位必须保真", entry.path);
        }
    }
}
