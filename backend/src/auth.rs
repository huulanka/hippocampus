//! Cloudflare Access verification.
//!
//! The backend sits behind a Cloudflare Tunnel with a Zero Trust Access
//! application in front of it. Access authenticates the user and injects a
//! signed JWT into every request it forwards. That signature is the only
//! thing distinguishing a request that came through Access from one that
//! reached the origin some other way — so it has to be checked here, on
//! the origin, rather than trusted because it arrived.
//!
//! Verification is skipped entirely when `CF_ACCESS_AUD` is unset, which is
//! how local development runs. That is a deliberate hole, and the startup
//! log says so out loud rather than letting it pass unnoticed.

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use jsonwebtoken::jwk::{Jwk, JwkSet};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::error::AppError;

/// The header Cloudflare Access puts the token in.
const TOKEN_HEADER: &str = "cf-access-jwt-assertion";

/// How long a fetched key set is trusted before it is refreshed anyway.
const KEYS_MAX_AGE: Duration = Duration::from_secs(60 * 60);

/// Floor between two fetches, so a stream of requests carrying unknown key
/// ids cannot turn this service into a load generator against Cloudflare.
const KEYS_MIN_REFETCH: Duration = Duration::from_secs(60);

/// What we read out of a verified token. Everything else Access puts in
/// there is ignored: this service has exactly one user, so identity is for
/// the log, not for authorisation.
#[derive(Debug, Deserialize)]
pub struct AccessClaims {
    #[serde(default)]
    pub email: Option<String>,
}

struct CachedKeys {
    set: JwkSet,
    fetched_at: Instant,
}

pub struct AccessVerifier {
    aud: String,
    issuer: String,
    certs_url: String,
    http: reqwest::Client,
    keys: RwLock<Option<CachedKeys>>,
}

impl AccessVerifier {
    /// `team_domain` is the Zero Trust team name or its full hostname —
    /// both `acme` and `acme.cloudflareaccess.com` work, because getting
    /// this wrong would fail closed at the first request rather than at
    /// startup, which is a bad time to find out.
    pub fn new(team_domain: &str, aud: String) -> Self {
        let host = normalise_team_domain(team_domain);
        Self {
            aud,
            issuer: format!("https://{host}"),
            certs_url: format!("https://{host}/cdn-cgi/access/certs"),
            http: reqwest::Client::new(),
            keys: RwLock::new(None),
        }
    }

    fn validation(&self) -> Validation {
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&[&self.aud]);
        validation.set_issuer(&[&self.issuer]);
        validation.validate_exp = true;
        validation.validate_nbf = true;
        validation
    }

    pub async fn verify(&self, token: &str) -> anyhow::Result<AccessClaims> {
        let header = decode_header(token)?;
        let kid = header
            .kid
            .ok_or_else(|| anyhow::anyhow!("token has no key id"))?;

        let key = self.key_for(&kid).await?;
        let decoded =
            decode::<AccessClaims>(token, &DecodingKey::from_jwk(&key)?, &self.validation())?;

        Ok(decoded.claims)
    }

    /// Finds the signing key, refreshing the cached set when the key id is
    /// unknown — which is what a key rotation looks like from here.
    async fn key_for(&self, kid: &str) -> anyhow::Result<Jwk> {
        {
            let cached = self.keys.read().await;
            if let Some(cached) = cached.as_ref()
                && cached.fetched_at.elapsed() < KEYS_MAX_AGE
                && let Some(key) = cached.set.find(kid)
            {
                return Ok(key.clone());
            }
        }

        let mut cached = self.keys.write().await;

        // Another request may have refreshed while this one waited, and a
        // very recent fetch is not worth repeating for a key id that was
        // already missing from it.
        if let Some(existing) = cached.as_ref()
            && existing.fetched_at.elapsed() < KEYS_MIN_REFETCH
        {
            return existing
                .set
                .find(kid)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("token signed by an unknown key"));
        }

        let set = self.fetch_keys().await?;
        let found = set.find(kid).cloned();
        *cached = Some(CachedKeys {
            set,
            fetched_at: Instant::now(),
        });

        found.ok_or_else(|| anyhow::anyhow!("token signed by an unknown key"))
    }

    async fn fetch_keys(&self) -> anyhow::Result<JwkSet> {
        let response = self.http.get(&self.certs_url).send().await?;
        if !response.status().is_success() {
            anyhow::bail!("Cloudflare returned {} for the key set", response.status());
        }
        Ok(response.json::<JwkSet>().await?)
    }

    /// Pre-seeded verifier, for tests: no network, keys already present.
    #[cfg(test)]
    fn with_keys(aud: &str, issuer: &str, set: JwkSet) -> Self {
        Self {
            aud: aud.to_string(),
            issuer: issuer.to_string(),
            certs_url: "http://127.0.0.1:1/unused".to_string(),
            http: reqwest::Client::new(),
            keys: RwLock::new(Some(CachedKeys {
                set,
                fetched_at: Instant::now(),
            })),
        }
    }
}

/// Accepts a bare team name or a full hostname, so both spellings of the
/// same thing in `.env` behave identically.
fn normalise_team_domain(team_domain: &str) -> String {
    let trimmed = team_domain
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');

    if trimmed.contains('.') {
        trimmed.to_string()
    } else {
        format!("{trimmed}.cloudflareaccess.com")
    }
}

