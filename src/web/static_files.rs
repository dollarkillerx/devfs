use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;

use crate::types::AppState;

const INDEX_HTML: &str = include_str!("../../web_ui/index.html");
const STYLE_CSS: &str = include_str!("../../web_ui/style.css");
const APP_JS: &str = include_str!("../../web_ui/app.js");

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/_web/", get(serve_index))
        .route("/_web/index.html", get(serve_index))
        .route("/_web/assets/style.css", get(serve_css))
        .route("/_web/assets/app.js", get(serve_js))
}

async fn serve_index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn serve_css() -> Response {
    (
        [("content-type", "text/css; charset=utf-8")],
        STYLE_CSS,
    )
        .into_response()
}

async fn serve_js() -> Response {
    (
        [("content-type", "application/javascript; charset=utf-8")],
        APP_JS,
    )
        .into_response()
}
