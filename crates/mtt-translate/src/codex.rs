//! ChatGPT (Codex) sign-in inside the app, replacing the `openai-oauth` proxy: OpenAI's
//! device flow (the user approves a code in the browser), tokens kept in the data folder
//! like the Codex CLI's `auth.json` (owner-only on Unix) and refreshed before they expire,
//! and the headers the Codex Responses endpoint wants. koharu signs in the same way.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::client::{Retry, SendError};

/// OpenAI's sign-in server and the Codex CLI's public OAuth client.
pub const AUTH_BASE: &str = "https://auth.openai.com";
pub const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
/// The device flow's fixed redirect (no local callback server is involved).
const DEVICE_REDIRECT_URI: &str = "https://auth.openai.com/deviceauth/callback";
/// Codex Responses API (`{CODEX_BASE_URL}/responses`).
pub const CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";
/// How long a device code stays valid, and how close to expiry tokens are refreshed.
const DEVICE_LOGIN_TTL: Duration = Duration::from_secs(15 * 60);
const REFRESH_MARGIN_MS: u64 = 60_000;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const AUTH_CLAIM: &str = "https://api.openai.com/auth";
const PROFILE_CLAIM: &str = "https://api.openai.com/profile";

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Tokens {
    access_token: String,
    refresh_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id_token: Option<String>,
    expires_at_ms: u64,
}

/// The signed-in ChatGPT account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub id: String,
    pub email: Option<String>,
    pub plan: Option<String>,
}

/// What a request needs: the access token and the account it belongs to.
#[derive(Clone, PartialEq, Eq)]
pub struct Session {
    pub access_token: String,
    pub account_id: String,
}

/// A code the user enters at `verification_url`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCode {
    pub user_code: String,
    pub verification_url: String,
    device_auth_id: String,
    interval: Duration,
}

/// The sign-in state stored at one path. Refreshes of a path are serialized across the
/// process (see [`refresh_lock`]) so parallel requests don't each spend the (single-use)
/// refresh token.
#[derive(Debug)]
pub struct CodexAuth {
    path: PathBuf,
    auth_base: String,
    refresh: Arc<Mutex<()>>,
}

/// The lock every [`CodexAuth`] for `path` refreshes under: the translation job and the
/// settings' model list each make their own, and must not both spend the refresh token.
fn refresh_lock(path: &Path) -> Arc<Mutex<()>> {
    static LOCKS: LazyLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = LazyLock::new(Default::default);
    let mut locks = LOCKS.lock().unwrap_or_else(PoisonError::into_inner);
    Arc::clone(locks.entry(path.to_path_buf()).or_default())
}

impl std::fmt::Debug for Tokens {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Tokens").field("expires_at_ms", &self.expires_at_ms).finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Session {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Session").field("account_id", &self.account_id).finish_non_exhaustive()
    }
}

