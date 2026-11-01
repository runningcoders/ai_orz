//! cerebellum_protocol_tests 单元测试（拆分自 cerebellum_types.rs）
//!
//! 文件瘦身：原 361 行 → 207 行，测试体 155 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;

/// 官方协议 fixture：请求序列化形态（state=对象，三类型问题并存）
#[test]
fn request_serialization_matches_official_fixture() {
    let mut choices = BTreeMap::new();
    choices.insert("route_cortex".to_string(), "交由大脑深度思考".to_string());
    choices.insert("answer_directly".to_string(), "小脑直接回答".to_string());
    let mut questions = BTreeMap::new();
    questions.insert(
        "q_route".to_string(),
        CerebellumQuestion {
            question_type: QuestionType::Choice,
            instructions: "判断该消息的路由去向".to_string(),
            criteria: Some(QuestionCriteria::Choices(choices)),
        },
    );
    questions.insert(
        "q_urgency".to_string(),
        CerebellumQuestion {
            question_type: QuestionType::Score,
            instructions: "为消息紧急程度打分".to_string(),
            criteria: Some(QuestionCriteria::Levels(vec![
                "无关".to_string(),
                "一般".to_string(),
                "紧急".to_string(),
            ])),
        },
    );
    questions.insert(
        "q_trivial".to_string(),
        CerebellumQuestion {
            question_type: QuestionType::Noul,
            instructions: "该消息是否为寒暄闲聊".to_string(),
            criteria: None,
        },
    );
    let req = CerebellumRequest {
        model: "jev-latest".to_string(),
        state: serde_json::json!({"conversation_id": "c1", "last_user_message": "你好"}),
        questions,
    };
    let v = serde_json::to_value(&req).unwrap();
    assert_eq!(v["model"], "jev-latest");
    assert!(v["state"].is_object());
    let qs = v["questions"].as_object().unwrap();
    assert_eq!(qs.len(), 3);
    let q_route = &qs["q_route"];
    assert_eq!(q_route["type"], "choice");
    assert!(q_route["criteria"].is_object());
    let q_urgency = &qs["q_urgency"];
    assert_eq!(q_urgency["type"], "score");
    assert!(q_urgency["criteria"].is_array());
    assert_eq!(q_urgency["criteria"].as_array().unwrap().len(), 3);
    let q_trivial = &qs["q_trivial"];
    assert_eq!(q_trivial["type"], "noul");
    assert!(
        q_trivial.get("criteria").is_none(),
        "noul 不得序列化 criteria 字段"
    );
}

/// 官方协议 fixture：响应解析（choice/score/noul 三型答案 + usage）
#[test]
fn response_parse_matches_official_fixture() {
    let fixture = r#"{
        "model": "jev-1.13.0",
        "answers": {
            "q_route": {"type": "choice", "choice": "answer_directly", "confidence": 0.87, "probabilities": {"answer_directly": 0.87, "route_cortex": 0.13}},
            "q_urgency": {"type": "score", "score": 2.4, "probabilities": {"无关": 0.1, "一般": 0.8, "紧急": 0.1}, "legend": ["无关", "一般", "紧急"], "confidence": 0.8},
            "q_trivial": {"type": "noul", "noul": 0.92}
        },
        "usage": {"input_tokens": 128, "output_tokens": 16}
    }"#;
    let resp: CerebellumResponse = serde_json::from_str(fixture).unwrap();
    assert_eq!(resp.model, "jev-1.13.0");
    assert_eq!(resp.usage.input_tokens, 128);
    assert_eq!(resp.usage.output_tokens, 16);
    let (choice, conf) = resp.answers["q_route"].as_choice().unwrap();
    assert_eq!(choice, "answer_directly");
    assert!((conf - 0.87).abs() < 1e-6);
    assert!((resp.answers["q_urgency"].as_score().unwrap() - 2.4).abs() < 1e-6);
    assert!((resp.answers["q_trivial"].as_noul().unwrap() - 0.92).abs() < 1e-6);
}

/// 响应缺 usage 字段时容错为零值（协议版本演进容忍）
#[test]
fn response_without_usage_defaults_to_zero() {
    let fixture = r#"{"model": "jev-1.13.0", "answers": {"q": {"type": "noul", "noul": 0.5}}}"#;
    let resp: CerebellumResponse = serde_json::from_str(fixture).unwrap();
    assert_eq!(resp.usage.input_tokens, 0);
    assert_eq!(resp.usage.output_tokens, 0);
}

/// 边界校验：choice ≤255 / score 2~10 / noul 无 criteria
#[test]
fn question_validate_enforces_protocol_boundaries() {
    // 合法用例
    let mut choices = BTreeMap::new();
    choices.insert("a".to_string(), "A".to_string());
    let ok_choice = CerebellumQuestion {
        question_type: QuestionType::Choice,
        instructions: "i".into(),
        criteria: Some(QuestionCriteria::Choices(choices)),
    };
    assert!(ok_choice.validate().is_ok());
    let ok_score = CerebellumQuestion {
        question_type: QuestionType::Score,
        instructions: "i".into(),
        criteria: Some(QuestionCriteria::Levels(vec!["低".into(), "高".into()])),
    };
    assert!(ok_score.validate().is_ok());
    let ok_noul = CerebellumQuestion {
        question_type: QuestionType::Noul,
        instructions: "i".into(),
        criteria: None,
    };
    assert!(ok_noul.validate().is_ok());

    // choice 256 选项越界
    let mut too_many = BTreeMap::new();
    for i in 0..256 {
        too_many.insert(format!("o{i}"), "x".to_string());
    }
    let bad_choice = CerebellumQuestion {
        question_type: QuestionType::Choice,
        instructions: "i".into(),
        criteria: Some(QuestionCriteria::Choices(too_many)),
    };
    assert!(bad_choice.validate().is_err());

    // score 1 级 / 11 级越界
    let bad_score_low = CerebellumQuestion {
        question_type: QuestionType::Score,
        instructions: "i".into(),
        criteria: Some(QuestionCriteria::Levels(vec!["唯一".into()])),
    };
    assert!(bad_score_low.validate().is_err());
    let bad_score_high = CerebellumQuestion {
        question_type: QuestionType::Score,
        instructions: "i".into(),
        criteria: Some(QuestionCriteria::Levels(
            (0..11).map(|i| format!("L{i}")).collect(),
        )),
    };
    assert!(bad_score_high.validate().is_err());

    // noul 携带 criteria 非法
    let bad_noul = CerebellumQuestion {
        question_type: QuestionType::Noul,
        instructions: "i".into(),
        criteria: Some(QuestionCriteria::Levels(vec!["低".into(), "高".into()])),
    };
    assert!(bad_noul.validate().is_err());
}
