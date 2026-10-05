//! tests 单元测试（拆分自 backup.rs）
//!
//! 文件瘦身：原 637 行 → 395 行，测试体 243 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use std::io::Write;

/// 测试 `to_tar_path` 把 Path 转为正斜杠分隔字符串
#[test]
fn test_to_tar_path_basic() {
    // 单层路径
    let p = Path::new("foo.txt");
    assert_eq!(to_tar_path(p), "foo.txt");

    // 多层路径（使用平台分隔符构造）
    let p = Path::new("a").join("b").join("c.txt");
    let tar_path = to_tar_path(&p);
    assert_eq!(tar_path, "a/b/c.txt");
}

/// 测试 `system_time_to_secs`：None 与已知时间点
#[test]
fn test_system_time_to_secs() {
    // None 应返回 0
    assert_eq!(system_time_to_secs(None), 0u64);

    // UNIX_EPOCH 应返回 0
    assert_eq!(system_time_to_secs(Some(std::time::UNIX_EPOCH)), 0u64);
}

/// 测试 `compute_file_md5`：对已知内容计算 MD5
#[test]
fn test_compute_file_md5_known_content() {
    // "hello" 的 MD5 = 5d41402abc4b2a76b9719d911017c592
    let dir = tempfile::tempdir().expect("create temp dir");
    let file_path = dir.path().join("hello.txt");
    std::fs::write(&file_path, b"hello").expect("write file");

    let md5 = compute_file_md5(&file_path).expect("compute md5");
    assert_eq!(md5, "5d41402abc4b2a76b9719d911017c592");
}

/// 测试 `compute_file_md5`：空文件 MD5 = d41d8cd98f00b204e9800998ecf8427e
#[test]
fn test_compute_file_md5_empty_file() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let file_path = dir.path().join("empty.bin");
    std::fs::write(&file_path, b"").expect("write empty file");

    let md5 = compute_file_md5(&file_path).expect("compute md5");
    assert_eq!(md5, "d41d8cd98f00b204e9800998ecf8427e");
}

/// 测试 `read_index` / `write_index` 往返：写入后读取应得到等价内容
#[test]
fn test_read_write_index_roundtrip() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let backups_dir = dir.path();

    // 初始时索引文件不存在，应返回默认空索引
    let initial = read_index(backups_dir).expect("read initial index");
    assert!(initial.backups.is_empty());
    assert_eq!(initial.schema_version, 1);

    // 写入一个含两条记录的索引
    let mut index = BackupIndex::default();
    index.backups.push(BackupInfo {
        version: 1,
        timestamp: "2026-07-17T10:00:00Z".to_string(),
        file_name: "v1_20260717_100000.tar.gz".to_string(),
        size_bytes: 1024,
        md5: "abc123".to_string(),
    });
    index.backups.push(BackupInfo {
        version: 2,
        timestamp: "2026-07-17T11:00:00Z".to_string(),
        file_name: "v2_20260717_110000.tar.gz".to_string(),
        size_bytes: 2048,
        md5: "def456".to_string(),
    });
    write_index(backups_dir, &index).expect("write index");

    // 读取回来，校验内容
    let read_back = read_index(backups_dir).expect("read back index");
    assert_eq!(read_back.schema_version, 1);
    assert_eq!(read_back.backups.len(), 2);
    assert_eq!(read_back.backups[0].version, 1);
    assert_eq!(read_back.backups[0].file_name, "v1_20260717_100000.tar.gz");
    assert_eq!(read_back.backups[0].size_bytes, 1024);
    assert_eq!(read_back.backups[0].md5, "abc123");
    assert_eq!(read_back.backups[1].version, 2);
    assert_eq!(read_back.backups[1].md5, "def456");

    // 索引文件确实落盘
    assert!(backups_dir.join(INDEX_FILE_NAME).exists());
}

/// 测试 `rebuild_index`：在空目录（无 .tar.gz）时应返回空 Vec
#[test]
fn test_rebuild_index_empty_dir() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let result = rebuild_index(dir.path()).expect("rebuild from empty dir");
    assert!(result.is_empty());
}

/// 测试 `rebuild_index`：从 `v{N}_*.tar.gz` 文件名解析版本号并按降序返回
#[test]
fn test_rebuild_index_parses_version_and_sorts_desc() {
    let dir = tempfile::tempdir().expect("create temp dir");

    // 创建 3 个 .tar.gz 文件（内容随意，仅为触发扫描）
    // 故意乱序创建以验证排序
    for (ver, suffix) in [
        (2u64, "20260717_110000"),
        (1, "20260717_100000"),
        (3, "20260717_120000"),
    ] {
        let name = format!("v{}_{}.tar.gz", ver, suffix);
        let path = dir.path().join(&name);
        std::fs::write(&path, b"dummy backup content").expect("write tar.gz");
    }

    // 添加一个非 .tar.gz 文件，应被忽略
    std::fs::write(dir.path().join("notes.txt"), b"ignore me").expect("write txt");

    let result = rebuild_index(dir.path()).expect("rebuild index");
    assert_eq!(result.len(), 3);

    // 降序：3, 2, 1
    assert_eq!(result[0].version, 3);
    assert_eq!(result[1].version, 2);
    assert_eq!(result[2].version, 1);

    // file_name 应反映原始文件名
    assert_eq!(result[0].file_name, "v3_20260717_120000.tar.gz");
    // size_bytes 应等于文件实际大小
    assert_eq!(result[0].size_bytes, "dummy backup content".len() as u64);
    // md5 应为非空十六进制串
    assert!(!result[0].md5.is_empty());
}

