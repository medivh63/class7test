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
use tower_http::trace::TraceLayer;
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
    // 初始化 tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_file(true)
        .init();

    // 数据库连接池
    let database = database().await.unwrap();

    let questions = get_all_question_ids(&database).await;
    tracing::info!("there have {} questions ", questions.len());
    let state = AppState {
        pool: database,
        questions,
    };

    // build our application with a route
    let app = Router::new()
        .nest("/class7/practice", practice_routes())
        .fallback(fallback)
        .layer(CookieManagerLayer::new()) // 添加此行以启用 Cookie 管理
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::info!("Server running on: {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap()
}

/// 初始化数据库
async fn database() -> Result<SqlitePool, Error> {
    let database_url = dotenv::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = PoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("db connect error");
    Ok(pool)
}
