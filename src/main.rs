use axum::{
    extract::{Path, State},
    http::{header, HeaderMap},
    response::{AppendHeaders, Html, IntoResponse},
    routing::get,
    Error, Router,
};
use captcha::{filters::Noise, Captcha};
use lazy_static::lazy_static;
use log::{info, warn};
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
    pub pool: SqlitePool,
}
#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    // 初始化日志,设置日志级别
    set_var("RUST_LOG", "debug");
    pretty_env_logger::init_timed();

    // 数据库连接池
    let db = init_db().await.unwrap();
    let state = AppState { pool: db };

    // build our application with a route
    let app = Router::new()
        .route("/", get(index))
        .route("/exam/:exam_id", get(question))
        //.route("/captcha", get(generate_captcha)) // 新增的路由
        .fallback(fallback)
        .layer(CookieManagerLayer::new()) // 添加此行以启用 Cookie 管理
        .with_state(state);

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    info!("Server running on: {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap()
}

/// 定义Question结构体
struct Question {
    id: String,
    content: String,
    question_images: Vec<String>,
    options: Vec<String>,
    correct_options: Vec<String>,
}

// fallback handler
async fn fallback() -> Html<String> {
    let html = TEMPLATES.render("404.html", &Context::new());
    match html {
        Ok(t) => Html(t),
        Err(e) => Html(format!("Error: {}", e)),
    }
}

async fn question(Path(exam_id): Path<String>, State(state): State<AppState>) -> Html<String> {
    // 这里可以添加从数据库获取题目内容的逻辑

    // 从数据库获取question实例
    // let question = sqlx::query_as!(
    //     Question,
    //     "SELECT * FROM questions WHERE id = ?",
    //     question_id
    // )
    // .fetch_one(&state.pool)
    // .await
    // .unwrap();
    let question_content_en = "Two cars arrive at a four-way stop at right angles to each
other at the same time. After both cars have made a complete stop, which car should go first?";
    let question_content_zh ="
在一个4个角落均有停车标志的十字路口，两辆车同时驶到跟对方成正角的位置，当双方都停下来的时候，谁先行？";
    let question_images =
        vec!["https://images.ctfassets.net/nnc41duedoho/3SXOiKUq7trvZ6O6i724C4/e0ce25b4c0e52ec67ec22819b2e5df13/Four-way-stop-vehicles-arrived-at-same-time.jpeg?w=640&q=100"];
    let options = vec![
        "30 km/h 时速30公里",
        "40 km/h 时速40公里",
        "50 km/h 时速50公里",
        "60 km/h 时速60公里",
    ];

    // 例如: let question_content = get_question_from_db(&question_id).await;
    // context.insert("question_content", &question_content);
    let mut context = Context::new();
    context.insert("question_id", &exam_id);
    context.insert("question_images", &question_images);
    context.insert("question_content_en", &question_content_en);
    context.insert("question_content_zh", &question_content_zh);
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
    let cookie = format!("exam_id={}; Path=/; HttpOnly", exam_id);
    let mut context = Context::new();
    context.insert("start_exam_url", &format!("/exam/{}", exam_id));
    let html = TEMPLATES.render("index.html", &context);
    match html {
        Ok(t) => {
            let headers = AppendHeaders([(header::SET_COOKIE, cookie)]);
            (headers, Html(t))
        }
        Err(e) => {
            let headers = AppendHeaders([(header::SET_COOKIE, cookie)]);
            (headers, Html(format!("错误: {}", e)))
        }
    }
}

/// 生成图片验证码
async fn generate_captcha() -> impl IntoResponse {
    let png = Captcha::new()
        .add_chars(4)
        .apply_filter(Noise::new(0.3))
        .view(220, 120)
        .as_png()
        .expect("生成验证码失败");
    ([(header::CONTENT_TYPE, "image/png")], png)
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