/// 测试 `append_dir_recursive`：把目录树追加到 tar builder，
/// 验证排除顶层 `backups/` 与 `logs/` 子目录的行为。
#[test]
fn test_append_dir_recursive_excludes_top_level_dirs() {
    let data_dir = tempfile::tempdir().expect("create data dir");
    let root = data_dir.path();
    let out_dir = tempfile::tempdir().expect("create output dir");

    // 在根目录下创建若干文件和子目录
    std::fs::write(root.join("keep.txt"), b"keep").expect("write keep.txt");
    std::fs::create_dir_all(root.join("sub")).expect("mkdir sub");
    std::fs::write(root.join("sub").join("inner.txt"), b"inner").expect("write inner.txt");

    // 这两个目录在顶层应被排除
    std::fs::create_dir_all(root.join(BACKUP_DIR_NAME)).expect("mkdir backups");
    std::fs::write(root.join(BACKUP_DIR_NAME).join("v1.tar.gz"), b"backup").expect("write backup");
    std::fs::create_dir_all(root.join(LOGS_DIR_NAME)).expect("mkdir logs");
    std::fs::write(
        root.join(LOGS_DIR_NAME).join("ai_orz.log.2026-07-17"),
        b"log",
    )
    .expect("write log");

    // 构建 tar.gz（输出文件放在遍历目录外，避免遍历时读到正在写入的归档文件导致 flaky）
    let archive_path = out_dir.path().join("output.tar.gz");
    let file = std::fs::File::create(&archive_path).expect("create archive");
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = Builder::new(encoder);
    append_dir_recursive(&mut builder, root, root, EXCLUDED_TOP_DIRS).expect("append dir");
    let encoder = builder.into_inner().expect("close builder");
    let mut file = encoder.finish().expect("finish encoder");
    file.flush().expect("flush");

    // 解压并验证归档内容：应包含 keep.txt 和 sub/inner.txt，不应包含 backups/ 或 logs/ 下的内容
    let archive_bytes = std::fs::read(&archive_path).expect("read archive");
    let decoder = flate2::read::GzDecoder::new(std::io::Cursor::new(archive_bytes));
    let mut tar_archive = tar::Archive::new(decoder);
    let mut entries: Vec<String> = tar_archive
        .entries()
        .expect("list entries")
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            e.path()
                .ok()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
        })
        .collect();
    entries.sort();

    // 期望归档中至少出现这两个文件（可能还包含目录条目 sub/，故用 contains 判断）
    assert!(
        entries.iter().any(|p| p == "keep.txt"),
        "归档应包含 keep.txt, 实际: {:?}",
        entries
    );
    assert!(
        entries
            .iter()
            .any(|p| p == "sub/inner.txt" || p == "sub\\inner.txt"),
        "归档应包含 sub/inner.txt, 实际: {:?}",
        entries
    );
    // 不应出现 backups/ 或 logs/ 下的任何条目
    assert!(
        !entries
            .iter()
            .any(|p| p.starts_with("backups/") || p.starts_with("backups\\")),
        "归档不应包含 backups/ 子目录, 实际: {:?}",
        entries
    );
    assert!(
        !entries
            .iter()
            .any(|p| p.starts_with("logs/") || p.starts_with("logs\\")),
        "归档不应包含 logs/ 子目录, 实际: {:?}",
        entries
    );
}

/// 测试 `BackupIndex::default`：默认 schema_version=1，backups 为空
#[test]
fn test_backup_index_default() {
    let idx = BackupIndex::default();
    assert_eq!(idx.schema_version, 1);
    assert!(idx.backups.is_empty());
}

/// 测试 `BackupInfo` 序列化/反序列化往返
#[test]
fn test_backup_info_serde_roundtrip() {
    let info = BackupInfo {
        version: 42,
        timestamp: "2026-07-17T12:34:56Z".to_string(),
        file_name: "v42_20260717_123456.tar.gz".to_string(),
        size_bytes: 4096,
        md5: "deadbeef".to_string(),
    };
    let json = serde_json::to_string(&info).expect("serialize");
    let back: BackupInfo = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.version, 42);
    assert_eq!(back.timestamp, "2026-07-17T12:34:56Z");
    assert_eq!(back.file_name, "v42_20260717_123456.tar.gz");
    assert_eq!(back.size_bytes, 4096);
    assert_eq!(back.md5, "deadbeef");
}
