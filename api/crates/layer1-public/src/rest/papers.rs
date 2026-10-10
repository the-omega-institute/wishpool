//! The author's routes (upload, confirm, revise, visibility), the files of
//! a paper, and the public pages of accepted papers.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use serde::Deserialize;
use wishpool_core::{
    CoreError,
    app::{App, MAX_UPLOAD_BYTES, PaperFile, SubmissionScope, Upload},
    ids::{RecordId, SubmissionId},
    model::{ClaimConfirmation, NewPaper, Visibility},
};

use super::{ApiResult, AppState, Body, PageQuery, Path, Query};
use crate::{auth::AuthenticatedCaller, problems::Problem};

pub(super) fn routes() -> Router<Arc<App>> {
    let upload_limit = DefaultBodyLimit::max(MAX_UPLOAD_BYTES + 1024 * 1024);
    Router::new()
        .route(
            "/submissions",
            get(list_submissions).post(submit).layer(upload_limit),
        )
        .route("/submissions/{id}", get(get_submission))
        .route("/submissions/{id}/claims", post(confirm_claims))
        .route(
            "/submissions/{id}/lean-statement/response",
            post(respond_lean_statement),
        )
        .route(
            "/submissions/{id}/versions",
            post(upload_version).layer(upload_limit),
        )
        .route("/submissions/{id}/withdraw", post(withdraw))
        .route("/submissions/{id}/visibility", put(set_visibility))
        .route("/submissions/{id}/contributors", put(set_contributors))
        .route("/submissions/{id}/analysis", get(analysis))
        .route("/submissions/{id}/files/{kind}", get(file))
        .route("/papers", get(list_papers))
        .route("/conjectures", get(list_conjectures))
        .route("/papers/{id}", get(paper))
}

/// The parts of an upload form: `metadata` (JSON), `note` (text) and
/// `source` (the `.tex`, `.zip` or `.tar.gz`).
struct UploadForm {
    metadata: Option<String>,
    note: String,
    source: Option<Upload>,
}

async fn read_form(mut multipart: Multipart) -> Result<UploadForm, Problem> {
    let bad =
        |e: axum::extract::multipart::MultipartError| Problem(CoreError::Invalid(e.body_text()));
    let mut form = UploadForm {
        metadata: None,
        note: String::new(),
        source: None,
    };
    while let Some(field) = multipart.next_field().await.map_err(bad)? {
        match field.name().unwrap_or_default() {
            "metadata" => form.metadata = Some(field.text().await.map_err(bad)?),
            "note" => form.note = field.text().await.map_err(bad)?,
            "source" => {
                let filename = field.file_name().unwrap_or("source.tex").to_owned();
                let bytes = field.bytes().await.map_err(bad)?.to_vec();
                form.source = Some(Upload { filename, bytes });
            }
            other => {
                return Err(Problem(CoreError::Invalid(format!(
                    "unexpected form field {other:?}"
                ))));
            }
        }
    }
    Ok(form)
}

fn missing_source() -> Problem {
    Problem(CoreError::invalid("attach the LaTeX source as `source`"))
}

async fn submit(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    multipart: Multipart,
) -> ApiResult<impl IntoResponse> {
    let form = read_form(multipart).await?;
    let metadata = form.metadata.ok_or_else(|| {
        Problem(CoreError::invalid(
            "attach the paper metadata as `metadata`",
        ))
    })?;
    let new: NewPaper = serde_json::from_str(&metadata)
        .map_err(|e| Problem(CoreError::Invalid(format!("metadata: {e}"))))?;
    let source = form.source.unwrap_or(Upload {
        filename: String::new(),
        bytes: vec![],
    });
    Ok((
        StatusCode::CREATED,
        Json(app.submit_paper(&caller, new, source).await?),
    ))
}