impl CodexAuth {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self::with_auth_base(path, AUTH_BASE)
    }

    /// Talks to another sign-in server (tests).
    pub fn with_auth_base(path: impl Into<PathBuf>, auth_base: &str) -> Self {
        let path = path.into();
        Self { refresh: refresh_lock(&path), path, auth_base: auth_base.trim_end_matches('/').to_owned() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The signed-in account, `None` when signed out.
    pub fn account(&self) -> Option<Account> {
        self.load().and_then(|tokens| account(&tokens).ok())
    }

    /// A session with at least a minute left, refreshing the tokens if needed. Errors are
    /// [`Retry::Stop`] when only signing in again helps, [`Retry::Later`] when the sign-in
    /// server could not be reached.
    pub fn session(&self) -> Result<Session, SendError> {
        let tokens = self.load().ok_or_else(|| stop(NOT_SIGNED_IN))?;
        if tokens.expires_at_ms > now_ms() + REFRESH_MARGIN_MS {
            return session(&tokens).map_err(stop);
        }
        let _refresh = self.refresh.lock().unwrap_or_else(PoisonError::into_inner);
        // Another request may have refreshed meanwhile.
        let current = self.load().ok_or_else(|| stop(NOT_SIGNED_IN))?;
        if current.expires_at_ms > now_ms() + REFRESH_MARGIN_MS {
            return session(&current).map_err(stop);
        }
        self.refresh_tokens(&current)
    }

    /// Refreshes even if the tokens look valid (the server answered 401). Skipped when
    /// another request already refreshed past `stale`.
    pub fn force_refresh(&self, stale: &Session) -> Result<Session, SendError> {
        let _refresh = self.refresh.lock().unwrap_or_else(PoisonError::into_inner);
        let current = self.load().ok_or_else(|| stop(NOT_SIGNED_IN))?;
        if current.access_token != stale.access_token {
            return session(&current).map_err(stop);
        }
        self.refresh_tokens(&current)
    }

    fn refresh_tokens(&self, current: &Tokens) -> Result<Session, SendError> {
        let mut response = agent()
            .post(&format!("{}/oauth/token", self.auth_base))
            .send_form([
                ("grant_type", "refresh_token"),
                ("refresh_token", current.refresh_token.as_str()),
                ("client_id", CLIENT_ID),
            ])
            .map_err(|error| SendError::new(format!("ChatGPT 로그인 갱신 요청 실패: {error}"), Retry::Later(None)))?;
        let status = response.status().as_u16();
        if status == 401 {
            // The refresh token was spent or revoked. If the file has other tokens by now,
            // someone else refreshed them; only the rejected ones are signed out.
            match self.load() {
                Some(stored) if stored.refresh_token != current.refresh_token => {
                    return session(&stored).map_err(stop);
                }
                Some(_) => {
                    let _ = std::fs::remove_file(&self.path);
                }
                None => {}
            }
            return Err(stop("ChatGPT 로그인이 만료되었습니다. 설정 › 번역에서 다시 로그인하세요."));
        }
        let value = read_success(&mut response, "로그인 갱신").map_err(|error| match status {
            408 | 429 | 500..=599 => SendError::new(error, Retry::Later(None)),
            _ => stop(error),
        })?;
        let tokens = tokens_from(value, Some(current)).map_err(stop)?;
        self.store(&tokens).map_err(stop)?;
        session(&tokens).map_err(stop)
    }

    /// Asks for a device code; show it and open [`DeviceCode::verification_url`].
    pub fn start_device_login(&self) -> Result<DeviceCode, String> {
        let mut response = agent()
            .post(&format!("{}/api/accounts/deviceauth/usercode", self.auth_base))
            .send_json(serde_json::json!({ "client_id": CLIENT_ID }))
            .map_err(|error| format!("ChatGPT 로그인 코드를 받지 못했습니다: {error}"))?;
        let value = read_success(&mut response, "로그인 코드 요청")?;
        let interval = value
            .get("interval")
            .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok()))
            .unwrap_or(5)
            .max(1);
        Ok(DeviceCode {
            user_code: string(&value, "user_code").or_else(|_| string(&value, "usercode"))?,
            verification_url: format!("{}/codex/device", self.auth_base),
            device_auth_id: string(&value, "device_auth_id")?,
            interval: Duration::from_secs(interval),
        })
    }

    /// Waits until the user approves `device` in the browser, then stores the tokens.
    /// `cancelled` stops the wait.
    pub fn finish_device_login(&self, device: &DeviceCode, cancelled: &dyn Fn() -> bool) -> Result<Account, String> {
        let started = Instant::now();
        let (code, verifier) = loop {
            if cancelled() {
                return Err("로그인을 취소했습니다.".into());
            }
            if started.elapsed() >= DEVICE_LOGIN_TTL {
                return Err("로그인 코드가 만료되었습니다. 다시 시도하세요.".into());
            }
            let mut response = agent()
                .post(&format!("{}/api/accounts/deviceauth/token", self.auth_base))
                .send_json(
                    serde_json::json!({ "device_auth_id": device.device_auth_id, "user_code": device.user_code }),
                )
                .map_err(|error| format!("ChatGPT 로그인 확인 실패: {error}"))?;
            // Not approved yet.
            if matches!(response.status().as_u16(), 403 | 404) {
                let until = Instant::now() + device.interval;
                while Instant::now() < until {
                    if cancelled() {
                        return Err("로그인을 취소했습니다.".into());
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                continue;
            }
            let value = read_success(&mut response, "로그인 확인")?;
            break (string(&value, "authorization_code")?, string(&value, "code_verifier")?);
        };
        let mut response = agent()
            .post(&format!("{}/oauth/token", self.auth_base))
            .send_form([
                ("grant_type", "authorization_code"),
                ("client_id", CLIENT_ID),
                ("code", code.as_str()),
                ("code_verifier", verifier.as_str()),
                ("redirect_uri", DEVICE_REDIRECT_URI),
            ])
            .map_err(|error| format!("ChatGPT 로그인 완료 요청 실패: {error}"))?;
        let tokens = tokens_from(read_success(&mut response, "로그인 완료")?, None)?;
        let account = account(&tokens)?;
        self.store(&tokens)?;
        Ok(account)
    }

    pub fn logout(&self) -> Result<(), String> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("로그인 정보를 지우지 못했습니다: {error}")),
        }
    }

    fn load(&self) -> Option<Tokens> {
        serde_json::from_slice(&std::fs::read(&self.path).ok()?).ok()
    }

    /// Writes the tokens through a temporary file readable only by the owner.
    fn store(&self, tokens: &Tokens) -> Result<(), String> {
        use std::io::Write;
        let bytes = serde_json::to_vec_pretty(tokens).map_err(|error| error.to_string())?;
        let temp = self.path.with_extension("json.tmp");
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&temp).map_err(|error| format!("로그인 정보를 저장하지 못했습니다: {error}"))?;
        file.write_all(&bytes).and_then(|()| file.sync_all()).map_err(|error| error.to_string())?;
        std::fs::rename(&temp, &self.path).map_err(|error| format!("로그인 정보를 저장하지 못했습니다: {error}"))
    }
}

