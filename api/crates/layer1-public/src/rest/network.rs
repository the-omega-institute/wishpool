//! Routes for volunteer contributors: tasks, leases, contributions, credit
//! and token donation.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use serde::Deserialize;
use wishpool_core::{
    app::App,
    ids::{PersonId, SubmissionId},
    model::{ContributionMode, GrantStatus, NewContribution, TaskKind},
    ports::{ContributionFilter, TaskFilter},
};

use super::{ApiResult, AppState, Body, Path, Query};
use crate::auth::AuthenticatedCaller;

pub(super) fn routes() -> Router<Arc<App>> {
    Router::new()
        .route("/submissions/{id}/tasks", post(generate_tasks))
        .route("/tasks", get(list_tasks))
        .route("/tasks/{id}", get(task_context))
        .route("/tasks/{id}/lease", post(lease_task).delete(release_task))
        .route("/tasks/{id}/contributions", post(submit_contribution))
        .route("/contributions", get(list_contributions))
        .route("/contributions/{id}/review", post(review_contribution))
        .route("/contributors", get(credits))
        .route("/donation", get(donation).patch(update_donation))
}

async fn generate_tasks(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        app.generate_review_tasks(&caller, &SubmissionId(id))
            .await?,
    ))
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskQuery {
    kind: Option<TaskKind>,
    status: Option<String>,
    submission: Option<String>,
    holder: Option<String>,
    limit: Option<u32>,
    before: Option<String>,
}

/// Tasks are listed to signed-in people only: their titles name statements
/// of papers under review.
async fn list_tasks(
    State(app): AppState,
    AuthenticatedCaller(_caller): AuthenticatedCaller,
    Query(q): Query<TaskQuery>,
) -> ApiResult<impl IntoResponse> {
    let filter = TaskFilter {
        kind: q.kind,
        status: q.status,
        submission: q.submission.map(SubmissionId),
        holder: q.holder.map(PersonId),
    };
    Ok(Json(app.list_tasks(filter, q.limit, q.before).await?))
}

async fn task_context(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(app.task_context(&caller, &id).await?))
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct LeaseBody {
    #[serde(default)]
    mode: Option<ContributionMode>,
}

async fn lease_task(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> ApiResult<impl IntoResponse> {
    // The body is optional: `{"mode": "hosted"}` or nothing.
    let body: LeaseBody = if body.is_empty() {
        LeaseBody::default()
    } else {
        serde_json::from_slice(&body).map_err(|e| {
            crate::problems::Problem(wishpool_core::CoreError::Invalid(e.to_string()))
        })?
    };
    let mode = body.mode.unwrap_or(ContributionMode::OwnAgent);
    Ok(Json(app.lease_task(&caller, &id, mode).await?))
}

async fn release_task(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(app.release_task(&caller, &id).await?))
}

async fn submit_contribution(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path(id): Path<String>,
    Body(body): Body<NewContribution>,
) -> ApiResult<impl IntoResponse> {
    let contribution = app.submit_contribution(&caller, &id, body).await?;
    let nature = contribution.kind.nature();
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "contribution": contribution, "nature": nature })),
    ))
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContributionQuery {
    task: Option<String>,
    contributor: Option<String>,
    status: Option<String>,
    kind: Option<TaskKind>,
    limit: Option<u32>,
    before: Option<String>,
}

async fn list_contributions(
    State(app): AppState,
    AuthenticatedCaller(_caller): AuthenticatedCaller,
    Query(q): Query<ContributionQuery>,
) -> ApiResult<impl IntoResponse> {
    let filter = ContributionFilter {
        task: q.task,
        contributor: q.contributor.map(PersonId),
        status: q.status,
        kind: q.kind,
    };
    Ok(Json(
        app.list_contributions(filter, q.limit, q.before).await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewBody {
    accept: bool,
    note: String,
}

async fn review_contribution(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path(id): Path<String>,
    Body(body): Body<ReviewBody>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        app.review_contribution(&caller, &id, body.accept, body.note)
            .await?,
    ))
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreditQuery {
    contributor: Option<String>,
}

async fn credits(
    State(app): AppState,
    Query(q): Query<CreditQuery>,
) -> ApiResult<impl IntoResponse> {
    let contributor = q.contributor.map(PersonId);
    Ok(Json(app.credits(contributor.as_ref()).await?))
}

async fn donation(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(app.donation(&caller).await?))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DonationPatch {
    #[serde(default)]
    monthly_cap: Option<u64>,
    #[serde(default)]
    status: Option<GrantStatus>,
}

async fn update_donation(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Body(body): Body<DonationPatch>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        app.update_donation(&caller, body.monthly_cap, body.status)
            .await?,
    ))
}
