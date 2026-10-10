//! End-to-end sign-in against a fake NyxID: discovery, JWKS, the token
//! endpoint, and RS256 tokens signed by a key generated per test run.

use std::{
    collections::{BTreeSet, HashMap},
    sync::{Arc, Mutex, OnceLock},
};

use axum::{
    Form, Json, Router,
    body::Body,
    extract::State,
    http::{Request, StatusCode, header},
    routing::{get, post},
};
use base64::Engine as _;
use jsonwebtoken::{EncodingKey, Header, encode};
use rsa::{
    RsaPrivateKey,
    pkcs8::{EncodePrivateKey, LineEnding},
    traits::PublicKeyParts,
};
use serde_json::{Value, json};
use tower::ServiceExt;
use url::Url;
use wishpool_core::{app::App, memory::MemoryStores, policy::Policy, ports::SystemClock};

use super::NyxIdClient;
use crate::{
    auth::{AuthState, MemoryAuthStore, Provider, pkce_challenge},
    composition::http_router,
    health::Readiness,
};

const KID: &str = "test-key";
const CLIENT_ID: &str = "wishpool-test";
const PUBLIC: &str = "http://127.0.0.1:5173";

fn signing_key() -> &'static (EncodingKey, Value) {
    static KEY: OnceLock<(EncodingKey, Value)> = OnceLock::new();
    KEY.get_or_init(|| {
        let private = RsaPrivateKey::new(&mut rand::thread_rng(), 2048).unwrap();
        let pem = private.to_pkcs8_pem(LineEnding::LF).unwrap();
        let b64 = |bytes: Vec<u8>| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
        let jwk = json!({
            "kty": "RSA", "kid": KID, "use": "sig", "alg": "RS256",
            "n": b64(private.n().to_bytes_be()), "e": b64(private.e().to_bytes_be()),
        });
        (EncodingKey::from_rsa_pem(pem.as_bytes()).unwrap(), jwk)
    })
}

fn sign(claims: Value) -> String {
    let mut header = Header::new(jsonwebtoken::Algorithm::RS256);
    header.kid = Some(KID.into());
    encode(&header, &claims, &signing_key().0).unwrap()
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// What the fake token endpoint will answer for each code.
#[derive(Default)]
struct FakeNyxId {
    issuer: String,
    /// code -> (expected PKCE challenge, nonce to embed)
    codes: Mutex<HashMap<String, (String, String)>>,
}

async fn token(
    State(p): State<Arc<FakeNyxId>>,
    Form(form): Form<HashMap<String, String>>,
) -> (StatusCode, Json<Value>) {
    if form.get("grant_type").map(String::as_str) == Some("refresh_token") {
        return match form.get("refresh_token").map(String::as_str) {
            Some("refresh-1" | "refresh-2") => (
                StatusCode::OK,
                Json(
                    json!({ "access_token": "delegated-access", "refresh_token": "refresh-2", "expires_in": 900 }),
                ),
            ),
            _ => (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "invalid_grant" })),
            ),
        };
    }
    let code = form.get("code").cloned().unwrap_or_default();
    let Some((challenge, nonce)) = p.codes.lock().unwrap().remove(&code) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid_grant" })),
        );
    };
    let verifier = form.get("code_verifier").cloned().unwrap_or_default();
    if pkce_challenge(&verifier) != challenge
        || form.get("client_secret").map(String::as_str) != Some("s3cret")
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid_grant" })),
        );
    }
    let id_token = sign(json!({
        "sub": "user-1", "iss": p.issuer, "aud": CLIENT_ID, "exp": now() + 3600, "iat": now(),
        "nonce": nonce, "name": "Ada Lovelace", "email": "ada@example.org",
    }));
    (
        StatusCode::OK,
        Json(
            json!({ "access_token": "opaque", "refresh_token": "refresh-1", "id_token": id_token, "token_type": "Bearer" }),
        ),
    )
}

struct Harness {
    router: Router,
    provider: Arc<FakeNyxId>,
    app: Arc<App>,
    client: Arc<NyxIdClient>,
    donations: Arc<crate::donations::Donations>,
    gateway: String,
    stores: Arc<MemoryStores>,
}