const NOT_SIGNED_IN: &str = "ChatGPT 계정에 로그인하지 않았습니다. 설정 › 번역에서 ChatGPT 로그인을 하세요.";

/// A sign-in error only signing in again fixes.
fn stop(message: impl Into<String>) -> SendError {
    SendError::new(message, Retry::Stop)
}

/// Headers naming `session`'s account on any ChatGPT backend request.
pub fn account_headers(session: &Session) -> [(&'static str, String); 3] {
    [
        ("Authorization", format!("Bearer {}", session.access_token)),
        ("chatgpt-account-id", session.account_id.clone()),
        ("originator", "manga_translation_tools".into()),
    ]
}

/// Headers for a Codex request with `session`.
pub fn request_headers(session: &Session) -> [(&'static str, String); 5] {
    let [authorization, account, originator] = account_headers(session);
    [
        authorization,
        account,
        originator,
        ("OpenAI-Beta", "responses=experimental".into()),
        ("Accept", "text/event-stream".into()),
    ]
}

fn agent() -> ureq::Agent {
    ureq::Agent::new_with_config(
        ureq::Agent::config_builder().http_status_as_error(false).timeout_global(Some(REQUEST_TIMEOUT)).build(),
    )
}

fn read_success(response: &mut ureq::http::Response<ureq::Body>, what: &str) -> Result<Value, String> {
    let status = response.status();
    let text = response.body_mut().read_to_string().unwrap_or_default();
    if !status.is_success() {
        return Err(format!("ChatGPT {what} 실패 ({}): {}", status.as_u16(), crate::client::preview(&text, 300)));
    }
    serde_json::from_str(&text).map_err(|error| format!("ChatGPT {what} 응답을 읽지 못했습니다: {error}"))
}

/// Tokens from a token response. A refresh may leave out the refresh and ID tokens; the
/// `previous` ones then stay (like the Codex CLI).
fn tokens_from(value: Value, previous: Option<&Tokens>) -> Result<Tokens, String> {
    let access_token = string(&value, "access_token")?;
    let expires_at_ms = value
        .get("expires_in")
        .and_then(Value::as_u64)
        .map(|seconds| now_ms() + seconds * 1000)
        .or_else(|| jwt(&access_token)?.get("exp")?.as_u64().map(|exp| exp * 1000))
        .ok_or("로그인 응답에 만료 시각이 없습니다.")?;
    let refresh_token = match (string(&value, "refresh_token"), previous) {
        (Ok(token), _) => token,
        (Err(_), Some(previous)) => previous.refresh_token.clone(),
        (Err(error), None) => return Err(error),
    };
    let id_token = value
        .get("id_token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
        .or_else(|| previous.and_then(|previous| previous.id_token.clone()));
    Ok(Tokens { access_token, refresh_token, id_token, expires_at_ms })
}

fn session(tokens: &Tokens) -> Result<Session, String> {
    Ok(Session { access_token: tokens.access_token.clone(), account_id: account(tokens)?.id })
}

fn account(tokens: &Tokens) -> Result<Account, String> {
    let claims = jwt(&tokens.access_token)
        .or_else(|| tokens.id_token.as_deref().and_then(jwt))
        .ok_or("로그인 토큰을 읽지 못했습니다.")?;
    let auth = claims.get(AUTH_CLAIM).ok_or("로그인 토큰에 ChatGPT 계정 정보가 없습니다.")?;
    let text = |value: Option<&Value>| value.and_then(Value::as_str).map(str::to_owned);
    Ok(Account {
        id: text(auth.get("chatgpt_account_id")).ok_or("로그인 토큰에 ChatGPT 계정 ID가 없습니다.")?,
        email: text(claims.get(PROFILE_CLAIM).and_then(|profile| profile.get("email"))),
        plan: text(auth.get("chatgpt_plan_type")),
    })
}

/// A JWT's claims (the signature isn't checked; the server checks it).
fn jwt(value: &str) -> Option<Value> {
    let mut parts = value.split('.');
    let (_, payload, _, None) = (parts.next()?, parts.next()?, parts.next()?, parts.next()) else { return None };
    serde_json::from_slice(&base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload).ok()?).ok()
}

fn string(value: &Value, field: &str) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("ChatGPT 로그인 응답에 {field}가 없습니다."))
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_millis() as u64).unwrap_or(0)
}

