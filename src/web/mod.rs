mod api;
mod static_files;

use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Router;

use crate::session::validate_token;
use crate::types::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .merge(static_files::routes())
        .merge(api::routes(state))
}

/// Extract bearer token from Authorization header
pub fn extract_bearer_token(request: &Request) -> Option<String> {
    let auth_header = request.headers().get("authorization")?.to_str().ok()?;
    let token = auth_header.strip_prefix("Bearer ")?;
    Some(token.to_string())
}

/// Middleware that validates JWT bearer token for API endpoints
pub async fn require_session(
    axum::extract::State(state): axum::extract::State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, Response> {
    let token = extract_bearer_token(&request).ok_or_else(|| unauthorized_response())?;

    validate_token(&token, state.jwt_secret.as_bytes())
        .map_err(|_| unauthorized_response())?;

    Ok(next.run(request).await)
}

fn unauthorized_response() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        axum::response::Json(serde_json::json!({
            "error": "Unauthorized"
        })),
    )
        .into_response()
}
