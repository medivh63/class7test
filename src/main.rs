use axum::{
    extract::{Path, State},
    http::header,
    middleware::map_request,
    response::{AppendHeaders, Html, IntoResponse, Redirect},
    routing::{get, post},
    Error, Json, Router,
};
use lazy_static::lazy_static;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use sqlx::{pool::PoolOptions, SqlitePool};
use tera::{Context, Tera};
use tower_cookies::{Cookie, CookieManagerLayer, Cookies};
use uuid::Uuid;

lazy_static! {
    pub static ref TEMPLATES: Tera = {
        let mut tera = match Tera::new("templates/**/*.html") {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("Parsing error(s): {}", e);
                Tera::default()
            }
        };
        tera.autoescape_on(vec![".html"]);
        tera
    };
}

#[derive(Clone)]
struct AppState {
    pool: SqlitePool,
    questions: Vec<String>,
}
#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    let file_appender = tracing_appender::rolling::daily("logs", "app.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .pretty()
        .with_thread_names(true)
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(non_blocking) // 将日志输出到文件
        .init();

    // 数据库连接池
    let database = init_db().await.unwrap();

    let questions = get_all_question_ids(&database).await;
    tracing::info!("there have {} questions ", questions.len());
    let state = AppState {
        pool: database,
        questions,
    };

    // build our application with a route
    let app = Router::new()
        .route("/class7", get(index))
        .route("/class7/practice/:practice_id", get(get_practice_test))
        .route("/class7/submit_answer", post(submit_answer))
        .route("/class7/restart", get(restart))
        .fallback(fallback)
        .layer(CookieManagerLayer::new()) // 添加此行以启用 Cookie 管理
        .layer(map_request(log_request))
        .with_state(state);

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::info!("Server running on: {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap()
}

#[derive(sqlx::FromRow, Debug, Deserialize, Serialize)]
struct Question {
    id: String,
    content: Option<String>,
    images: Option<String>,
    options: String,
}

#[derive(sqlx::FromRow, Debug, Deserialize, Serialize)]
struct PracticeRecord {
    practice_id: String,
    question_id: String,
    is_correct: i64,
    created_at: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct QuestionOption {
    content: String,
    is_correct: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct Answer {
    practice_id: String,
    question_id: String,
    is_correct: bool,
}

// 添加新的日志中间件函数
async fn log_request<B>(request: axum::http::Request<B>) -> axum::http::Request<B> {
    let path = request.uri().path();
    // 定义需要记录的路径
    let logged_paths = [
        "/class7",
        "/class7/practice/",
        "/class7/submit_answer",
        "/class7/restart",
    ];
    let now = chrono::Local::now();
    // 检查当前路径是否需要记录
    if logged_paths.iter().any(|&p| path.starts_with(p)) {
        tracing::info!("------->request path: {}, time: {}", path, now);
    }
    request
}

/// 重新开始
async fn restart(cookies: Cookies) -> Redirect {
    tracing::info!(
        "restart practice test {}",
        cookies.get("practice_id").unwrap().value()
    );
    cookies.remove(Cookie::new("practice_id", ""));
    Redirect::to("/class7")
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
async fn submit_answer(
    State(state): State<AppState>,
    Json(answer): Json<Answer>,
) -> impl IntoResponse {
    // 处理答题记录
    let record = PracticeRecord {
        practice_id: answer.practice_id,
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
        "start_practice_url",
        &format!("/class7/practice/{}", practice_id),
    );
    let html = TEMPLATES.render("class7/index.html", &context);
    match html {
        Ok(t) => (headers, Html(t)),
        Err(e) => (headers, Html(format!("错误: {}", e))),
    }
}

/// 开始练习
async fn get_practice_test(
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

/// 初始化数据库
async fn init_db() -> Result<SqlitePool, Error> {
    let database_url = dotenv::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = PoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("db connect error");
    Ok(pool)
}