/// A JWT with these claims (tests).
#[cfg(test)]
pub(crate) fn test_jwt(account_id: &str, email: &str) -> String {
    let encode = |value: Value| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value.to_string());
    let claims = serde_json::json!({
        AUTH_CLAIM: { "chatgpt_account_id": account_id, "chatgpt_plan_type": "plus" },
        PROFILE_CLAIM: { "email": email },
    });
    format!("{}.{}.sig", encode(serde_json::json!({ "alg": "none" })), encode(claims))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accounts_come_from_the_token_claims() {
        let tokens = Tokens {
            access_token: test_jwt("acct-1", "a@example.com"),
            refresh_token: "r".into(),
            id_token: None,
            expires_at_ms: now_ms() + 3_600_000,
        };
        assert_eq!(
            account(&tokens).unwrap(),
            Account { id: "acct-1".into(), email: Some("a@example.com".into()), plan: Some("plus".into()) }
        );
        let dir = std::env::temp_dir().join(format!("mtt-codex-{}", std::process::id()));
        let auth = CodexAuth::new(dir.join("codex-auth.json"));
        assert!(auth.account().is_none());
        let error = auth.session().unwrap_err();
        assert!(error.message.contains("로그인하지 않았습니다"));
        assert_eq!(error.retry, Retry::Stop);
        auth.store(&tokens).unwrap();
        assert_eq!(auth.session().unwrap().account_id, "acct-1", "fresh tokens are used as they are");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(auth.path()).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "owner-only");
        }
        auth.logout().unwrap();
        assert!(auth.account().is_none());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_refresh_without_new_refresh_or_id_tokens_keeps_the_old_ones() {
        let previous = Tokens {
            access_token: "old".into(),
            refresh_token: "r-1".into(),
            id_token: Some("id-1".into()),
            expires_at_ms: 0,
        };
        let value = serde_json::json!({ "access_token": test_jwt("acct-1", "a@example.com"), "expires_in": 60 });
        let tokens = tokens_from(value.clone(), Some(&previous)).unwrap();
        assert_eq!((tokens.refresh_token.as_str(), tokens.id_token.as_deref()), ("r-1", Some("id-1")));
        assert!(tokens_from(value, None).is_err(), "a first sign-in needs a refresh token");
    }
}
