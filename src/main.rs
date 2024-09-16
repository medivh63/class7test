use axum::{
    extract::{Path, State},
    http::header,
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
        let mut tera = match Tera::new("templates/pages/*") {
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

    let class7_tests_router = Router::new()
        .route("/practice/:practice_id", get(practice))
        .route("/practice-answers", post(practice_answers));

    // build our application with a route
    let app = Router::new()
        .route("/", get(index))
        .route("/restart", get(restart))
        .nest("/v1/driving/class7", class7_tests_router)
        .fallback(fallback)
        .layer(CookieManagerLayer::new()) // 添加此行以启用 Cookie 管理
        .with_state(state);

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::info!("Server running on: {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap()
}

#[derive(sqlx::FromRow, Debug, Deserialize, Serialize)]
struct Question {
    id: Option<String>,
    content: Option<String>,
    images: Option<String>,
    options: Option<String>,
}

#[derive(sqlx::FromRow, Debug, Deserialize, Serialize)]
struct PracticeRecord {
    practice_id: Option<String>,
    question_id: Option<String>,
    is_correct: Option<i64>,
    created_at: Option<String>,
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

/// 重新开始
async fn restart(cookies: Cookies) -> Redirect {
    cookies.remove(Cookie::new("practice_id", ""));
    Redirect::to("/")
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
        .map(|record| record.id.unwrap())
        .collect();
    questions
}

/// 保存答题记录
async fn practice_answers(
    State(state): State<AppState>,
    Json(answer): Json<Answer>,
) -> impl IntoResponse {
    // 处理答题记录
    let record = PracticeRecord {
        practice_id: Some(answer.practice_id),
        question_id: Some(answer.question_id),
        created_at: Some(chrono::Utc::now().to_string()),
        is_correct: Some(answer.is_correct as i64),
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
        "practice_id={}; Path=/; HttpOnly; Max-Age=10800",
        practice_id
    );
    let headers = AppendHeaders([(header::SET_COOKIE, cookie)]);
    let mut context = Context::new();
    context.insert(
        "start_practice_url",
        &format!("/v1/driving/class7/practice/{}", practice_id),
    );
    let html = TEMPLATES.render("index.html", &context);
    match html {
        Ok(t) => (headers, Html(t)),
        Err(e) => (headers, Html(format!("错误: {}", e))),
    }
}

/// 开始练习
async fn practice(Path(practice_id): Path<String>, State(state): State<AppState>) -> Html<String> {
    // 查询 exam_record , 这个sql返回的是一个集合
    let practice_question_ids: Vec<String> = sqlx::query_scalar(
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
        .filter(|id| !practice_question_ids.contains(id))
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
        context.insert("correct_answers", &practice_question_ids.len());
        context.insert(
            "accuracy",
            &format!(
                "{:.2}%",
                (practice_question_ids.len() as f64 / state.questions.len() as f64) * 100.0
            ),
        );
        let html = TEMPLATES.render("completed.html", &context);
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
    let options: Vec<QuestionOption> = serde_json::from_str(&question.options.unwrap()).unwrap();
    let mut context = Context::new();
    context.insert("practice_id", &practice_id);
    context.insert("question_id", &question_id);
    context.insert("question_images", &question_images);
    context.insert("question_content", &question.content);
    context.insert("current_question_number", &question_ids.len());
    context.insert("total_questions", &state.questions.len());
    context.insert("options", &options);
    let html = TEMPLATES.render("practice.html", &context);
    match html {
        Ok(t) => Html(t),
        Err(e) => {
            tracing::error!("Error: {:?}", e);
            return Html(format!("错误: {}", e));
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
