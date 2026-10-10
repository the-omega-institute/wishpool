//! Wire configuration to concrete adapters and build the HTTP router.

use std::{sync::Arc, time::Duration};

use axum::{Router, extract::DefaultBodyLimit, middleware};
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};
use wishpool_core::{
    app::App,
    memory::MemoryStores,
    model::Role,
    policy::Policy,
    ports::{Ports, SystemClock},
};
use wishpool_review::{ReviewModel, openai_compat::ChatModel, openalex::OpenAlex};

use crate::{
    auth::{self, AuthState, AuthStore, MemoryAuthStore, Provider, nyxid::NyxIdClient},
    config::{AdvisorConfig, AuthMode, Config, OracleConfig, Storage},
    donations::{Donations, DonorTokens, HostedWorker, MemoryDonorTokens, TokenCipher},
    health::{self, Readiness},
    latex::{Compile, LatexReader},
    review::{JobLease, MemoryJobs, Worker},
    store::MongoStore,
};

/// What the chosen storage provides to the rest of the composition.
type StorageParts = (
    Ports,
    Arc<dyn AuthStore>,
    Arc<dyn JobLease>,
    Option<MongoStore>,
);

// Two 30-second environment commands and at most two 120-second Lean checks.
// Audit uses the same margin for workspace preparation and result parsing.
const REVIEW_STEP_MARGIN_SECS: u64 = 5 * 60;

pub const REQUEST_BODY_LIMIT_BYTES: usize = 2 * 1024 * 1024;

pub struct Composition {
    pub router: Router,
    pub worker: Worker,
    pub mongo: Option<MongoStore>,
    pub app: Arc<App>,
    pub hosted: Option<Arc<HostedWorker>>,
}

