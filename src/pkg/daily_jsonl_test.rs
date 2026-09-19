//! Daily JSONL 单元测试

use crate::pkg::daily_jsonl::DailyJsonlWriter;
use serde::{Deserialize, Serialize};
use tempfile::tempdir;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct TestLogEntry {
    id: usize,
    message: String,
    value: f64,
}

#[test]
fn test_daily_jsonl_append_and_read() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let base_path = temp_dir.path().to_path_buf();

    let writer = DailyJsonlWriter::new(base_path.clone());

    // Append multiple entries
    let entry1 = TestLogEntry {
        id: 1,
        message: "first message".to_string(),
        value: 1.5,
    };
    let (date1, line1) = writer.append(&entry1)?;

    let entry2 = TestLogEntry {
        id: 2,
        message: "second message".to_string(),
        value: 2.7,
    };
    let (date2, line2) = writer.append(&entry2)?;

    let entry3 = TestLogEntry {
        id: 3,
        message: "third message".to_string(),
        value: 3.15,
    };
    let (date3, line3) = writer.append(&entry3)?;

    // Check line numbers are 0-indexed, dates are same (assuming test runs within same day)
    assert_eq!(line1, 0);
    assert_eq!(line2, 1);
    assert_eq!(line3, 2);
    assert_eq!(date1, date2);
    assert_eq!(date2, date3);

    // Read back each line
    let json1 = writer.read_line(&date1, 0)?;
    let json2 = writer.read_line(&date1, 1)?;
    let json3 = writer.read_line(&date1, 2)?;

    // Parse and verify
    let parsed1: TestLogEntry = serde_json::from_str(&json1)?;
    let parsed2: TestLogEntry = serde_json::from_str(&json2)?;
    let parsed3: TestLogEntry = serde_json::from_str(&json3)?;

    assert_eq!(parsed1, entry1);
    assert_eq!(parsed2, entry2);
    assert_eq!(parsed3, entry3);

    Ok(())
}

#[test]
fn test_daily_jsonl_read_line_json() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let base_path = temp_dir.path().to_path_buf();

    let writer = DailyJsonlWriter::new(base_path.clone());

    let entry = TestLogEntry {
        id: 42,
        message: "test deserialization".to_string(),
        value: 99.9,
    };
    let (date, line) = writer.append(&entry)?;

    let parsed: TestLogEntry = writer.read_line_json(&date, line)?;
    assert_eq!(parsed, entry);

    Ok(())
}

#[test]
fn test_daily_jsonl_line_out_of_range_returns_error() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let base_path = temp_dir.path().to_path_buf();

    let writer = DailyJsonlWriter::new(base_path.clone());
    let (date, _line) = writer.append(&"hello world")?;

    // Line 0 exists, line 1 does not
    assert!(writer.read_line(&date, 0).is_ok());
    assert!(writer.read_line(&date, 1).is_err());

    Ok(())
}

#[test]
fn test_daily_jsonl_nonexistent_date_returns_error() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let base_path = temp_dir.path().to_path_buf();

    let writer = DailyJsonlWriter::new(base_path.clone());

    // File doesn't exist for this date
    assert!(writer.read_line("19990101", 0).is_err());

    Ok(())
}

/// `append_batch` 只数一次行：整批位置连续，且与单条 `append` 混用时行号接着走。
#[test]
fn test_daily_jsonl_append_batch_counts_lines_once() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let writer = DailyJsonlWriter::new(temp_dir.path());

    let entries: Vec<TestLogEntry> = (1..=3)
        .map(|id| TestLogEntry {
            id,
            message: format!("batch message {id}"),
            value: id as f64,
        })
        .collect();

    let (date, first_line) = writer.append_batch(&entries)?;
    assert_eq!(first_line, 0);
    for (offset, expected) in entries.iter().enumerate() {
        let parsed: TestLogEntry = writer.read_line_json(&date, first_line + offset)?;
        assert_eq!(&parsed, expected);
    }

    // 批后单条 append：行号从批尾继续（说明整批只占 3 行、行号算法自洽）
    let extra = TestLogEntry {
        id: 4,
        message: "after batch".to_string(),
        value: 4.0,
    };
    let (date_extra, line_extra) = writer.append(&extra)?;
    assert_eq!(date_extra, date);
    assert_eq!(line_extra, 3);
    assert_eq!(writer.read_line_json::<TestLogEntry>(&date, 3)?, extra);

    // 空批不写任何行，但返回的起始行号仍是当前行尾
    let (_, empty_first_line) = writer.append_batch::<TestLogEntry>(&[])?;
    assert_eq!(empty_first_line, 4);
    assert!(writer.read_line(&date, 4).is_err());

    Ok(())
}

/// `append_no_position` 不数行、不返回坐标，但内容必须照常落盘。
#[test]
fn test_daily_jsonl_append_no_position_skips_line_number() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    let writer = DailyJsonlWriter::new(temp_dir.path());

    let first = TestLogEntry {
        id: 1,
        message: "no position".to_string(),
        value: 1.0,
    };
    let second = TestLogEntry {
        id: 2,
        message: "no position again".to_string(),
        value: 2.0,
    };

    let date_a = writer.append_no_position(&first)?;
    let date_b = writer.append_no_position(&second)?;
    assert_eq!(date_a, date_b);

    // 行号不再由写入方给出，但按行号读取仍然可用（说明两行都完整落盘）
    let parsed_first: TestLogEntry = writer.read_line_json(&date_a, 0)?;
    let parsed_second: TestLogEntry = writer.read_line_json(&date_a, 1)?;
    assert_eq!(parsed_first, first);
    assert_eq!(parsed_second, second);

    // 与需要行号的 `append` 混用：行号从既有行数继续，不会被覆盖
    let third = TestLogEntry {
        id: 3,
        message: "positioned".to_string(),
        value: 3.0,
    };
    let (_date, line) = writer.append(&third)?;
    assert_eq!(line, 2);

    Ok(())
}
