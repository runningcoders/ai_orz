//! 逗号分隔 query 参数解析单测

use super::split_csv_ids;

#[test]
fn splits_trims_and_dedups_keeping_order() {
    assert_eq!(
        split_csv_ids(Some(" a, b ,,a,c ")),
        Some(vec!["a".to_string(), "b".to_string(), "c".to_string()])
    );
}

#[test]
fn blank_input_means_absent() {
    // 空串 / 全空白等价于「未指定该过滤维度」，不能变成空集合去卡死查询
    assert_eq!(split_csv_ids(None), None);
    assert_eq!(split_csv_ids(Some("")), None);
    assert_eq!(split_csv_ids(Some(" , ,")), None);
}