/// A fake NyxID LLM gateway: answers one statement judgement with usage.
async fn gateway(
    headers: axum::http::HeaderMap,
    Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
    if headers.get("authorization").and_then(|v| v.to_str().ok()) != Some("Bearer delegated-access")
    {
        return (StatusCode::UNAUTHORIZED, Json(json!({ "error": "no" })));
    }
    assert_eq!(body["model"], "claude-test");
    let content = json!({ "shape": "content", "witnesses": ["Lemma 2: the gap is even"], "rationale": "needs a new parity argument" }).to_string();
    (
        StatusCode::OK,
        Json(
            json!({ "choices": [{ "message": { "content": content } }], "usage": { "prompt_tokens": 900, "completion_tokens": 100 } }),
        ),
    )
}

async fn harness() -> Harness {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let issuer = format!("http://{}", listener.local_addr().unwrap());
    let provider = Arc::new(FakeNyxId {
        issuer: issuer.clone(),
        codes: Mutex::default(),
    });
    let jwk = signing_key().1.clone();
    let discovery = json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{issuer}/oauth/authorize"),
        "token_endpoint": format!("{issuer}/oauth/token"),
        "jwks_uri": format!("{issuer}/.well-known/jwks.json"),
    });
    let fake = Router::new()
        .route(
            "/.well-known/openid-configuration",
            get(move || async move { Json(discovery) }),
        )
        .route(
            "/.well-known/jwks.json",
            get(move || async move { Json(json!({ "keys": [jwk] })) }),
        )
        .route("/oauth/token", post(token))
        .route("/gateway/v1/chat/completions", post(gateway))
        .with_state(provider.clone());
    tokio::spawn(async move { axum::serve(listener, fake).await.unwrap() });

    let client = NyxIdClient::discover(
        &Url::parse(&issuer).unwrap(),
        CLIENT_ID.into(),
        "s3cret".into(),
        format!("{PUBLIC}/auth/callback"),
    )
    .await
    .unwrap();
    let stores = Arc::new(MemoryStores::default());
    let app = App::new(
        stores.ports(Arc::new(SystemClock), Arc::new(crate::latex::LatexReader)),
        Policy::default(),
        BTreeSet::from(["editor".into()]),
    );
    let client = Arc::new(client);
    let gateway = format!("{issuer}/gateway/v1");
    let donations = Arc::new(crate::donations::Donations {
        tokens: Arc::new(crate::donations::MemoryDonorTokens::default()),
        cipher: crate::donations::TokenCipher::from_base64(
            &base64::engine::general_purpose::STANDARD.encode([9_u8; 32]),
        )
        .unwrap(),
        scope: "openid offline_access proxy".into(),
        service_ids: vec![],
        gateway_url: gateway.clone(),
        default_model: "claude-test".into(),
    });
    let auth = Arc::new(AuthState {
        app: app.clone(),
        store: Arc::new(MemoryAuthStore::default()),
        provider: Provider::NyxId(client.clone()),
        public_origin: PUBLIC.into(),
        secure_cookies: false,
        donations: Some(donations.clone()),
    });
    Harness {
        router: http_router(app.clone(), auth, Arc::new(Readiness { mongo: None })),
        provider,
        app,
        client,
        donations,
        gateway,
        stores,
    }
}

struct Reply {
    status: StatusCode,
    location: Option<String>,
    cookies: Vec<String>,
    json: Value,
}

async fn send(router: &Router, request: Request<Body>) -> Reply {
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let location = response
        .headers()
        .get(header::LOCATION)
        .map(|v| v.to_str().unwrap().to_owned());
    let cookies = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_owned())
        .collect();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    Reply {
        status,
        location,
        cookies,
        json: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    }
}

fn get_req(uri: &str, cookie: Option<&str>) -> Request<Body> {
    let mut builder = Request::get(uri);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    builder.body(Body::empty()).unwrap()
}

fn cookie_pair(set_cookie: &[String], name: &str) -> String {
    set_cookie
        .iter()
        .find(|c| c.starts_with(&format!("{name}=")) && !c.starts_with(&format!("{name}=;")))
        .map(|c| c.split(';').next().unwrap().to_owned())
        .unwrap_or_else(|| panic!("no {name} cookie in {set_cookie:?}"))
}

