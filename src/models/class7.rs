
/// 问题
#[derive(sqlx::FromRow, Debug, Deserialize, Serialize)]
struct Question {
    id: String,
    content: Option<String>,
    images: Option<String>,
    options: String,
}

/// 练习记录
#[derive(sqlx::FromRow, Debug, Deserialize, Serialize)]
struct PracticeRecord {
    practice_id: String,
    question_id: String,
    is_correct: i64,
    created_at: String,
}

/// 问题选项
#[derive(Debug, Deserialize, Serialize)]
struct QuestionOption {
    content: String,
    is_correct: bool,
}

/// 回答
#[derive(Debug, Deserialize, Serialize)]
struct Answer {
    practice_id: String,
    question_id: String,
    is_correct: bool,
}
