use super::{ApiResult, AppState, Body, Path, Query};
use crate::auth::AuthenticatedCaller;
use axum::{
    Json, Router,
    extract::State,
    http::{StatusCode, header},
    response::IntoResponse,
    routing::{get, patch, post},
};
use serde::Deserialize;
use std::sync::Arc;
use wishpool_core::{
    app::App,
    ids::{ClaimId, RecordId, SubmissionId},
    model::Caller,
};
pub(super) fn routes() -> Router<Arc<App>> {
    Router::new()
        .route("/me/agents", get(agents).post(create_agent))
        .route("/me/agents/{id}", patch(update_agent).delete(retire_agent))
        .route("/me/notifications", get(notifications))
        .route("/conjectures/{record}/{claim}", get(conjecture))
        .route("/conjectures/{record}/{claim}/target", get(target))
        .route(
            "/conjectures/{record}/{claim}/attempts",
            get(verified).post(attempt),
        )
        .route("/conjectures/{record}/{claim}/attempts/mine", get(mine))
        .route("/attempts/{id}", get(status))
        .route("/leaderboard", get(board))
        .route("/entrants/{id}", get(profile))
        .route(
            "/submissions/{id}/open-problems/check",
            post(check_problems),
        )
}
async fn agents(
    State(app): AppState,
    AuthenticatedCaller(c): AuthenticatedCaller,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(app.agents(&c).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentBody {
    name: String,
}
async fn create_agent(
    State(app): AppState,
    AuthenticatedCaller(c): AuthenticatedCaller,
    Body(b): Body<AgentBody>,
) -> ApiResult<impl IntoResponse> {
    Ok((
        StatusCode::CREATED,
        Json(app.create_agent(&c, b.name).await?),
    ))
}
async fn update_agent(
    State(app): AppState,
    AuthenticatedCaller(c): AuthenticatedCaller,
    Path(id): Path<String>,
    Body(b): Body<AgentBody>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(app.update_agent(&c, &id, Some(b.name), false).await?))
}
async fn retire_agent(
    State(app): AppState,
    AuthenticatedCaller(c): AuthenticatedCaller,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(app.update_agent(&c, &id, None, true).await?))
}
async fn notifications(
    State(app): AppState,
    AuthenticatedCaller(c): AuthenticatedCaller,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(app.solve_notifications(&c).await?))
}
async fn check_problems(
    State(app): AppState,
    AuthenticatedCaller(c): AuthenticatedCaller,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    app.queue_open_problems(&c, &SubmissionId(id)).await?;
    Ok(StatusCode::ACCEPTED)
}
async fn target(
    State(app): AppState,
    Path((r, c)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    let target = app.target(&RecordId(r), &ClaimId(c)).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"Target.lean\"",
            ),
        ],
        target.lean,
    ))
}
async fn conjecture(
    State(app): AppState,
    Path((r, c)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(app.conjecture(&RecordId(r), &ClaimId(c)).await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AttemptBody {
    solution: String,
    #[serde(default)]
    as_agent: Option<String>,
    #[serde(default)]
    note: String,
}
async fn attempt(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path((r, c)): Path<(String, String)>,
    Body(b): Body<AttemptBody>,
) -> ApiResult<impl IntoResponse> {
    Ok((
        StatusCode::ACCEPTED,
        Json(
            app.submit_attempt(
                &caller,
                &RecordId(r),
                &ClaimId(c),
                b.solution,
                b.as_agent,
                b.note,
            )
            .await?,
        ),
    ))
}
async fn mine(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path((r, c)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        app.my_attempts(&caller, &RecordId(r), &ClaimId(c)).await?,
    ))
}
fn caller(c: &Option<AuthenticatedCaller>) -> Option<&Caller> {
    c.as_ref().map(|c| &c.0)
}
async fn status(
    State(app): AppState,
    c: Option<AuthenticatedCaller>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(app.attempt(caller(&c), &id).await?))
}
async fn verified(
    State(app): AppState,
    Path((r, c)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(app.public_attempts(&RecordId(r), &ClaimId(c)).await?))
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct BoardQuery {
    period: Option<String>,
    entrants: Option<String>,
}
async fn board(State(app): AppState, Query(q): Query<BoardQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        app.leaderboard(
            q.period.as_deref().unwrap_or("all"),
            q.entrants.as_deref().unwrap_or("all"),
        )
        .await?,
    ))
}
async fn profile(State(app): AppState, Path(id): Path<String>) -> ApiResult<impl IntoResponse> {
    Ok(Json(app.entrant_profile(&id).await?))
}
