//! Deployment configuration, read once from the environment. Every key read
//! here is listed in `infra/api/configmap.yaml` or `infra/api/secret.yaml`.

use std::{collections::BTreeSet, net::SocketAddr};

use anyhow::{Context, bail};
use url::Url;
use wishpool_core::ids::PersonId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Storage {
    Mongo {
        uri: String,
        database: String,
    },
    /// Process memory. Local development only; refused on non-loopback binds.
    Memory,
}

#[derive(Clone, PartialEq, Eq)]
pub enum AuthMode {
    NyxId {
        base_url: Url,
        client_id: String,
        client_secret: String,
    },
    /// Sign in as any name without a provider. Local development only;
    /// refused unless the server binds to loopback.
    Dev,
}

impl std::fmt::Debug for AuthMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NyxId {
                base_url,
                client_id,
                ..
            } => f
                .debug_struct("NyxId")
                .field("base_url", &base_url.as_str())
                .field("client_id", client_id)
                .field("client_secret", &"<redacted>")
                .finish(),
            Self::Dev => f.write_str("Dev"),
        }
    }
}

#[derive(Clone)]
pub struct ReviewModelConfig {
    pub base_url: String,
    pub token: String,
    pub model: String,
}

impl std::fmt::Debug for ReviewModelConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReviewModelConfig")
            .field("base_url", &self.base_url)
            .field("token", &"<redacted>")
            .field("model", &self.model)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub enum OracleConfig {
    Http {
        base_url: String,
        token: OracleToken,
        pools: Vec<String>,
        model: String,
    },
    Cli {
        program: String,
        pools: Vec<String>,
        model: String,
    },
}

#[derive(Clone)]
pub struct OracleToken(pub String);

impl std::fmt::Debug for OracleToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<redacted>")
    }
}

#[derive(Debug, Clone)]
pub enum AdvisorConfig {
    Codex {
        program: String,
        model: Option<String>,
    },
    Chat {
        model: String,
    },
}

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: SocketAddr,
    /// The browser-facing origin, e.g. `https://wishpool.example.org`.
    pub public_url: Url,
    pub storage: Storage,
    pub auth: AuthMode,
    pub bootstrap_admins: BTreeSet<PersonId>,
    /// The machine review model; when absent, model-driven stages wait for humans.
    pub review_model: Option<ReviewModelConfig>,
    /// Subject of the machine reviewer account the worker files reports as.
    pub review_account: PersonId,
    pub oracle: Option<OracleConfig>,
    pub oracle_poll_secs: u64,
    pub referee_account: PersonId,
    pub auditor_account: PersonId,
    pub advisor: Option<AdvisorConfig>,
    /// Deadline for the Codex audit itself. Advice and letters use the
    /// shorter `advisor_timeout_secs` deadline.
    pub audit_timeout_secs: u64,
    pub advisor_timeout_secs: u64,
    pub advisor_work_dir: String,
    /// A Lean project with Mathlib built; with the Codex advisor, rounds
    /// try a private formalization probe in it.
    pub lean_workspace: Option<String>,
    pub formal_timeout_secs: u64,
    /// The TeX Live bin directory (`pdflatex`, `xelatex`, `lualatex`,
    /// `bibtex`) that compiles uploaded sources.
    pub tex_bin: String,
    /// Writable directory for TeX's font and format caches.
    pub tex_cache: String,
    /// Extra texmf tree with packages beyond the distribution.
    pub texmf_home: Option<String>,
    pub compile_timeout_secs: u64,
    /// Donated quota, when enabled (requires NyxID sign-in).
    pub hosted: Option<HostedConfig>,
    pub openalex_url: String,
    pub openalex_api_key: Option<String>,
    pub role: Role,
}

#[derive(Clone)]
pub struct HostedConfig {
    /// Base64 of 32 bytes; seals delegated refresh tokens.
    pub token_key: String,
    pub gateway_url: String,
    pub scope: String,
    pub service_ids: Vec<String>,
    pub default_model: String,
    pub interval_secs: u64,
}