impl Composition {
    pub async fn build(config: &Config) -> anyhow::Result<Self> {
        let (mut ports, auth_store, jobs, mongo): StorageParts = match &config.storage {
            Storage::Mongo { uri, database } => {
                let store = MongoStore::connect(uri, database).await?;
                let shared = Arc::new(store.clone());
                let ports = Ports {
                    solving: shared.clone(),
                    verifier: Arc::new(wishpool_core::ports::UnavailableVerifier),
                    clock: Arc::new(SystemClock),
                    people: shared.clone(),
                    submissions: shared.clone(),
                    endorsements: shared.clone(),
                    records: shared.clone(),
                    blobs: shared.clone(),
                    reader: Arc::new(LatexReader),
                    queue: shared.clone(),
                    tasks: shared.clone(),
                    contributions: shared.clone(),
                    judgements: shared.clone(),
                    grants: shared.clone(),
                    referees: shared.clone(),
                };
                (ports, shared.clone(), shared, Some(store))
            }
            Storage::Memory => {
                tracing::warn!("memory storage: all state is lost on exit");
                let stores = Arc::new(MemoryStores::default());
                let ports = stores.ports(Arc::new(SystemClock), Arc::new(LatexReader));
                (
                    ports,
                    Arc::new(MemoryAuthStore::default()),
                    Arc::new(MemoryJobs::new(stores)),
                    None,
                )
            }
        };

        let timeout = Duration::from_secs(config.verifier_timeout_secs);
        if let Some(url) = &config.verifier_url {
            ports.verifier = Arc::new(crate::verifier::HttpVerifier {
                url: url.clone(),
                timeout,
            });
        } else if let Some(workspace) = &config.lean_workspace {
            std::fs::create_dir_all(&config.advisor_work_dir)?;
            ports.verifier = Arc::new(crate::verifier::ProcessVerifier {
                program: config.verifier_program.clone().into(),
                workspace: workspace.into(),
                scratch: config.advisor_work_dir.clone().into(),
                timeout,
            });
        }
        let app = App::with_attempt_limit(
            ports,
            Policy::default(),
            config.bootstrap_admins.clone(),
            config.attempts_per_day,
        );

        let provider = match &config.auth {
            AuthMode::NyxId {
                base_url,
                client_id,
                client_secret,
            } => {
                let redirect_uri = config.public_url.join("auth/callback")?.to_string();
                let client = NyxIdClient::discover(
                    base_url,
                    client_id.clone(),
                    client_secret.clone(),
                    redirect_uri,
                )
                .await?;
                Provider::NyxId(Arc::new(client))
            }
            AuthMode::Dev => {
                tracing::warn!("development sign-in: anyone can sign in as any name");
                Provider::Dev
            }
        };
        let donations = match &config.hosted {
            Some(hosted) => {
                let tokens: Arc<dyn DonorTokens> = match &mongo {
                    Some(store) => Arc::new(store.clone()),
                    None => Arc::new(MemoryDonorTokens::default()),
                };
                Some(Arc::new(Donations {
                    tokens,
                    cipher: TokenCipher::from_base64(&hosted.token_key)?,
                    scope: hosted.scope.clone(),
                    service_ids: hosted.service_ids.clone(),
                    gateway_url: hosted.gateway_url.clone(),
                    default_model: hosted.default_model.clone(),
                }))
            }
            None => None,
        };
        let openalex = Arc::new(OpenAlex::new(
            &config.openalex_url,
            config.openalex_api_key.clone(),
        )?);
        let hosted = match (&provider, &donations, &config.hosted) {
            (Provider::NyxId(client), Some(donations), Some(hosted)) => {
                Some(Arc::new(HostedWorker {
                    app: app.clone(),
                    nyxid: client.clone(),
                    donations: donations.clone(),
                    openalex: openalex.clone(),
                    interval: Duration::from_secs(hosted.interval_secs),
                }))
            }
            _ => None,
        };
        let auth_state = Arc::new(AuthState {
            app: app.clone(),
            store: auth_store,
            provider,
            public_origin: config.public_origin(),
            secure_cookies: config.secure_cookies(),
            donations,
        });

        let reviewer = app
            .ensure_service_account(
                &config.review_account,
                "Wishpool paper engine",
                Role::Reviewer,
            )
            .await?;
        let model: Option<Arc<dyn ReviewModel>> = match &config.review_model {
            Some(m) => Some(Arc::new(ChatModel::new(
                &m.base_url,
                m.token.clone(),
                m.model.clone(),
            )?)),
            None => {
                tracing::warn!(
                    "no review model configured; literature and escape stages wait for editors"
                );
                None
            }
        };
        let referee_account = app
            .ensure_service_account(&config.referee_account, "Wishpool referee", Role::Reviewer)
            .await?;
        let oracle: Option<Arc<dyn wishpool_review::oracle::Oracle>> = match &config.oracle {
            Some(OracleConfig::Http {
                base_url,
                token,
                pools,
                ..
            }) => Some(Arc::new(wishpool_review::oracle::OracleHttp {
                base_url: base_url.clone(),
                token: token.0.clone(),
                pools: pools.clone(),
            })),
            Some(OracleConfig::Cli { program, pools, .. }) => {
                Some(Arc::new(wishpool_review::oracle::OracleCli {
                    program: program.into(),
                    pools: pools.clone(),
                    work_dir: std::path::PathBuf::from(&config.advisor_work_dir).join("oracle"),
                }))
            }
            None => {
                tracing::warn!("no oracle configured; referee rounds are disabled");
                None
            }
        };
        let referee_model = match &config.oracle {
            Some(OracleConfig::Http { model, .. } | OracleConfig::Cli { model, .. }) => {
                model.clone()
            }
            None => "chatgpt-pro".into(),
        };
        let formalizer: Option<Arc<dyn wishpool_review::lean::Formalizer>> =
            match (&config.lean_workspace, &config.advisor) {
                (Some(workspace), Some(AdvisorConfig::Codex { program, model })) => {
                    Some(Arc::new(wishpool_review::lean::CodexLean {
                        program: program.into(),
                        model: model.clone(),
                        workspace: workspace.into(),
                        timeout: Duration::from_secs(config.formal_timeout_secs),
                        check_timeout: Duration::from_secs(120),
                    }))
                }
                _ => None,
            };
        let auditor_account = app
            .ensure_service_account(&config.auditor_account, "Wishpool auditor", Role::Reviewer)
            .await?;
        let advisor: Option<Arc<dyn wishpool_review::advisor::Advisor>> = match &config.advisor {
            Some(AdvisorConfig::Codex { program, model }) => {
                Some(Arc::new(wishpool_review::advisor::CodexCli {
                    program: program.into(),
                    model: model.clone(),
                    timeout: Duration::from_secs(config.advisor_timeout_secs),
                    audit_timeout: Duration::from_secs(config.audit_timeout_secs),
                }))
            }
            Some(AdvisorConfig::Chat { model }) => {
                let endpoint = config
                    .review_model
                    .as_ref()
                    .expect("chat advisor config validated");
                Some(Arc::new(ChatModel::new(
                    &endpoint.base_url,
                    endpoint.token.clone(),
                    model.clone(),
                )?))
            }
            None => None,
        };
        let worker = Worker {
            app: app.clone(),
            jobs,
            compile: Compile {
                tex_bin: config.tex_bin.clone().into(),
                cache_dir: config.tex_cache.clone().into(),
                texmf_home: config.texmf_home.clone().map(Into::into),
                timeout: Duration::from_secs(config.compile_timeout_secs),
            },
            model,
            openalex,
            reviewer,
            referee_account,
            auditor_account,
            referee_model,
            oracle,
            advisor,
            formalizer,
            oracle_poll: Duration::from_secs(config.oracle_poll_secs),
            advisor_work_dir: config.advisor_work_dir.clone().into(),
            audit_budget: Duration::from_secs(config.audit_timeout_secs + REVIEW_STEP_MARGIN_SECS),
            formal_budget: Duration::from_secs(
                config.formal_timeout_secs + REVIEW_STEP_MARGIN_SECS,
            ),
        };

        let router = http_router(
            app.clone(),
            auth_state,
            Arc::new(Readiness {
                mongo: mongo.clone(),
            }),
        );
        Ok(Self {
            router,
            worker,
            mongo,
            app,
            hosted,
        })
    }
}

/// The complete HTTP surface: `/api/v1`, `/auth`, health.
pub fn http_router(app: Arc<App>, auth_state: Arc<AuthState>, readiness: Arc<Readiness>) -> Router {
    let api = wishpool_public::router(app).layer(middleware::from_fn_with_state(
        auth_state.clone(),
        auth::authenticate,
    ));
    Router::new()
        .nest("/api/v1", api)
        .merge(auth::routes(auth_state))
        .merge(health::routes(readiness))
        .layer(DefaultBodyLimit::max(REQUEST_BODY_LIMIT_BYTES))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(TraceLayer::new_for_http())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
}
