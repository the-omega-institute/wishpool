//! `/api/v1` resource routes.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{FromRequest, FromRequestParts, State},
    response::IntoResponse,
    routing::{get, put},
};
use serde::{Deserialize, Serialize};
use wishpool_core::{
    app::App,
    ids::PersonId,
    model::{Role, Stage},
    policy::Policy,
};

use crate::{auth::AuthenticatedCaller, problems::Problem};

mod network;
mod papers;
mod review;
mod solving;

pub(crate) type AppState = State<Arc<App>>;
pub(crate) type ApiResult<T> = Result<T, Problem>;

#[derive(FromRequest)]
#[from_request(via(axum::Json), rejection(Problem))]
pub(crate) struct Body<T>(pub(crate) T);

#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Path), rejection(Problem))]
pub(crate) struct Path<T>(pub(crate) T);

#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Query), rejection(Problem))]
pub(crate) struct Query<T>(pub(crate) T);

pub(crate) fn routes() -> Router<Arc<App>> {
    Router::new()
        .merge(solving::routes())
        .merge(papers::routes())
        .merge(review::routes())
        .merge(network::routes())
        .route("/policy", get(policy))
        .route("/me", get(me))
        .route("/people", get(list_people))
        .route("/people/{id}/roles", put(set_roles))
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PageQuery {
    pub(crate) limit: Option<u32>,
    pub(crate) before: Option<String>,
}

#[derive(Serialize)]
struct StageDescription {
    stage: Stage,
    code: &'static str,
    title: &'static str,
    description: &'static str,
}

#[derive(Serialize)]
struct PolicyDocument<'a> {
    policy: &'a Policy,
    stages: Vec<StageDescription>,
}

const STAGES: [(Stage, &str, &str); 4] = [
    (
        Stage::Hygiene,
        "Source",
        "The LaTeX source is read, the PDF compiles, and AI use is disclosed.",
    ),
    (
        Stage::Claims,
        "Statements",
        "The author confirms the theorems, lemmas and conjectures read from the source and marks the main results.",
    ),
    (
        Stage::Literature,
        "Literature",
        "No main result is already stated in, or directly implied by, prior work.",
    ),
    (
        Stage::Escape,
        "Escape analysis",
        "Each proved statement is judged bind-only or content. A content statement names escape witnesses: new propositions on its proof path that prior results do not give by instantiation, projection or normalisation.",
    ),
];

async fn policy(State(app): AppState) -> impl IntoResponse {
    let stages = STAGES
        .iter()
        .map(|(stage, title, description)| StageDescription {
            stage: *stage,
            code: stage.code(),
            title,
            description,
        })
        .collect();
    Json(
        serde_json::to_value(PolicyDocument {
            policy: app.policy(),
            stages,
        })
        .unwrap_or_default(),
    )
}

async fn me(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(app.me(&caller).await?))
}

async fn list_people(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Query(q): Query<PageQuery>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(app.list_people(&caller, q.limit, q.before).await?))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RolesBody {
    roles: Vec<Role>,
}

async fn set_roles(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path(id): Path<String>,
    Body(body): Body<RolesBody>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        app.set_roles(&caller, &PersonId(id), &body.roles).await?,
    ))
}

#[cfg(test)]
mod tests;
