
/// class7 practice 路由

/// 重新开始
async fn restart(cookies: Cookies) -> Redirect {
    tracing::info!(
        "restart practice test {}",
        cookies.get("practice_id").unwrap().value()
    );
    cookies.remove(Cookie::new("practice_id", ""));
    Redirect::to("/class7/practice")
}

/// 404页面
async fn fallback() -> Html<String> {
    let html = TEMPLATES.render("404.html", &Context::new());
    match html {
        Ok(t) => Html(t),
        Err(e) => Html(format!("Error: {}", e)),
    }
}

/// 获取所有题目id
async fn get_all_question_ids(pool: &SqlitePool) -> Vec<String> {
    let questions: Vec<String> = sqlx::query!("SELECT id FROM question")
        .fetch_all(pool)
        .await
        .unwrap()
        .into_iter()
        .map(|record| record.id)
        .collect();
    questions
}

/// 提交问题
async fn answers(
    Path(practice_id): Path<String>,
    State(state): State<AppState>,
    Json(answer): Json<Answer>,
) -> impl IntoResponse {
    // 处理答题记录
    let record = PracticeRecord {
        practice_id,
        question_id: answer.question_id,
        created_at: chrono::Local::now().to_string(),
        is_correct: answer.is_correct as i64,
    };
    sqlx::query!(
        "INSERT INTO practice_record (practice_id, question_id, is_correct, created_at) VALUES ($1, $2, $3, $4)",
        record.practice_id,
        record.question_id,
        record.is_correct,
        record.created_at
    )
    .execute(&state.pool)
    .await
    .unwrap();

    "answer saved".into_response()
}

/// 首页
async fn index(cookies: Cookies) -> impl IntoResponse {
    let practice_id = cookies.get("practice_id").map_or_else(
        || {
            // 如果没有cookie则生成新的practice_id
            let id = Uuid::new_v4().to_string();
            tracing::info!("generate a new practice_id:{}", &id);
            id
        },
        |cookie| cookie.value().to_string(),
    );
    let cookie = format!(
        "practice_id={}; Path=/class7; HttpOnly; Max-Age=10800",
        practice_id
    );
    let headers = AppendHeaders([(header::SET_COOKIE, cookie)]);
    let mut context = Context::new();
    context.insert(
        "get_practice_url",
        &format!("/class7/practice/{}", practice_id),
    );
    let html = TEMPLATES.render("class7/index.html", &context);
    match html {
        Ok(t) => (headers, Html(t)),
        Err(e) => (headers, Html(format!("错误: {}", e))),
    }
}

/// 获取练习
async fn get_practice(
    Path(practice_id): Path<String>,
    State(state): State<AppState>,
) -> Html<String> {
    // 查询 exam_record , 这个sql返回的是一个集合
    let exam_question_ids: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT question_id FROM practice_record WHERE practice_id = ?",
    )
    .bind(&practice_id)
    .fetch_all(&state.pool)
    .await
    .unwrap();
    // 从state.questions和exam_question_ids取差集
    let question_ids = state
        .questions
        .clone()
        .into_iter()
        .filter(|id| !exam_question_ids.contains(id))
        .collect::<Vec<String>>();
    // 如果question_ids为空，表示用户已答完所有题目
    if question_ids.is_empty() {
        tracing::info!(
            "user has finished all questions. practice_id:{}",
            practice_id
        );
        // 跳转到completed页面
        let mut context = Context::new();
        context.insert("practice_id", &practice_id);
        context.insert("total_questions", &state.questions.len());
        context.insert("correct_answers", &exam_question_ids.len());
        context.insert(
            "accuracy",
            &format!(
                "{:.2}%",
                (exam_question_ids.len() as f64 / state.questions.len() as f64) * 100.0
            ),
        );
        let html = TEMPLATES.render("class7/completed.html", &context);
        match html {
            Ok(t) => return Html(t),
            Err(e) => {
                tracing::error!("Error: {:?}", e);
                return Html(format!("错误: {}", e));
            }
        }
    }
    // 从question_ids随机取一个
    let question_id = question_ids.choose(&mut rand::thread_rng()).unwrap();
    // question_id 查询数据库获取题目内容
    let question = sqlx::query_as::<_, Question>("SELECT * FROM question WHERE id = $1")
        .bind(question_id)
        .fetch_one(&state.pool)
        .await
        .unwrap();
    let question_images = vec![question.images];
    let options: Vec<QuestionOption> = serde_json::from_str(&question.options).unwrap();
    let mut context = Context::new();
    context.insert("practice_id", &practice_id);
    context.insert("question_id", &question_id);
    context.insert("question_images", &question_images);
    context.insert("question_content", &question.content);
    context.insert("current_question_number", &question_ids.len());
    context.insert("total_questions", &state.questions.len());
    context.insert("options", &options);
    let html = TEMPLATES.render("class7/practice.html", &context);
    match html {
        Ok(t) => Html(t),
        Err(e) => {
            tracing::error!("Error: {:?}", e);
            Html(format!("错误: {}", e))
        }
    }
}