/// Begin sign-in; returns (state, nonce, challenge, binding cookie).
async fn begin(h: &Harness, return_to: &str) -> (String, String, String, String) {
    let reply = send(
        &h.router,
        get_req(&format!("/auth/login?return_to={return_to}"), None),
    )
    .await;
    assert_eq!(reply.status, StatusCode::SEE_OTHER);
    let location = Url::parse(reply.location.as_deref().unwrap()).unwrap();
    let query: HashMap<String, String> = location.query_pairs().into_owned().collect();
    assert_eq!(query["client_id"], CLIENT_ID);
    assert_eq!(query["redirect_uri"], format!("{PUBLIC}/auth/callback"));
    assert_eq!(query["code_challenge_method"], "S256");
    (
        query["state"].clone(),
        query["nonce"].clone(),
        query["code_challenge"].clone(),
        cookie_pair(&reply.cookies, "wp_login"),
    )
}

#[tokio::test]
async fn sign_in_round_trip_sets_a_session() {
    let h = harness().await;
    let (state, nonce, challenge, binding) = begin(&h, "/wishes").await;
    h.provider
        .codes
        .lock()
        .unwrap()
        .insert("code-1".into(), (challenge, nonce));

    let reply = send(
        &h.router,
        get_req(
            &format!("/auth/callback?code=code-1&state={state}"),
            Some(&binding),
        ),
    )
    .await;
    assert_eq!(reply.status, StatusCode::SEE_OTHER, "{:?}", reply.json);
    assert_eq!(reply.location.as_deref(), Some("/wishes"));
    let session = cookie_pair(&reply.cookies, "wp_session");
    assert!(
        reply.cookies.iter().any(|c| c.starts_with("wp_login=;")),
        "binding cookie is cleared"
    );

    let me = send(&h.router, get_req("/api/v1/me", Some(&session))).await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.json["display_name"], "Ada Lovelace");
    let s = send(&h.router, get_req("/auth/session", Some(&session))).await;
    assert_eq!(s.json["authenticated"], true);

    // The state cannot be redeemed twice.
    h.provider
        .codes
        .lock()
        .unwrap()
        .insert("code-2".into(), ("x".into(), "y".into()));
    let replay = send(
        &h.router,
        get_req(
            &format!("/auth/callback?code=code-2&state={state}"),
            Some(&binding),
        ),
    )
    .await;
    assert_eq!(replay.status, StatusCode::UNAUTHORIZED);

    // Cookie-authenticated writes must come from the public origin.
    let body = json!({ "open": true });
    let cross = Request::put("/api/v1/submissions/none/contributors")
        .header(header::COOKIE, &session)
        .header(header::ORIGIN, "https://evil.example")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    assert_eq!(send(&h.router, cross).await.status, StatusCode::FORBIDDEN);
    // From the public origin the request reaches Layer 2 (no such paper).
    let same = Request::put("/api/v1/submissions/none/contributors")
        .header(header::COOKIE, &session)
        .header(header::ORIGIN, PUBLIC)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    assert_eq!(send(&h.router, same).await.status, StatusCode::NOT_FOUND);

    let logout = Request::post("/auth/logout")
        .header(header::COOKIE, &session)
        .header(header::ORIGIN, PUBLIC)
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&h.router, logout).await.status, StatusCode::NO_CONTENT);
    assert_eq!(
        send(&h.router, get_req("/api/v1/me", Some(&session)))
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn callback_requires_the_initiating_browser_and_nonce() {
    let h = harness().await;
    let (state, nonce, challenge, _binding) = begin(&h, "/").await;
    h.provider
        .codes
        .lock()
        .unwrap()
        .insert("c".into(), (challenge.clone(), nonce));
    let foreign = send(
        &h.router,
        get_req(
            &format!("/auth/callback?code=c&state={state}"),
            Some("wp_login=someone-else"),
        ),
    )
    .await;
    assert_eq!(foreign.status, StatusCode::UNAUTHORIZED);

    let (state, _nonce, challenge, binding) = begin(&h, "/").await;
    h.provider
        .codes
        .lock()
        .unwrap()
        .insert("c2".into(), (challenge, "wrong-nonce".into()));
    let reply = send(
        &h.router,
        get_req(
            &format!("/auth/callback?code=c2&state={state}"),
            Some(&binding),
        ),
    )
    .await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn open_redirects_are_neutralised() {
    let h = harness().await;
    let (state, nonce, challenge, binding) = begin(&h, "https://evil.example").await;
    h.provider
        .codes
        .lock()
        .unwrap()
        .insert("c".into(), (challenge, nonce));
    let reply = send(
        &h.router,
        get_req(
            &format!("/auth/callback?code=c&state={state}"),
            Some(&binding),
        ),
    )
    .await;
    assert_eq!(reply.location.as_deref(), Some("/"));
}

#[tokio::test]
async fn bearer_tokens_are_verified_locally() {
    let h = harness().await;
    let issuer = h.provider.issuer.clone();
    let bearer = |claims: Value| {
        Request::get("/api/v1/me")
            .header(header::AUTHORIZATION, format!("Bearer {}", sign(claims)))
            .body(Body::empty())
            .unwrap()
    };
    let base =
        json!({ "sub": "svc-1", "iss": issuer, "aud": issuer, "exp": now() + 600, "iat": now() });

    let mut access = base.clone();
    access["token_type"] = "access".into();
    assert_eq!(send(&h.router, bearer(access)).await.status, StatusCode::OK);

    let mut service = base.clone();
    service["sa"] = true.into();
    assert_eq!(
        send(&h.router, bearer(service)).await.status,
        StatusCode::OK
    );

    let mut refresh = base.clone();
    refresh["token_type"] = "refresh".into();
    assert_eq!(
        send(&h.router, bearer(refresh)).await.status,
        StatusCode::UNAUTHORIZED
    );

    let mut wrong_audience = base.clone();
    wrong_audience["token_type"] = "access".into();
    wrong_audience["aud"] = CLIENT_ID.into();
    assert_eq!(
        send(&h.router, bearer(wrong_audience)).await.status,
        StatusCode::UNAUTHORIZED
    );

    let mut expired = base;
    expired["token_type"] = "access".into();
    expired["exp"] = (now() - 3600).into();
    assert_eq!(
        send(&h.router, bearer(expired)).await.status,
        StatusCode::UNAUTHORIZED
    );

    let garbage = Request::get("/api/v1/me")
        .header(header::AUTHORIZATION, "Bearer not-a-jwt")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        send(&h.router, garbage).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn donated_quota_runs_a_hosted_judgement() {
    use wishpool_core::{
        app::Upload,
        model::{
            AiDisclosure, AiUse, ClaimConfirmation, ContributionStatus, GrantStatus, NewPaper,
            Role, TaskKind, VerifiedIdentity,
        },
        ports::TaskFilter,
    };
    let h = harness().await;
    // Sign in as Ada (user-1).
    let (state, nonce, challenge, binding) = begin(&h, "/").await;
    h.provider
        .codes
        .lock()
        .unwrap()
        .insert("c1".into(), (challenge, nonce));
    let reply = send(
        &h.router,
        get_req(
            &format!("/auth/callback?code=c1&state={state}"),
            Some(&binding),
        ),
    )
    .await;
    let session = cookie_pair(&reply.cookies, "wp_session");

    // A link on another site cannot start the consent.
    let cross = Request::get("/auth/donate?cap=50000")
        .header(header::COOKIE, &session)
        .header("sec-fetch-site", "cross-site")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&h.router, cross).await.status, StatusCode::FORBIDDEN);

    // Ask to donate: incremental consent with offline access.
    let reply = send(
        &h.router,
        get_req("/auth/donate?cap=50000&model=claude-test", Some(&session)),
    )
    .await;
    assert_eq!(reply.status, StatusCode::SEE_OTHER);
    let location = Url::parse(reply.location.as_deref().unwrap()).unwrap();
    let query: HashMap<String, String> = location.query_pairs().into_owned().collect();
    assert_eq!(query["scope"], "openid offline_access proxy");
    assert_eq!(query["include_granted_scopes"], "true");
    let binding = cookie_pair(&reply.cookies, "wp_login");
    h.provider.codes.lock().unwrap().insert(
        "c2".into(),
        (query["code_challenge"].clone(), query["nonce"].clone()),
    );
    let reply = send(
        &h.router,
        get_req(
            &format!("/auth/callback?code=c2&state={}", query["state"]),
            Some(&format!("{session}; {binding}")),
        ),
    )
    .await;
    assert_eq!(reply.status, StatusCode::SEE_OTHER, "{:?}", reply.json);
    assert_eq!(reply.location.as_deref(), Some("/contribute"));

    // The refresh token is stored sealed, never in the clear.
    let sealed = h
        .donations
        .tokens
        .get(&"user-1".into())
        .await
        .unwrap()
        .unwrap();
    assert!(!sealed.contains("refresh-1"));
    assert_eq!(h.donations.cipher.open(&sealed).unwrap(), "refresh-1");
    let me = send(&h.router, get_req("/api/v1/donation", Some(&session))).await;
    assert_eq!(me.json["monthly_cap"], 50000);
    assert_eq!(me.json["status"], "active");

    // An author opens a paper to contributors: its theorem gets a
    // judgement task and a literature task.
    let identity = |subject: &str| VerifiedIdentity {
        subject: subject.into(),
        name: None,
        email: None,
        picture: None,
    };
    let editor = h.app.caller(&identity("editor")).await.unwrap();
    h.app.sign_in(&identity("engine")).await.unwrap();
    h.app
        .set_roles(&editor, &"engine".into(), &[Role::Reviewer])
        .await
        .unwrap();
    let engine = h.app.caller(&identity("engine")).await.unwrap();
    let author = h.app.caller(&identity("author")).await.unwrap();
    let source = br"\documentclass{article}\newtheorem{theorem}{Theorem}
\title{Even gaps}\author{A. Author}\begin{document}
\begin{theorem}Every gap is even.\end{theorem}\begin{proof}Parity.\end{proof}
\end{document}";
    let paper = h
        .app
        .submit_paper(
            &author,
            NewPaper {
                kind: wishpool_core::model::SubmissionKind::Paper,
                make_public_after_acceptance: true,
                typed_conjecture: None,
                ai_disclosure: AiDisclosure {
                    level: AiUse::None,
                    statement: "No AI was used.".into(),
                },
                authors: vec![],
                msc: vec![],
                doi: None,
                open_to_contributors: true,
            },
            Upload {
                filename: "even.tex".into(),
                bytes: source.to_vec(),
            },
        )
        .await
        .unwrap();
    h.app
        .record_compilation(&engine, &paper.id, 1, Ok(b"%PDF".to_vec()))
        .await
        .unwrap();
    let theorem = &paper.extracted[0];
    h.app
        .confirm_claims(
            &author,
            &paper.id,
            vec![ClaimConfirmation {
                id: theorem.id.clone(),
                kind: theorem.kind,
                role: theorem.role,
                depends_on: vec![],
                settles: None,
                excluded: false,
            }],
        )
        .await
        .unwrap();

    let worker = crate::donations::HostedWorker {
        app: h.app.clone(),
        nyxid: h.client.clone(),
        donations: h.donations.clone(),
        openalex: Arc::new(
            wishpool_review::openalex::OpenAlex::new("http://127.0.0.1:9", None).unwrap(),
        ),
        interval: std::time::Duration::from_secs(1),
    };
    assert_eq!(worker.round_for_test().await.unwrap(), 1);
    let contributions = h
        .app
        .list_contributions(Default::default(), None, None)
        .await
        .unwrap()
        .items;
    assert_eq!(contributions.len(), 1);
    let tokens = contributions[0].tokens.unwrap();
    assert_eq!(
        (tokens.input, tokens.output, tokens.metered),
        (900, 100, true)
    );
    assert_eq!(
        contributions[0].status,
        ContributionStatus::Submitted,
        "one family alone does not corroborate"
    );
    let grant = h
        .app
        .donation(
            &h.app
                .caller(&VerifiedIdentity {
                    subject: "user-1".into(),
                    name: None,
                    email: None,
                    picture: None,
                })
                .await
                .unwrap(),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!((grant.used, grant.status), (1_000, GrantStatus::Active));
    // The rotated refresh token replaced the old one.
    assert_eq!(
        h.donations
            .cipher
            .open(
                &h.donations
                    .tokens
                    .get(&"user-1".into())
                    .await
                    .unwrap()
                    .unwrap()
            )
            .unwrap(),
        "refresh-2"
    );
    // The donor already judged the theorem; the literature task needs
    // OpenAlex, which is unreachable here, so the lease is released.
    assert_eq!(worker.round_for_test().await.unwrap(), 0);
    let literature = h
        .app
        .list_tasks(
            TaskFilter {
                kind: Some(TaskKind::LiteratureCheck),
                status: Some("open".into()),
                ..Default::default()
            },
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(literature.items.len(), 1);
    let open = h
        .app
        .list_tasks(
            TaskFilter {
                kind: Some(TaskKind::JudgeEscape),
                status: Some("open".into()),
                ..Default::default()
            },
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(open.items.len(), 1);
    let _ = &h.gateway;
}

#[tokio::test]
async fn lean_target_response_requires_cookie_provenance_even_with_a_valid_bearer() {
    use wishpool_core::{app::Upload, model::*, ports::SubmissionStore};
    let h = harness().await;
    let (state, nonce, challenge, binding) = begin(&h, "/submit").await;
    h.provider
        .codes
        .lock()
        .unwrap()
        .insert("target-code".into(), (challenge, nonce));
    let reply = send(
        &h.router,
        get_req(
            &format!("/auth/callback?code=target-code&state={state}"),
            Some(&binding),
        ),
    )
    .await;
    let session = cookie_pair(&reply.cookies, "wp_session");
    let author = h
        .app
        .caller(&VerifiedIdentity {
            subject: "user-1".into(),
            name: None,
            email: None,
            picture: None,
        })
        .await
        .unwrap();
    let mut paper = h
        .app
        .submit_paper(
            &author,
            serde_json::from_value(json!({
                "kind":"conjecture", "authors":[{"name":"Ada Lovelace"}],
                "ai_disclosure":{"level":"none","statement":"No AI used."},
                "typed_conjecture":{"title":"Target","statement":"An open mathematical assertion."}
            }))
            .unwrap(),
            Upload {
                filename: String::new(),
                bytes: vec![],
            },
        )
        .await
        .unwrap();
    // Seed the already elaborated/delivered state: this test exercises HTTP auth,
    // while the core and worker tests exercise admission and generation.
    paper.claims = paper.extracted.clone();
    paper.claims_revision = 1;
    paper.status = SubmissionStatus::Accepted {
        record: "WP-2026-9999".into(),
    };
    let digest = "d".repeat(64);
    paper.lean_statements.push(LeanStatementAttempt {
        claim: "C1".into(),
        version: 1,
        claims_revision: 1,
        lean: "theorem wishpool_target : True := by sorry".into(),
        digest: digest.clone(),
        toolchain: "test Lean".into(),
        reading: "The target assertion.".into(),
        response: LeanStatementResponse::AwaitingAuthor,
        created_at: chrono::Utc::now(),
    });
    let expected = paper.revision;
    paper.revision += 1;
    h.stores.replace(&paper, expected).await.unwrap();
    let token = sign(
        json!({"sub":"user-1", "iss":h.provider.issuer, "aud":h.provider.issuer,
        "exp":now()+600, "iat":now(), "token_type":"access"}),
    );
    let uri = format!("/api/v1/submissions/{}/lean-statement/response", paper.id);
    for cookie in [false, true] {
        let mut request = Request::post(&uri)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ORIGIN, PUBLIC);
        if cookie {
            request = request.header(header::COOKIE, &session);
        }
        let reply = send(
            &h.router,
            request
                .body(Body::from(
                    json!({"digest":digest, "confirm":true}).to_string(),
                ))
                .unwrap(),
        )
        .await;
        assert_eq!(reply.status, StatusCode::FORBIDDEN, "{:?}", reply.json);
    }
    let request = |origin: &str, digest: &str| {
        Request::post(&uri)
            .header(header::COOKIE, &session)
            .header(header::ORIGIN, origin)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({"digest":digest, "confirm":true}).to_string(),
            ))
            .unwrap()
    };
    assert_eq!(
        send(&h.router, request("https://other.invalid", &digest))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(&h.router, request(PUBLIC, "stale")).await.status,
        StatusCode::CONFLICT
    );
    let confirmed = send(&h.router, request(PUBLIC, &digest)).await;
    assert_eq!(confirmed.status, StatusCode::OK, "{:?}", confirmed.json);
    assert_eq!(
        confirmed.json["lean_statements"][0]["response"]["state"],
        "confirmed"
    );
    assert_eq!(
        confirmed.json["lean_statements"][0]["response"]["author"],
        "user-1"
    );
}