/// Rejects anything that did not arrive through Cloudflare Access.
pub async fn require_access(
    State(verifier): State<Arc<AccessVerifier>>,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let token = request
        .headers()
        .get(TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| AppError::unauthorized("no Cloudflare Access token on the request"))?;

    let claims = verifier
        .verify(token)
        .await
        .map_err(|err| AppError::unauthorized(format!("token rejected: {err}")))?;

    tracing::debug!(email = ?claims.email, "request authorised by Cloudflare Access");
    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use jsonwebtoken::{EncodingKey, Header};
    use rsa::pkcs1::EncodeRsaPrivateKey;
    use rsa::traits::PublicKeyParts;
    use serde_json::json;

    const AUD: &str = "test-audience";
    const ISSUER: &str = "https://acme.cloudflareaccess.com";
    const KID: &str = "test-key";

    struct Signer {
        key: EncodingKey,
        jwks: JwkSet,
    }

    /// Generates a throwaway key pair and the key set a verifier would
    /// have fetched for it. 2048 bits is the smallest size Cloudflare
    /// actually uses and keeps the tests quick.
    fn signer(kid: &str) -> Signer {
        let private = rsa::RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).unwrap();
        let public = private.to_public_key();

        let der = private.to_pkcs1_der().unwrap();
        let key = EncodingKey::from_rsa_der(der.as_bytes());

        let jwks: JwkSet = serde_json::from_value(json!({
            "keys": [{
                "kty": "RSA",
                "alg": "RS256",
                "use": "sig",
                "kid": kid,
                "n": URL_SAFE_NO_PAD.encode(public.n().to_bytes_be()),
                "e": URL_SAFE_NO_PAD.encode(public.e().to_bytes_be()),
            }]
        }))
        .unwrap();

        Signer { key, jwks }
    }

    fn token(signer: &Signer, kid: &str, aud: &str, issuer: &str, expires_in: i64) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(kid.to_string());

        let claims = json!({
            "aud": [aud],
            "iss": issuer,
            "email": "someone@example.com",
            "exp": chrono::Utc::now().timestamp() + expires_in,
            "iat": chrono::Utc::now().timestamp() - 10,
        });

        jsonwebtoken::encode(&header, &claims, &signer.key).unwrap()
    }

    #[tokio::test]
    async fn a_properly_signed_token_is_accepted() {
        let signer = signer(KID);
        let verifier = AccessVerifier::with_keys(AUD, ISSUER, signer.jwks.clone());
        let claims = verifier
            .verify(&token(&signer, KID, AUD, ISSUER, 300))
            .await
            .unwrap();
        assert_eq!(claims.email.as_deref(), Some("someone@example.com"));
    }

    #[tokio::test]
    async fn a_token_for_a_different_application_is_rejected() {
        let signer = signer(KID);
        let verifier = AccessVerifier::with_keys(AUD, ISSUER, signer.jwks.clone());
        // The whole point of `aud`: another Access application in the same
        // account produces valid signatures over tokens meant for it.
        assert!(
            verifier
                .verify(&token(&signer, KID, "someone-elses-app", ISSUER, 300))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn a_token_from_a_different_team_is_rejected() {
        let signer = signer(KID);
        let verifier = AccessVerifier::with_keys(AUD, ISSUER, signer.jwks.clone());
        assert!(
            verifier
                .verify(&token(
                    &signer,
                    KID,
                    AUD,
                    "https://someone-else.cloudflareaccess.com",
                    300
                ))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn an_expired_token_is_rejected() {
        let signer = signer(KID);
        let verifier = AccessVerifier::with_keys(AUD, ISSUER, signer.jwks.clone());
        assert!(
            verifier
                .verify(&token(&signer, KID, AUD, ISSUER, -300))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn a_token_signed_by_someone_else_is_rejected() {
        let genuine = signer(KID);
        let forger = signer(KID);
        let verifier = AccessVerifier::with_keys(AUD, ISSUER, genuine.jwks.clone());
        assert!(
            verifier
                .verify(&token(&forger, KID, AUD, ISSUER, 300))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn an_unsigned_token_is_rejected() {
        let signer = signer(KID);
        let verifier = AccessVerifier::with_keys(AUD, ISSUER, signer.jwks.clone());

        // alg=none, the classic: valid-looking claims, no signature.
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none","kid":"test-key"}"#);
        let claims = URL_SAFE_NO_PAD.encode(
            json!({ "aud": [AUD], "iss": ISSUER, "exp": chrono::Utc::now().timestamp() + 300 })
                .to_string(),
        );
        assert!(
            verifier
                .verify(&format!("{header}.{claims}."))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn nonsense_is_rejected_without_reaching_the_network() {
        let signer = signer(KID);
        let verifier = AccessVerifier::with_keys(AUD, ISSUER, signer.jwks);
        assert!(verifier.verify("not-a-token").await.is_err());
    }

    #[test]
    fn a_team_name_and_its_hostname_mean_the_same_thing() {
        assert_eq!(
            normalise_team_domain("acme"),
            "acme.cloudflareaccess.com".to_string()
        );
        assert_eq!(
            normalise_team_domain("https://acme.cloudflareaccess.com/"),
            "acme.cloudflareaccess.com".to_string()
        );
    }
}
