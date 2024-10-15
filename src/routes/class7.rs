
/// class7 practice 路由
fn practice_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(index))
        .route("/:practice_id", get(get_practice))
        .route("/:practice_id/answers", post(answers))
        .route("/:practice_id/restart", get(restart))
}