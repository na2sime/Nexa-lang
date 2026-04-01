use axum::{
    extract::State,
    http::header,
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub html: String,
    pub js: String,
    pub port: u16,
}

pub fn build_router(state: AppState) -> Router {
    let shared = Arc::new(state);
    Router::new()
        .route("/", get(serve_html))
        .route("/app.js", get(serve_js))
        .with_state(shared)
}

async fn serve_html(State(state): State<Arc<AppState>>) -> Html<String> {
    Html(state.html.clone())
}

async fn serve_js(State(state): State<Arc<AppState>>) -> Response {
    (
        [(header::CONTENT_TYPE, "application/javascript; charset=utf-8")],
        state.js.clone(),
    )
        .into_response()
}
