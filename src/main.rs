use axum::{
    extract::{Path, State},
    http::header,
    response::{AppendHeaders, Html, IntoResponse},
    routing::{get, post},
    Error, Json, Router,
};
use lazy_static::lazy_static;
use log::{info, warn};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use sqlx::{pool::PoolOptions, SqlitePool};
use std::env::set_var;
use tera::{Context, Tera};
use tower_cookies::{CookieManagerLayer, Cookies};
use uuid::Uuid;

lazy_static! {
    pub static ref TEMPLATES: Tera = {
        let mut tera = match Tera::new("templates/pages/*") {
            Ok(t) => t,
            Err(e) => {
                println!("Parsing error(s): {}", e);
                ::std::process::exit(1);
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
    // 初始化日志,设置日志级别
    set_var("RUST_LOG", "debug");
    pretty_env_logger::init_timed();

    // 数据库连接池
    let db = init_db().await.unwrap();

    let questions = get_all_question_ids(&db).await;
    info!("there have {} questions ", questions.len());
    let state = AppState {
        pool: db,
        questions,
    };

    // build our application with a route
    let app = Router::new()
        .route("/", get(index))
        .route("/driving/exam/:exam_id", get(exam_question))
        .route("/driving/exam/answer", post(exam_answer))
        .fallback(fallback)
        .layer(CookieManagerLayer::new()) // 添加此行以启用 Cookie 管理
        .with_state(state);

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    info!("Server running on: {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap()
}

/// 定义Question结构体
#[derive(sqlx::FromRow, Debug, Deserialize, Serialize)]
struct Question {
    id: Option<String>,
    content: Option<String>,
    images: Option<String>,
    options: Option<String>,
}

#[derive(sqlx::FromRow, Debug, Deserialize, Serialize)]
struct ExamRecord {
    exam_id: Option<String>,
    question_id: Option<String>,
    is_correct: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize)]
struct QuestionOption {
    id: String,
    content: String,
    is_correct: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct Answer {
    exam_id: String,
    question_id: String,
    is_correct: bool,
}

// fallback handler
async fn fallback() -> Html<String> {
    let html = TEMPLATES.render("404.html", &Context::new());
    match html {
        Ok(t) => Html(t),
        Err(e) => Html(format!("Error: {}", e)),
    }
}

async fn get_all_question_ids(pool: &SqlitePool) -> Vec<String> {
    let questions: Vec<String> = sqlx::query!("SELECT id FROM question")
        .fetch_all(pool)
        .await
        .unwrap()
        .into_iter()
        .map(|record| record.id.unwrap())
        .collect();
    questions
}

async fn exam_answer(
    State(state): State<AppState>,
    Json(answer): Json<Answer>,
) -> impl IntoResponse {
    // 处理答题记录
    let record = ExamRecord {
        exam_id: Some(answer.exam_id),
        question_id: Some(answer.question_id),
        is_correct: Some(answer.is_correct as i64),
    };
    sqlx::query!(
        "INSERT INTO exam_record (exam_id, question_id, is_correct) VALUES ($1, $2, $3)",
        record.exam_id,
        record.question_id,
        record.is_correct
    )
    .execute(&state.pool)
    .await
    .unwrap();

    "答题记录已保存".into_response()
}

async fn exam_question(Path(exam_id): Path<String>, State(state): State<AppState>) -> Html<String> {
    // 从state.questions随机取出一个id
    let question_id = state.questions.choose(&mut rand::thread_rng()).unwrap();
    // 这里可以添加从数据库获取题目内容的逻辑
    // question_id 查询数据库获取题目内容
    let question = sqlx::query_as::<_, Question>("SELECT * FROM question WHERE id = $1")
        .bind(question_id)
        .fetch_one(&state.pool)
        .await
        .unwrap();
    let question_images = vec![question.images];
    // question.options 转化为 options
    let options: Vec<QuestionOption> = serde_json::from_str(&question.options.unwrap()).unwrap();
    let mut context = Context::new();
    context.insert("exam_id", &exam_id);
    context.insert("question_id", &question_id);
    context.insert("question_images", &question_images);
    context.insert("question_content", &question.content);
    context.insert("options", &options);

    let html = TEMPLATES.render("question.html", &context);
    match html {
        Ok(t) => Html(t),
        Err(e) => {
            warn!("Error: {:?}", e);
            Html(format!("错误: {}", e))
        }
    }
}

async fn index(cookies: Cookies) -> impl IntoResponse {
    let exam_id = cookies.get("exam_id").map_or_else(
        || {
            // 如果没有cookie则生成新的exam_id
            let id = Uuid::new_v4().to_string();
            info!("generate a new exam_id:{}", &id);
            id
        },
        |cookie| cookie.value().to_string(),
    );
    let cookie = format!("exam_id={}; Path=/; HttpOnly; Max-Age=10800", exam_id);
    let headers = AppendHeaders([(header::SET_COOKIE, cookie)]);
    let mut context = Context::new();
    context.insert("start_exam_url", &format!("/driving/exam/{}", exam_id));
    let html = TEMPLATES.render("index.html", &context);
    match html {
        Ok(t) => (headers, Html(t)),
        Err(e) => (headers, Html(format!("错误: {}", e))),
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
