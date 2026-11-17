//! 按天 JSONL 旁车索引的通用底层（sidecar index）
//!
//! 每个 `{date}.jsonl` 配一个 `{date}.jsonl.idx`：索引每行是一条 [`IndexRecord<F>`]，
//! `offset`/`len` 是原文字节坐标，`fields` 是业务自定义的**小**过滤字段
//! （不含 input/rounds 这类大文本原文）。查询只读小索引做过滤/排序/分页，
//! 再按字节坐标 seek 回原文只反序列化命中行——把「全目录 + 全文件 + 每行全量
//! 反序列化」从 O(总载荷) 降到 O(索引大小 + 命中行载荷)。
//!
//! 本模块只做**与业务无关的机械部分**：分片发现、watermark 校验、增量补齐、
//! 坏行重建、按坐标读回。三类 trace（工具调用 / 记忆 / 思考）各自的过滤字段、
//! 匹配语义、排序与合并逻辑留在自己的模块里，通过泛型 `F` 和解析闭包注入。
//!
//! ## 一致性不变量
//!
//! `watermark`（最后一条索引行的 `offset + len`）**恒 ≤ 原文文件大小**：
//! - 写时维护在原文 append 成功后、且调用方持有 `APPEND_LOCK` 的临界区内追加索引；
//! - 崩溃 / 索引写失败只会让 watermark **落后**，由读路径 [`Self::ensure_fresh`] 补齐；
//! - 索引出现坏行时全量重建；索引头部缺失（前缀存在可解析行却未索引）时也重建。
//!
//! ⚠️ 「索引头部缺失」**不能**用「首条记录 offset != 0」判定：原文首行本身是坏行时，
//! 首条合法记录的 offset 必然 > 0，那样会误判成永久重建，并在并发查询下因反复
//! `set_len(0)` 让另一个查询读到半截索引。正确判定见 [`Self::prefix_has_parseable_line`]。
//!
//! 索引是纯派生数据：删掉任意 `.idx` 文件后下次查询自动重建，不影响原文。

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::marker::PhantomData;
use std::path::Path;

use anyhow::Result;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// 一条索引记录：原文字节坐标 + 业务过滤字段
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexRecord<F> {
    pub offset: u64,
    pub len: u64,
    pub fields: F,
}

impl<F> IndexRecord<F> {
    pub fn end_offset(&self) -> u64 {
        self.offset + self.len
    }
}

/// 校验结果（内部）
struct Inspect {
    watermark: u64,
    /// 索引文件自身含无法反序列化的坏行
    bad_record: bool,
    /// 首条合法索引记录的 offset（用于判定「头部缺失」）
    first_offset: Option<u64>,
}

/// 旁车索引句柄：只持有 trace 目录，无全局状态，便于临时目录隔离测试
#[derive(Debug, Clone)]
pub struct JsonlSidecar<F> {
    trace_dir: std::path::PathBuf,
    _marker: PhantomData<fn() -> F>,
}