impl std::fmt::Debug for HostedConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostedConfig")
            .field("token_key", &"<redacted>")
            .field("gateway_url", &self.gateway_url)
            .field("scope", &self.scope)
            .field("default_model", &self.default_model)
            .finish()
    }
}

/// Which parts of the service this process runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// HTTP only: stateless replicas behind the load balancer.
    Api,
    /// Background workers (compile, review, hosted, reconcile), one
    /// replica; serves health endpoints.
    Worker,
    /// Both, for single-process deployments and local work.
    All,
}

impl Role {
    pub fn runs_workers(self) -> bool {
        self != Role::Api
    }
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> anyhow::Result<Self> {
        let get = |key: &str| {
            lookup(key)
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        let required = |key: &str| get(key).with_context(|| format!("{key} is required"));

        let bind: SocketAddr = get("WISHPOOL_BIND")
            .unwrap_or_else(|| "0.0.0.0:8080".into())
            .parse()
            .context("WISHPOOL_BIND must be host:port")?;
        let public_url =
            Url::parse(&required("WISHPOOL_PUBLIC_URL")?).context("WISHPOOL_PUBLIC_URL")?;
        if !matches!(public_url.scheme(), "http" | "https") || public_url.path() != "/" {
            bail!("WISHPOOL_PUBLIC_URL must be a bare http(s) origin");
        }

        let storage = match get("WISHPOOL_STORAGE").as_deref().unwrap_or("mongo") {
            "mongo" => Storage::Mongo {
                uri: required("WISHPOOL_MONGODB_URI")?,
                database: get("WISHPOOL_MONGODB_DATABASE").unwrap_or_else(|| "wishpool".into()),
            },
            "memory" => Storage::Memory,
            other => bail!("WISHPOOL_STORAGE must be mongo or memory, not {other}"),
        };

        let auth = match get("WISHPOOL_AUTH_MODE").as_deref().unwrap_or("nyxid") {
            "nyxid" => AuthMode::NyxId {
                base_url: Url::parse(
                    &get("CHRONO_NYXID_BASE_URL")
                        .unwrap_or_else(|| "https://nyx.chrono-ai.fun".into()),
                )
                .context("CHRONO_NYXID_BASE_URL")?,
                client_id: required("CHRONO_NYXID_CLIENT_ID")?,
                client_secret: required("CHRONO_NYXID_CLIENT_SECRET")?,
            },
            "dev" => AuthMode::Dev,
            other => bail!("WISHPOOL_AUTH_MODE must be nyxid or dev, not {other}"),
        };

        let loopback = bind.ip().is_loopback();
        if !loopback && (auth == AuthMode::Dev || storage == Storage::Memory) {
            bail!("dev sign-in and memory storage require a loopback WISHPOOL_BIND");
        }

        let bootstrap_admins = get("WISHPOOL_ADMIN_SUBJECTS")
            .map(|v| {
                v.split(',')
                    .map(|s| PersonId(s.trim().to_owned()))
                    .filter(|p| !p.0.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        let review_model = match (
            get("WISHPOOL_REVIEW_MODEL_BASE_URL"),
            get("WISHPOOL_REVIEW_MODEL_TOKEN"),
        ) {
            (Some(base_url), Some(token)) => Some(ReviewModelConfig {
                base_url,
                token,
                model: get("WISHPOOL_REVIEW_MODEL").unwrap_or_else(|| "claude-opus-5-5".into()),
            }),
            (None, None) => None,
            _ => bail!(
                "WISHPOOL_REVIEW_MODEL_BASE_URL and WISHPOOL_REVIEW_MODEL_TOKEN are set together"
            ),
        };

        // Every listed pool receives the request; the first answer wins.
        let oracle_pools: Vec<String> = get("WISHPOOL_ORACLE_POOL")
            .unwrap_or_else(|| "chrono-chatgpt-pro-pool".into())
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect();
        if oracle_pools.is_empty() {
            bail!("WISHPOOL_ORACLE_POOL names no pool");
        }
        // The label recorded on referee judgements; each pool applies its
        // own default model.
        let oracle_model = get("WISHPOOL_ORACLE_MODEL").unwrap_or_else(|| "chatgpt-6-pro".into());
        let oracle = match get("WISHPOOL_ORACLE").as_deref() {
            None => None,
            Some("http") => Some(OracleConfig::Http {
                base_url: get("WISHPOOL_ORACLE_BASE_URL").unwrap_or_else(|| {
                    "https://nyx-api.chrono-ai.fun/api/v1/proxy/s/oracle".into()
                }),
                token: OracleToken(required("WISHPOOL_ORACLE_TOKEN")?),
                pools: oracle_pools,
                model: oracle_model,
            }),
            Some("cli") if loopback => Some(OracleConfig::Cli {
                program: get("WISHPOOL_ORACLE_CLI").unwrap_or_else(|| "nyxid".into()),
                pools: oracle_pools,
                model: oracle_model,
            }),
            Some("cli") => bail!("WISHPOOL_ORACLE=cli requires a loopback WISHPOOL_BIND"),
            Some(other) => bail!("WISHPOOL_ORACLE must be http or cli, not {other}"),
        };
        let advisor = match get("WISHPOOL_ADVISOR").as_deref() {
            None => None,
            Some("codex") if loopback => Some(AdvisorConfig::Codex {
                program: get("WISHPOOL_CODEX_BIN").unwrap_or_else(|| "codex".into()),
                model: get("WISHPOOL_CODEX_MODEL"),
            }),
            Some("codex") => bail!("WISHPOOL_ADVISOR=codex requires a loopback WISHPOOL_BIND"),
            Some("chat") => {
                if review_model.is_none() {
                    bail!(
                        "WISHPOOL_ADVISOR=chat requires WISHPOOL_REVIEW_MODEL_BASE_URL and WISHPOOL_REVIEW_MODEL_TOKEN"
                    );
                }
                Some(AdvisorConfig::Chat {
                    model: get("WISHPOOL_ADVISOR_MODEL").unwrap_or_else(|| "gpt-5.5".into()),
                })
            }
            Some(other) => bail!("WISHPOOL_ADVISOR must be codex or chat, not {other}"),
        };
        let seconds = |key: &str, default: u64| -> anyhow::Result<u64> {
            let value = get(key)
                .map(|v| v.parse::<u64>())
                .transpose()
                .with_context(|| format!("{key} must be seconds"))?
                .unwrap_or(default);
            if value == 0 {
                bail!("{key} must be positive");
            }
            Ok(value)
        };
        let oracle_poll_secs = seconds("WISHPOOL_ORACLE_POLL_SECS", 60)?;
        let advisor_timeout_secs = seconds("WISHPOOL_ADVISOR_TIMEOUT_SECS", 1200)?;
        if advisor_timeout_secs > 3600 {
            bail!("WISHPOOL_ADVISOR_TIMEOUT_SECS must be at most 3600 seconds");
        }
        let audit_timeout_secs = seconds("WISHPOOL_AUDIT_TIMEOUT_SECS", 3600)?;
        if audit_timeout_secs > 7200 {
            bail!("WISHPOOL_AUDIT_TIMEOUT_SECS must be at most 7200 seconds");
        }
        let formal_timeout_secs = seconds("WISHPOOL_FORMAL_TIMEOUT_SECS", 1200)?;
        if formal_timeout_secs > 3600 {
            bail!("WISHPOOL_FORMAL_TIMEOUT_SECS must be at most 3600 seconds");
        }
        let lean_workspace = get("WISHPOOL_LEAN_WORKSPACE");
        if lean_workspace.is_some() && !matches!(advisor, Some(AdvisorConfig::Codex { .. })) {
            bail!("WISHPOOL_LEAN_WORKSPACE requires WISHPOOL_ADVISOR=codex");
        }

        let review_account = PersonId(
            get("WISHPOOL_REVIEW_ACCOUNT").unwrap_or_else(|| "wishpool:review-engine".into()),
        );
        let referee_account =
            PersonId(get("WISHPOOL_REFEREE_ACCOUNT").unwrap_or_else(|| "wishpool:referee".into()));
        if oracle.is_some() && review_account == referee_account {
            bail!("WISHPOOL_REFEREE_ACCOUNT must differ from WISHPOOL_REVIEW_ACCOUNT");
        }

        let auditor_account =
            PersonId(get("WISHPOOL_AUDITOR_ACCOUNT").unwrap_or_else(|| "wishpool:auditor".into()));
        if auditor_account == referee_account || auditor_account == review_account {
            bail!("WISHPOOL_AUDITOR_ACCOUNT must differ from referee and review accounts");
        }

        let hosted = match get("WISHPOOL_HOSTED_DONATIONS").as_deref() {
            Some("true" | "1") => {
                let AuthMode::NyxId { base_url, .. } = &auth else {
                    bail!("donated quota needs NyxID sign-in");
                };
                Some(HostedConfig {
                    token_key: required("WISHPOOL_TOKEN_KEY")?,
                    gateway_url: get("WISHPOOL_LLM_GATEWAY_URL").unwrap_or_else(|| {
                        format!(
                            "{}/api/v1/llm/gateway/v1",
                            base_url.as_str().trim_end_matches('/')
                        )
                    }),
                    scope: get("WISHPOOL_DONATION_SCOPE")
                        .unwrap_or_else(|| "openid offline_access proxy".into()),
                    service_ids: get("WISHPOOL_DONATION_SERVICE_IDS")
                        .map(|v| {
                            v.split(',')
                                .map(|s| s.trim().to_owned())
                                .filter(|s| !s.is_empty())
                                .collect()
                        })
                        .unwrap_or_default(),
                    default_model: get("WISHPOOL_DONATION_MODEL")
                        .unwrap_or_else(|| "claude-opus-5-5".into()),
                    interval_secs: get("WISHPOOL_HOSTED_INTERVAL_SECS")
                        .map(|v| v.parse())
                        .transpose()
                        .context("WISHPOOL_HOSTED_INTERVAL_SECS")?
                        .unwrap_or(30),
                })
            }
            _ => None,
        };

        Ok(Self {
            bind,
            public_url,
            storage,
            auth,
            bootstrap_admins,
            review_model,
            oracle,
            oracle_poll_secs,
            referee_account,
            auditor_account,
            advisor,
            audit_timeout_secs,
            advisor_timeout_secs,
            advisor_work_dir: get("WISHPOOL_ADVISOR_WORK_DIR")
                .unwrap_or_else(|| "/tmp/wishpool-advisor".into()),
            lean_workspace,
            formal_timeout_secs,
            review_account,
            hosted,
            tex_bin: get("WISHPOOL_TEX_BIN").unwrap_or_else(|| "/usr/bin".into()),
            tex_cache: get("WISHPOOL_TEX_CACHE").unwrap_or_else(|| "/tmp/texmf-var".into()),
            texmf_home: get("WISHPOOL_TEXMF_HOME"),
            compile_timeout_secs: get("WISHPOOL_COMPILE_TIMEOUT_SECS")
                .map(|v| v.parse())
                .transpose()
                .context("WISHPOOL_COMPILE_TIMEOUT_SECS must be seconds")?
                .unwrap_or(180),
            openalex_url: get("WISHPOOL_OPENALEX_URL")
                .unwrap_or_else(|| "https://api.openalex.org".into()),
            openalex_api_key: get("WISHPOOL_OPENALEX_API_KEY"),
            role: match get("WISHPOOL_ROLE").as_deref().unwrap_or("all") {
                "api" => Role::Api,
                "worker" => Role::Worker,
                "all" => Role::All,
                other => bail!("WISHPOOL_ROLE must be api, worker or all, not {other}"),
            },
        })
    }

    /// The origin string browsers send in `Origin`, without a trailing slash.
    pub fn public_origin(&self) -> String {
        self.public_url.origin().ascii_serialization()
    }

    pub fn secure_cookies(&self) -> bool {
        self.public_url.scheme() == "https"
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn config(pairs: &[(&str, &str)]) -> anyhow::Result<Config> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Config::from_lookup(|k| map.get(k).cloned())
    }

    #[test]
    fn production_shape() {
        let c = config(&[
            ("WISHPOOL_PUBLIC_URL", "https://wishpool.example.org"),
            ("WISHPOOL_MONGODB_URI", "mongodb://db:27017"),
            ("CHRONO_NYXID_CLIENT_ID", "id"),
            ("CHRONO_NYXID_CLIENT_SECRET", "secret"),
            ("WISHPOOL_ADMIN_SUBJECTS", "a, b"),
        ])
        .unwrap();
        assert!(c.secure_cookies());
        assert_eq!(c.public_origin(), "https://wishpool.example.org");
        assert_eq!(c.bootstrap_admins.len(), 2);
        assert!(!format!("{:?}", c.auth).contains("secret\""));
        assert!(c.review_model.is_none());
    }

    #[test]
    fn dev_modes_require_loopback() {
        let base = [
            ("WISHPOOL_PUBLIC_URL", "http://127.0.0.1:5173"),
            ("WISHPOOL_AUTH_MODE", "dev"),
            ("WISHPOOL_STORAGE", "memory"),
        ];
        assert!(
            config(&base).is_err(),
            "default bind 0.0.0.0 must be refused"
        );
        let mut local = base.to_vec();
        local.push(("WISHPOOL_BIND", "127.0.0.1:8080"));
        assert!(config(&local).is_ok());
    }

    #[test]
    fn review_model_keys_go_together() {
        let base = [
            ("WISHPOOL_PUBLIC_URL", "http://127.0.0.1:5173"),
            ("WISHPOOL_AUTH_MODE", "dev"),
            ("WISHPOOL_STORAGE", "memory"),
            ("WISHPOOL_BIND", "127.0.0.1:8080"),
            (
                "WISHPOOL_REVIEW_MODEL_BASE_URL",
                "https://nyx.example/api/v1/llm/gateway/v1",
            ),
        ];
        assert!(config(&base).is_err());
    }
}

#[cfg(test)]
mod referee_tests {
    use super::*;
    use std::collections::HashMap;

    fn local(extra: &[(&str, &str)]) -> anyhow::Result<Config> {
        let mut values: HashMap<&str, String> = [
            ("WISHPOOL_BIND", "127.0.0.1:8080"),
            ("WISHPOOL_PUBLIC_URL", "http://127.0.0.1:5173"),
            ("WISHPOOL_STORAGE", "memory"),
            ("WISHPOOL_AUTH_MODE", "dev"),
        ]
        .into_iter()
        .map(|(k, v)| (k, v.into()))
        .collect();
        values.extend(extra.iter().map(|(k, v)| (*k, (*v).into())));
        Config::from_lookup(|k| values.get(k).cloned())
    }

    #[test]
    fn defaults_and_cli_settings() {
        let disabled = local(&[]).unwrap();
        assert!(disabled.oracle.is_none() && disabled.advisor.is_none());
        assert_eq!(disabled.oracle_poll_secs, 60);
        assert_eq!(disabled.advisor_timeout_secs, 1200);
        assert_eq!(disabled.audit_timeout_secs, 3600);
        assert_eq!(disabled.advisor_work_dir, "/tmp/wishpool-advisor");
        assert!(disabled.lean_workspace.is_none());
        assert_eq!(disabled.formal_timeout_secs, 1200);
        assert!(local(&[("WISHPOOL_LEAN_WORKSPACE", "/lean")]).is_err());
        assert_eq!(disabled.referee_account.as_str(), "wishpool:referee");
        assert_eq!(disabled.auditor_account.as_str(), "wishpool:auditor");
        let c = local(&[("WISHPOOL_ORACLE", "cli"), ("WISHPOOL_ADVISOR", "codex")]).unwrap();
        assert!(
            matches!(c.oracle, Some(OracleConfig::Cli { program, pools, model }) if program == "nyxid" && pools == ["chrono-chatgpt-pro-pool"] && model == "chatgpt-6-pro")
        );
        assert!(
            matches!(c.advisor, Some(AdvisorConfig::Codex { program, model: None }) if program == "codex")
        );
        let c = local(&[
            ("WISHPOOL_ORACLE", "cli"),
            ("WISHPOOL_ORACLE_CLI", "/local/nyxid"),
            ("WISHPOOL_ORACLE_POOL", " mine, , other "),
            ("WISHPOOL_ORACLE_MODEL", "pro-label"),
            ("WISHPOOL_ORACLE_POLL_SECS", "12"),
            ("WISHPOOL_REFEREE_ACCOUNT", "referee"),
            ("WISHPOOL_ADVISOR", "codex"),
            ("WISHPOOL_CODEX_BIN", "/local/codex"),
            ("WISHPOOL_CODEX_MODEL", "m"),
            ("WISHPOOL_ADVISOR_TIMEOUT_SECS", "42"),
            ("WISHPOOL_AUDIT_TIMEOUT_SECS", "3601"),
            ("WISHPOOL_ADVISOR_WORK_DIR", "/tmp/custom"),
            ("WISHPOOL_LEAN_WORKSPACE", "/lean"),
            ("WISHPOOL_FORMAL_TIMEOUT_SECS", "99"),
        ])
        .unwrap();
        assert_eq!(c.oracle_poll_secs, 12);
        assert_eq!(c.advisor_timeout_secs, 42);
        assert_eq!(c.audit_timeout_secs, 3601);
        assert_eq!(c.referee_account.as_str(), "referee");
        assert_eq!(c.advisor_work_dir, "/tmp/custom");
        assert_eq!(c.lean_workspace.as_deref(), Some("/lean"));
        assert_eq!(c.formal_timeout_secs, 99);
        assert!(
            matches!(c.oracle, Some(OracleConfig::Cli { program, pools, model }) if program == "/local/nyxid" && pools == ["mine", "other"] && model == "pro-label")
        );
        assert!(
            matches!(c.advisor, Some(AdvisorConfig::Codex { program, model }) if program == "/local/codex" && model.as_deref() == Some("m"))
        );
    }

    #[test]
    fn timeout_caps_allow_heartbeat_extended_steps() {
        for (key, maximum) in [
            ("WISHPOOL_ADVISOR_TIMEOUT_SECS", "3600"),
            ("WISHPOOL_AUDIT_TIMEOUT_SECS", "7200"),
            ("WISHPOOL_FORMAL_TIMEOUT_SECS", "3600"),
        ] {
            assert!(
                local(&[(key, maximum)]).is_ok(),
                "{key} accepts its maximum"
            );
            for value in ["0", "not-a-number", "18446744073709551615"] {
                assert!(local(&[(key, value)]).is_err(), "{key} rejects {value}");
            }
        }
        assert!(local(&[("WISHPOOL_ADVISOR_TIMEOUT_SECS", "3601")]).is_err());
        assert!(local(&[("WISHPOOL_AUDIT_TIMEOUT_SECS", "7201")]).is_err());
        assert!(local(&[("WISHPOOL_FORMAL_TIMEOUT_SECS", "3601")]).is_err());
    }

    #[test]
    fn http_token_and_chat_endpoint_requirements() {
        let token = crate::auth::random_token();
        assert!(local(&[("WISHPOOL_ORACLE", "http")]).is_err());
        let c = local(&[
            ("WISHPOOL_ORACLE", "http"),
            ("WISHPOOL_ORACLE_TOKEN", &token),
            ("CHRONO_NYXID_BASE_URL", "https://oracle.example"),
        ])
        .unwrap();
        assert!(
            matches!(&c.oracle, Some(OracleConfig::Http { base_url, pools, model, .. }) if base_url == "https://nyx-api.chrono-ai.fun/api/v1/proxy/s/oracle" && pools == &["chrono-chatgpt-pro-pool"] && model == "chatgpt-6-pro")
        );
        assert!(!format!("{:?}", c.oracle).contains(&token));
        let c = local(&[
            ("WISHPOOL_ORACLE", "http"),
            ("WISHPOOL_ORACLE_TOKEN", &token),
            ("CHRONO_NYXID_BASE_URL", "https://fallback.example"),
            ("WISHPOOL_ORACLE_BASE_URL", "https://chosen.example"),
        ])
        .unwrap();
        assert!(
            matches!(c.oracle, Some(OracleConfig::Http { base_url, .. }) if base_url == "https://chosen.example")
        );
        assert!(local(&[("WISHPOOL_ADVISOR", "chat")]).is_err());
        let c = local(&[
            ("WISHPOOL_ADVISOR", "chat"),
            (
                "WISHPOOL_REVIEW_MODEL_BASE_URL",
                "https://gateway.example/v1",
            ),
            ("WISHPOOL_REVIEW_MODEL_TOKEN", &token),
        ])
        .unwrap();
        assert!(matches!(c.advisor, Some(AdvisorConfig::Chat { model }) if model == "gpt-5.5"));
    }

    #[test]
    fn cli_rejected_on_production_bind_and_bad_settings() {
        let token = crate::auth::random_token();
        for (key, backend) in [("WISHPOOL_ORACLE", "cli"), ("WISHPOOL_ADVISOR", "codex")] {
            let values = HashMap::from([
                ("WISHPOOL_BIND", "0.0.0.0:8080"),
                ("WISHPOOL_PUBLIC_URL", "https://wishpool.example"),
                ("WISHPOOL_MONGODB_URI", "mongodb://db:27017"),
                ("CHRONO_NYXID_CLIENT_ID", "test-client"),
                ("CHRONO_NYXID_CLIENT_SECRET", token.as_str()),
                (key, backend),
            ]);
            let error = Config::from_lookup(|k| values.get(k).map(|v| v.to_string())).unwrap_err();
            assert!(error.to_string().contains("loopback"));
        }
        assert!(
            local(&[
                ("WISHPOOL_ORACLE", "cli"),
                ("WISHPOOL_REFEREE_ACCOUNT", "shared"),
                ("WISHPOOL_REVIEW_ACCOUNT", "shared")
            ])
            .is_err()
        );
        for setting in [
            ("WISHPOOL_ORACLE", "unknown"),
            ("WISHPOOL_ADVISOR", "unknown"),
            ("WISHPOOL_ORACLE_POLL_SECS", "0"),
            ("WISHPOOL_ADVISOR_TIMEOUT_SECS", "not-a-number"),
        ] {
            assert!(local(&[setting]).is_err());
        }
    }

    #[test]
    fn all_configuration_keys_are_declared_in_infra() {
        let config = include_str!("config.rs");
        let infra = format!(
            "{}\n{}",
            include_str!("../../../../infra/api/configmap.yaml"),
            include_str!("../../../../infra/api/secret.yaml")
        );
        for part in config
            .split('"')
            .filter(|s| s.starts_with("WISHPOOL_") || s.starts_with("CHRONO_"))
        {
            if part != "WISHPOOL_"
                && part != "CHRONO_"
                && part.chars().all(|c| c.is_ascii_uppercase() || c == '_')
            {
                assert!(
                    infra.contains(&format!("{part}:")),
                    "missing infra key {part}"
                );
            }
        }
    }
}

#[cfg(test)]
mod audit_config_tests {
    use super::*;
    #[test]
    fn audit_identity_is_separate_and_advisor_timeout_is_capped() {
        let base = [
            ("WISHPOOL_PUBLIC_URL", "http://127.0.0.1:5173"),
            ("WISHPOOL_BIND", "127.0.0.1:8080"),
            ("WISHPOOL_STORAGE", "memory"),
            ("WISHPOOL_AUTH_MODE", "dev"),
        ];
        for (key, value) in [
            ("WISHPOOL_AUDITOR_ACCOUNT", "wishpool:referee"),
            ("WISHPOOL_AUDITOR_ACCOUNT", "wishpool:review-engine"),
            ("WISHPOOL_ADVISOR_TIMEOUT_SECS", "3601"),
        ] {
            let values: std::collections::BTreeMap<_, _> =
                base.into_iter().chain([(key, value)]).collect();
            assert!(Config::from_lookup(|key| values.get(key).map(|v| v.to_string())).is_err());
        }
    }
}
