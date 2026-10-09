//! The editors' routes: stage reports, escape judgements, decisions,
//! formalization and conjecture follow-up.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{post, put},
};
use serde::Deserialize;
use wishpool_core::{
    app::{App, FiledBy},
    ids::{ClaimId, SubmissionId},
    model::{
        ConjectureState, FormalArtifact, NewEndorsement, NewLetter, ProofShape, ReportDraft, Stage,
    },
};

use super::{ApiResult, AppState, Body, Path};
use crate::auth::AuthenticatedCaller;

pub(super) fn routes() -> Router<Arc<App>> {
    Router::new()
        .route("/submissions/{id}/referee", axum::routing::get(referee))
        .route("/submissions/{id}/referee/restart", post(restart_referee))
        .route("/submissions/{id}/referee/letters", post(send_feedback))
        .route(
            "/submissions/{id}/stages/{stage}/reports",
            post(file_report),
        )
        .route(
            "/submissions/{id}/claims/{claim}/judgements",
            post(judge_claim),
        )
        .route("/submissions/{id}/escape/adopt", post(adopt_judgements))
        .route(
            "/submissions/{id}/endorsements",
            axum::routing::get(list_endorsements).post(endorse),
        )
        .route(
            "/submissions/{id}/decision",
            axum::routing::get(preview_decision).post(decide),
        )
        .route(
            "/submissions/{id}/formalization/repository",
            put(set_repository),
        )
        .route("/submissions/{id}/formalization/items", post(propose))
        .route(
            "/submissions/{id}/formalization/items/{claim}/response",
            post(respond),
        )
        .route(
            "/submissions/{id}/formalization/items/{claim}/start",
            post(start),
        )
        .route(
            "/submissions/{id}/formalization/items/{claim}/verification",
            post(verify),
        )
        .route(
            "/submissions/{id}/conjectures/{claim}",
            put(update_conjecture),
        )
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileReport {
    report: ReportDraft,
    #[serde(default)]
    filed_by: Option<FiledBy>,
}

async fn file_report(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path((id, stage)): Path<(String, Stage)>,
    Body(body): Body<FileReport>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        app.file_report(
            &caller,
            &SubmissionId(id),
            stage,
            body.report,
            body.filed_by,
        )
        .await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct JudgementBody {
    shape: ProofShape,
    #[serde(default)]
    witnesses: Vec<String>,
    rationale: String,
}

async fn judge_claim(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path((id, claim)): Path<(String, String)>,
    Body(body): Body<JudgementBody>,
) -> ApiResult<impl IntoResponse> {
    let judgement = app
        .judge_claim(
            &caller,
            &SubmissionId(id),
            &ClaimId(claim),
            body.shape,
            body.witnesses,
            body.rationale,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(judgement)))
}

async fn adopt_judgements(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        app.adopt_judgements(&caller, &SubmissionId(id)).await?,
    ))
}

async fn list_endorsements(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(app.endorsements(&caller, &SubmissionId(id)).await?))
}

async fn endorse(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path(id): Path<String>,
    Body(new): Body<NewEndorsement>,
) -> ApiResult<impl IntoResponse> {
    Ok((
        StatusCode::CREATED,
        Json(app.endorse(&caller, &SubmissionId(id), new).await?),
    ))
}

async fn preview_decision(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        app.preview_decision(&caller, &SubmissionId(id)).await?,
    ))
}

async fn decide(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(app.decide(&caller, &SubmissionId(id)).await?))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RepositoryBody {
    repository: String,
}

async fn set_repository(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path(id): Path<String>,
    Body(body): Body<RepositoryBody>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        app.set_formalization_repository(&caller, &SubmissionId(id), body.repository)
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposeBody {
    claim: String,
    reason: String,
}

async fn propose(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path(id): Path<String>,
    Body(body): Body<ProposeBody>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        app.propose_formalization(
            &caller,
            &SubmissionId(id),
            &ClaimId(body.claim),
            body.reason,
        )
        .await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResponseBody {
    approve: bool,
    #[serde(default)]
    reason: String,
}

async fn respond(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path((id, claim)): Path<(String, String)>,
    Body(body): Body<ResponseBody>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        app.respond_to_formalization(
            &caller,
            &SubmissionId(id),
            &ClaimId(claim),
            body.approve,
            body.reason,
        )
        .await?,
    ))
}

async fn start(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path((id, claim)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        app.start_formalization(&caller, &SubmissionId(id), &ClaimId(claim))
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VerifyBody {
    artifact: FormalArtifact,
    #[serde(default)]
    axioms: Vec<String>,
    #[serde(default)]
    contribution: Option<String>,
}

async fn verify(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path((id, claim)): Path<(String, String)>,
    Body(body): Body<VerifyBody>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        app.verify_formalization(
            &caller,
            &SubmissionId(id),
            &ClaimId(claim),
            body.artifact,
            body.axioms,
            body.contribution,
        )
        .await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConjectureBody {
    state: ConjectureState,
}

async fn update_conjecture(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path((id, claim)): Path<(String, String)>,
    Body(body): Body<ConjectureBody>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        app.update_conjecture(&caller, &SubmissionId(id), &ClaimId(claim), body.state)
            .await?,
    ))
}

async fn referee(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(app.referee(&caller, &SubmissionId(id)).await?))
}

async fn restart_referee(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(app.restart_referee(&caller, &SubmissionId(id)).await?))
}

async fn send_feedback(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path(id): Path<String>,
    Body(new): Body<NewLetter>,
) -> ApiResult<impl IntoResponse> {
    Ok((
        StatusCode::CREATED,
        Json(app.send_feedback(&caller, &SubmissionId(id), new).await?),
    ))
}