async fn upload_version(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path(id): Path<String>,
    multipart: Multipart,
) -> ApiResult<impl IntoResponse> {
    let form = read_form(multipart).await?;
    let source = form.source.ok_or_else(missing_source)?;
    Ok(Json(
        app.upload_version(&caller, &SubmissionId(id), source, form.note)
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SubmissionQuery {
    scope: Option<SubmissionScope>,
    limit: Option<u32>,
    before: Option<String>,
}

async fn list_submissions(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Query(q): Query<SubmissionQuery>,
) -> ApiResult<impl IntoResponse> {
    let scope = q.scope.unwrap_or(SubmissionScope::Mine);
    Ok(Json(
        app.list_submissions(&caller, scope, q.limit, q.before)
            .await?,
    ))
}

async fn get_submission(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(app.submission(&caller, &SubmissionId(id)).await?))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfirmBody {
    claims: Vec<ClaimConfirmation>,
}

async fn confirm_claims(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path(id): Path<String>,
    Body(body): Body<ConfirmBody>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        app.confirm_claims(&caller, &SubmissionId(id), body.claims)
            .await?,
    ))
}

async fn withdraw(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(app.withdraw(&caller, &SubmissionId(id)).await?))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VisibilityBody {
    visibility: Visibility,
}

async fn set_visibility(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path(id): Path<String>,
    Body(body): Body<VisibilityBody>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        app.set_analysis_visibility(&caller, &SubmissionId(id), body.visibility)
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContributorsBody {
    open: bool,
}

async fn set_contributors(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path(id): Path<String>,
    Body(body): Body<ContributorsBody>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        app.set_open_to_contributors(&caller, &SubmissionId(id), body.open)
            .await?,
    ))
}

async fn analysis(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(app.analysis(&caller, &SubmissionId(id)).await?))
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileQuery {
    version: Option<u32>,
}

/// `kind` is `pdf` (inline; public once accepted) or `source` (download;
/// authors and staff).
async fn file(
    State(app): AppState,
    caller: Option<AuthenticatedCaller>,
    Path((id, kind)): Path<(String, String)>,
    Query(q): Query<FileQuery>,
) -> ApiResult<Response> {
    let pdf = match kind.as_str() {
        "pdf" => true,
        "source" => false,
        _ => return Err(Problem(CoreError::not_found("file", kind))),
    };
    let file = app
        .paper_file(
            caller.as_ref().map(|c| &c.0),
            &SubmissionId(id),
            q.version,
            pdf,
        )
        .await?;
    Ok(file_response(file, pdf))
}

fn file_response(file: PaperFile, inline: bool) -> Response {
    let filename: String = file
        .filename
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .collect();
    let disposition = format!(
        "{}; filename=\"{}\"",
        if inline { "inline" } else { "attachment" },
        if filename.is_empty() {
            "paper"
        } else {
            &filename
        }
    );
    let mut response = file.bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(file.content_type),
    );
    if let Ok(value) = HeaderValue::from_str(&disposition) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("sandbox"),
    );
    response
}

async fn list_papers(
    State(app): AppState,
    Query(q): Query<PageQuery>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(app.list_papers(q.limit, q.before).await?))
}

async fn paper(State(app): AppState, Path(id): Path<String>) -> ApiResult<impl IntoResponse> {
    Ok(Json(app.paper(&RecordId(id)).await?))
}

async fn list_conjectures(
    State(app): AppState,
    Query(q): Query<PageQuery>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(app.list_conjectures(q.limit, q.before).await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LeanStatementBody {
    digest: String,
    confirm: bool,
    #[serde(default)]
    comment: String,
}
async fn respond_lean_statement(
    State(app): AppState,
    AuthenticatedCaller(caller): AuthenticatedCaller,
    crate::auth::RequestAuthentication(authentication): crate::auth::RequestAuthentication,
    Path(id): Path<String>,
    Body(body): Body<LeanStatementBody>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        app.respond_lean_statement(
            &caller,
            authentication,
            &SubmissionId(id),
            &body.digest,
            body.confirm,
            body.comment,
        )
        .await?,
    ))
}