impl<F> JsonlSidecar<F>
where
    F: Serialize + DeserializeOwned,
{
    pub fn new(trace_dir: std::path::PathBuf) -> Self {
        Self {
            trace_dir,
            _marker: PhantomData,
        }
    }

    fn data_path(&self, date: &str) -> std::path::PathBuf {
        self.trace_dir.join(format!("{date}.jsonl"))
    }

    fn index_path(&self, date: &str) -> std::path::PathBuf {
        self.trace_dir.join(format!("{date}.jsonl.idx"))
    }

    /// 列出有原文的分片日期（`YYYYMMDD`，按文件名排序保证恢复顺序稳定）
    pub fn data_dates(&self) -> Result<Vec<String>> {
        let mut dates = Vec::new();
        if !self.trace_dir.exists() {
            return Ok(dates);
        }
        for entry in fs::read_dir(&self.trace_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let path = entry.path();
            if !path.extension().is_some_and(|ext| ext == "jsonl") {
                continue;
            }
            let Some(date) = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .filter(|stem| stem.len() == 8 && stem.bytes().all(|b| b.is_ascii_digit()))
            else {
                continue;
            };
            dates.push(date.to_string());
        }
        dates.sort();
        Ok(dates)
    }

    /// 写时维护：在原文已写、且调用方持有 `APPEND_LOCK` 的临界区内追加一条索引。
    ///
    /// 索引写失败返回 Err，但调用方只告警不传播——原文已落盘，最多索引落后，
    /// 读路径会补齐。本方法自身不拿锁（已在锁内），避免重入死锁。
    pub fn append_record(&self, date: &str, offset: u64, len: u64, fields: F) -> Result<()> {
        let record = IndexRecord {
            offset,
            len,
            fields,
        };
        let mut line = serde_json::to_string(&record)?;
        line.push('\n');
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.index_path(date))?;
        file.write_all(line.as_bytes())?;
        file.flush()?;
        Ok(())
    }

    /// 读索引全部记录（坏行跳过）。正常路径只读这个轻量文件，不碰原文大载荷。
    pub fn read_records(&self, date: &str) -> Result<Vec<IndexRecord<F>>> {
        let path = self.index_path(date);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let mut records = Vec::new();
        let file = File::open(path)?;
        for line in BufReader::new(file).lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(record) = serde_json::from_str::<IndexRecord<F>>(&line) {
                records.push(record);
            }
        }
        Ok(records)
    }

    /// 按字节坐标精确读回一行原文并反序列化（O(命中行载荷)，不扫整文件）
    pub fn read_line_at<T: DeserializeOwned>(
        &self,
        date: &str,
        offset: u64,
        len: u64,
    ) -> Result<T> {
        let mut file = File::open(self.data_path(date))?;
        file.seek(SeekFrom::Start(offset))?;
        let mut buf = vec![0u8; len as usize];
        file.read_exact(&mut buf)?;
        let line = std::str::from_utf8(&buf)?.trim_end_matches('\n');
        Ok(serde_json::from_str(line)?)
    }

    /// **在 `APPEND_LOCK` 临界区内**一次性完成「恢复 + 读取」。
    ///
    /// 把 `ensure_fresh`（可能 `set_len(0)` 全量重建）与 `read_records` 绑进同一把锁，
    /// 消除「线程 A 重建清空索引 / 线程 B 正读到半截」的竞态。三个适配层的查询统一走这里。
    pub fn records_fresh(
        &self,
        date: &str,
        parse: impl Fn(&[u8], u64) -> Option<F>,
    ) -> Result<Vec<IndexRecord<F>>> {
        self.ensure_fresh(date, parse)?;
        self.read_records(date)
    }

    /// 读索引，检查其自身完整性与 watermark。**调用方必须持有 `APPEND_LOCK`**。
    fn inspect(&self, date: &str) -> Result<Inspect> {
        let path = self.index_path(date);
        if !path.exists() {
            return Ok(Inspect {
                watermark: 0,
                bad_record: false,
                first_offset: None,
            });
        }
        let file = File::open(path)?;
        let mut watermark = 0u64;
        let mut first_offset = None;
        for line in BufReader::new(file).lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<IndexRecord<F>>(&line) {
                Ok(record) => {
                    if first_offset.is_none() {
                        first_offset = Some(record.offset);
                    }
                    watermark = record.end_offset();
                }
                Err(_) => {
                    return Ok(Inspect {
                        watermark: 0,
                        bad_record: true,
                        first_offset: None,
                    });
                }
            }
        }
        Ok(Inspect {
            watermark,
            bad_record: false,
            first_offset,
        })
    }

    /// 原文 `[0, prefix_len)` 内是否存在可解析行。
    ///
    /// 用于精确判定「索引头部缺失」：若这段原文里有能解析成 `F` 的行，说明本该被索引
    /// 却不在索引里（头部被删/截断）→ 需要重建；若整段都是坏行，则是原文首行损坏的
    /// 正常结果 → 不重建。彻底替代会误判的 `first_offset != 0`。
    fn prefix_has_parseable_line(
        &self,
        data_path: &Path,
        prefix_len: u64,
        parse: &impl Fn(&[u8], u64) -> Option<F>,
    ) -> Result<bool> {
        let file = File::open(data_path)?;
        let mut reader = BufReader::new(file);
        let mut line = Vec::new();
        let mut pos = 0u64;
        loop {
            line.clear();
            let read = reader.read_until(b'\n', &mut line)?;
            if read == 0 || pos + read as u64 > prefix_len {
                break;
            }
            if parse(&line, 0).is_some() {
                return Ok(true);
            }
            pos += read as u64;
        }
        Ok(false)
    }

    /// 数原文前 `end` 字节内的换行符个数 = 这段区间覆盖的物理行数。
    ///
    /// 增量补齐的物理行号起点用这个真实值，而非「索引合法记录数」——后者在原文含坏行时
    /// 会偏小（坏行不落索引却占物理行），导致 memory 引用链的 `line_number` 错位读到别的行。
    fn count_physical_lines(&self, data_path: &Path, end: u64) -> Result<u64> {
        let mut file = File::open(data_path)?;
        file.seek(SeekFrom::Start(end))?;
        // 分段读避免为大文件一次性分配 end 字节
        let mut buf = [0u8; 8192];
        let mut remaining = end;
        let mut count = 0u64;
        while remaining > 0 {
            let want = remaining.min(buf.len() as u64) as usize;
            // 回退文件指针到当前段起点：seek 已在 end，逐段向前需要重新定位
            let seg_start = end - remaining;
            file.seek(SeekFrom::Start(seg_start))?;
            let read = file.read(&mut buf[..want])?;
            if read == 0 {
                break;
            }
            count += buf[..read].iter().filter(|&&b| b == b'\n').count() as u64;
            remaining -= read as u64;
        }
        Ok(count)
    }

    /// 确保索引与原文同步。**调用方必须持有 `APPEND_LOCK`**（防与写并发）。
    ///
    /// `parse` 把原文行字节 + 该行的物理行号（0-based）解析成业务过滤字段 `F`；
    /// 返回 `None` 表示该行不可解析，跳过（与读索引跳过坏行同口径）。行号按物理行
    /// 递增（坏行也占号），与 append-only 原文严格对齐——memory 的引用链坐标依赖它。
    /// 只索引到最后一个完整 `\n`，残片留给下次。
    pub fn ensure_fresh(&self, date: &str, parse: impl Fn(&[u8], u64) -> Option<F>) -> Result<()> {
        let data_path = self.data_path(date);
        if !data_path.exists() {
            return Ok(());
        }
        let data_len = fs::metadata(&data_path)?.len();
        let inspect = self.inspect(date)?;

        // 头部缺失判定：首条索引 offset > 0 时，检查它之前的原文是否含可解析行。
        // 有 → 头部被删需重建；全是坏行 → 原文首行损坏的正常结果，不重建。
        let head_missing = match inspect.first_offset {
            Some(offset) if offset > 0 => {
                self.prefix_has_parseable_line(&data_path, offset, &parse)?
            }
            _ => false,
        };
        let need_rebuild = inspect.bad_record || head_missing;

        if !need_rebuild && inspect.watermark == data_len {
            return Ok(());
        }

        let start = if need_rebuild || inspect.watermark > data_len {
            0
        } else {
            inspect.watermark
        };

        let index_path = self.index_path(date);
        let mut index_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&index_path)?;
        if start == 0 {
            // 全量重建：清空索引。append 模式下 set_len(0) 即从头开始写。
            index_file.set_len(0)?;
        }

        // 增量补齐的物理行号起点 = 原文 [0, start) 的真实物理行数（数 \n）。
        // 全量重建从 0 起算。坏行也占物理行，故与写时 count_lines 口径一致。
        let mut line_no = if start == 0 {
            0
        } else {
            self.count_physical_lines(&data_path, start)?
        };
        let data_file = File::open(&data_path)?;
        let mut reader = BufReader::new(data_file);
        reader.seek(SeekFrom::Start(start))?;
        let mut pos = start;
        loop {
            let mut line = Vec::new();
            let read = reader.read_until(b'\n', &mut line)?;
            if read == 0 || !line.ends_with(b"\n") {
                // 文件结尾残片（无换行）不索引，watermark 不前进到它
                break;
            }
            if let Some(fields) = parse(&line, line_no) {
                let record = IndexRecord {
                    offset: pos,
                    len: read as u64,
                    fields,
                };
                let mut encoded = serde_json::to_string(&record)?;
                encoded.push('\n');
                index_file.write_all(encoded.as_bytes())?;
            }
            pos += read as u64;
            line_no += 1;
        }
        index_file.flush()?;
        Ok(())
    }
}
