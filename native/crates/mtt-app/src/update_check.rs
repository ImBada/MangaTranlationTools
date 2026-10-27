//! GitHub latest-release check (`src/server/updateCheck.ts`), cached for 30 minutes.

use std::time::{Duration, Instant};

const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/ImBada/MangaTranlationTools/releases/latest";
pub const CACHE_TTL: Duration = Duration::from_secs(30 * 60);
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateStatus {
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub release_url: Option<String>,
    pub error: Option<String>,
    pub checked_at: Instant,
}

impl UpdateStatus {
    pub fn fresh(&self) -> bool {
        self.checked_at.elapsed() < CACHE_TTL
    }
}

/// `resolveUpdateStatusText`.
pub fn status_text(status: Option<&UpdateStatus>, busy: bool) -> String {
    let Some(status) = status else {
        return if busy { "업데이트 확인 중...".into() } else { "업데이트 확인 대기 중".into() };
    };
    if let Some(error) = &status.error {
        return format!("업데이트 확인 실패: {error}");
    }
    match &status.latest_version {
        Some(latest) if status.update_available => format!("새 버전 v{latest} 사용 가능"),
        Some(_) => "최신 버전입니다.".into(),
        None => "최신 릴리스 정보를 찾지 못했습니다.".into(),
    }
}

pub fn check() -> UpdateStatus {
    let checked_at = Instant::now();
    let empty = |error: Option<String>| UpdateStatus {
        latest_version: None,
        update_available: false,
        release_url: None,
        error,
        checked_at,
    };
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder().http_status_as_error(false).timeout_global(Some(Duration::from_secs(15))).build(),
    );
    let response = agent
        .get(LATEST_RELEASE_URL)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "MangaTranslationTools")
        .call();
    let mut response = match response {
        Ok(response) => response,
        Err(error) => return empty(Some(error.to_string())),
    };
    let code = response.status().as_u16();
    if code == 404 {
        return empty(None);
    }
    if !(200..300).contains(&code) {
        return empty(Some(format!("업데이트 확인 실패 ({code})")));
    }
    let release: serde_json::Value = match response.body_mut().read_json() {
        Ok(value) => value,
        Err(error) => return empty(Some(error.to_string())),
    };
    let latest_version = release["tag_name"].as_str().map(normalize_version);
    UpdateStatus {
        update_available: latest_version.as_deref().is_some_and(|latest| compare_versions(latest, CURRENT_VERSION) > 0),
        latest_version,
        release_url: release["html_url"].as_str().map(str::to_owned),
        error: None,
        checked_at,
    }
}

fn normalize_version(version: &str) -> String {
    let trimmed = version.trim();
    trimmed.strip_prefix(['v', 'V']).unwrap_or(trimmed).to_owned()
}

/// Numeric comparison over `.`/`-` separated parts (at least three), non-numbers count as 0.
fn compare_versions(left: &str, right: &str) -> i32 {
    let parts = |value: &str| -> Vec<u64> {
        normalize_version(value).split(['.', '-']).map(|part| part.parse().unwrap_or(0)).collect()
    };
    let (left, right) = (parts(left), parts(right));
    for index in 0..left.len().max(right.len()).max(3) {
        let (l, r) = (left.get(index).copied().unwrap_or(0), right.get(index).copied().unwrap_or(0));
        if l != r {
            return if l > r { 1 } else { -1 };
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_like_ts() {
        assert_eq!(compare_versions("v0.9.32", "0.9.31"), 1);
        assert_eq!(compare_versions("0.9.31", "0.9.31"), 0);
        assert_eq!(compare_versions("0.10", "0.9.31"), 1);
        assert_eq!(compare_versions("0.9", "0.9.0"), 0);
        assert_eq!(compare_versions("1.0.0-beta", "1.0.0"), 0);
        assert_eq!(normalize_version(" v1.2.3 "), "1.2.3");
    }

    #[test]
    fn status_texts_match_ts() {
        assert_eq!(status_text(None, true), "업데이트 확인 중...");
        assert_eq!(status_text(None, false), "업데이트 확인 대기 중");
        let mut status = UpdateStatus {
            latest_version: Some("1.0.0".into()),
            update_available: true,
            release_url: None,
            error: None,
            checked_at: Instant::now(),
        };
        assert_eq!(status_text(Some(&status), false), "새 버전 v1.0.0 사용 가능");
        status.update_available = false;
        assert_eq!(status_text(Some(&status), false), "최신 버전입니다.");
        status.latest_version = None;
        assert_eq!(status_text(Some(&status), false), "최신 릴리스 정보를 찾지 못했습니다.");
    }
}
