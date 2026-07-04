#![forbid(unsafe_code)]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, net::IpAddr};
use thiserror::Error;

pub const POOL_PROFILE_SCHEMA: &str = "rigos.pool-profile/v1";

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct PoolProfile {
    pub id: PoolProfileId,
    pub display_name: String,
    pub endpoints: Vec<PoolEndpoint>,
    pub authentication: PoolAuthentication,
    pub algorithm: AlgorithmSelection,
    pub tls: TlsPolicy,
    pub backend: MinerBackendId,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct PoolProfileId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct PoolEndpoint {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum PoolAuthentication {
    MiningIdentity {
        identity: String,
        credential_ref: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "mode", content = "algorithm")]
pub enum AlgorithmSelection {
    Auto,
    Explicit(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TlsPolicy {
    Disabled,
    Required,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MinerBackendId {
    Xmrig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolTemplate {
    pub id: &'static str,
    pub display_name: &'static str,
    pub backend: MinerBackendId,
}

pub const BUILT_IN_TEMPLATES: &[PoolTemplate] = &[
    PoolTemplate {
        id: "moneroocean",
        display_name: "MoneroOcean",
        backend: MinerBackendId::Xmrig,
    },
    PoolTemplate {
        id: "2miners",
        display_name: "2Miners",
        backend: MinerBackendId::Xmrig,
    },
    PoolTemplate {
        id: "nicehash-stratum",
        display_name: "NiceHash Stratum",
        backend: MinerBackendId::Xmrig,
    },
    PoolTemplate {
        id: "supportxmr",
        display_name: "SupportXMR",
        backend: MinerBackendId::Xmrig,
    },
    PoolTemplate {
        id: "herominers",
        display_name: "HeroMiners",
        backend: MinerBackendId::Xmrig,
    },
    PoolTemplate {
        id: "hashvault",
        display_name: "HashVault",
        backend: MinerBackendId::Xmrig,
    },
    PoolTemplate {
        id: "nanopool",
        display_name: "Nanopool",
        backend: MinerBackendId::Xmrig,
    },
    PoolTemplate {
        id: "custom-stratum",
        display_name: "Custom Stratum",
        backend: MinerBackendId::Xmrig,
    },
];

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PoolProfileError {
    #[error("profile ID must contain lowercase ASCII letters, digits, or hyphens")]
    InvalidId,
    #[error("display name is empty")]
    EmptyDisplayName,
    #[error("at least one endpoint is required")]
    MissingEndpoint,
    #[error("endpoint is malformed")]
    InvalidEndpoint,
    #[error("duplicate endpoint changes failover semantics")]
    DuplicateEndpoint,
    #[error("mining identity is empty or contains whitespace")]
    InvalidAuthentication,
    #[error("algorithm is unsupported by the selected backend")]
    UnsupportedAlgorithm,
}

pub struct BackendCapabilities {
    pub algorithms: BTreeSet<String>,
}

pub fn validate_profile(
    profile: &PoolProfile,
    capabilities: &BackendCapabilities,
) -> Result<(), PoolProfileError> {
    if profile.id.0.is_empty()
        || !profile
            .id
            .0
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(PoolProfileError::InvalidId);
    }
    if profile.display_name.trim().is_empty() {
        return Err(PoolProfileError::EmptyDisplayName);
    }
    if profile.endpoints.is_empty() {
        return Err(PoolProfileError::MissingEndpoint);
    }
    let mut endpoints = BTreeSet::new();
    for endpoint in &profile.endpoints {
        if !valid_host(&endpoint.host) || endpoint.port == 0 {
            return Err(PoolProfileError::InvalidEndpoint);
        }
        if !endpoints.insert((endpoint.host.to_ascii_lowercase(), endpoint.port)) {
            return Err(PoolProfileError::DuplicateEndpoint);
        }
    }
    let PoolAuthentication::MiningIdentity { identity, .. } = &profile.authentication;
    if identity.is_empty() || identity.chars().any(char::is_whitespace) {
        return Err(PoolProfileError::InvalidAuthentication);
    }
    if let AlgorithmSelection::Explicit(algorithm) = &profile.algorithm {
        if !capabilities.algorithms.contains(algorithm) {
            return Err(PoolProfileError::UnsupportedAlgorithm);
        }
    }
    Ok(())
}

fn valid_host(host: &str) -> bool {
    if host.parse::<IpAddr>().is_ok() {
        return true;
    }
    if host.is_empty() || host.contains(['/', '@', ':']) || host.chars().any(char::is_whitespace) {
        return false;
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(host: &str) -> PoolProfile {
        PoolProfile {
            id: PoolProfileId("custom".into()),
            display_name: "Custom".into(),
            endpoints: vec![PoolEndpoint {
                host: host.into(),
                port: 3333,
            }],
            authentication: PoolAuthentication::MiningIdentity {
                identity: "wallet.worker".into(),
                credential_ref: None,
            },
            algorithm: AlgorithmSelection::Auto,
            tls: TlsPolicy::Required,
            backend: MinerBackendId::Xmrig,
        }
    }

    #[test]
    fn arbitrary_compatible_endpoint_is_not_whitelisted() {
        assert!(
            validate_profile(
                &profile("pool.example.net"),
                &BackendCapabilities {
                    algorithms: BTreeSet::new()
                }
            )
            .is_ok()
        );
    }

    #[test]
    fn url_userinfo_and_schemes_are_rejected() {
        assert_eq!(
            validate_profile(
                &profile("stratum+tcp://wallet@pool.example"),
                &BackendCapabilities {
                    algorithms: BTreeSet::new()
                }
            ),
            Err(PoolProfileError::InvalidEndpoint)
        );
    }

    #[test]
    fn unsupported_algorithm_is_explicit() {
        let mut value = profile("pool.example.net");
        value.algorithm = AlgorithmSelection::Explicit("unknown/9".into());
        assert_eq!(
            validate_profile(
                &value,
                &BackendCapabilities {
                    algorithms: BTreeSet::new()
                }
            ),
            Err(PoolProfileError::UnsupportedAlgorithm)
        );
    }

    #[test]
    fn templates_are_convenience_not_validation_rules() {
        assert!(BUILT_IN_TEMPLATES.iter().any(|v| v.id == "moneroocean"));
        assert!(BUILT_IN_TEMPLATES.iter().any(|v| v.id == "custom-stratum"));
        assert!(
            validate_profile(
                &profile("not-in-any-template.invalid"),
                &BackendCapabilities {
                    algorithms: BTreeSet::new()
                }
            )
            .is_ok()
        );
    }
}
