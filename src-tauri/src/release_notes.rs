//! What's new's release notes from GitHub: the body of a version's GitHub
//! release, read from the releases API
//! (`GET https://api.github.com/repos/brinkercode/freewheel/releases/tags/v<version>`,
//! no token). The settings window asks for them only when What's new opens
//! without notes stored from the update (src/whatsNew.js), never in the
//! background, and cleans them with the release's own cut
//! (src/releaseNotes.js appNotes) before drawing the safe markdown subset.
//!
//! Like the updater, builds without the release's update key (built from
//! source, CI test builds) make no request at all. The e2e-only `e2e` cargo
//! feature lets the update e2e point the API at its local release server
//! (`FREEWHEEL_E2E_RELEASES_API`); shipped builds are built without it.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use serde::Deserialize;

use crate::selfupdate;

/// The releases API of freewheel's repo; a tag's release is `<this>/tags/v<version>`.
pub const RELEASES_API: &str = "https://api.github.com/repos/brinkercode/freewheel/releases";

/// Update e2e builds only: the API base override (the local release server,
/// which serves `/repos/brinkercode/freewheel/releases/tags/v<version>`).
#[cfg(feature = "e2e")]
pub const E2E_RELEASES_API_ENV: &str = "FREEWHEEL_E2E_RELEASES_API";

/// How long the whole request may take: What's new waits for it.
pub const TIMEOUT: Duration = Duration::from_secs(8);

/// The longest release body kept (GitHub allows 125 000 characters).
pub const MAX_BODY: usize = 256 * 1024;

/// Why there are no notes from GitHub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotesError {
    /// This build has no update key: it makes no requests.
    NotConfigured,
    /// Not a plain semver version: no URL is built from it.
    BadVersion(String),
    /// The request failed or timed out, or GitHub answered other than 200/404.
    Network(String),
    /// The answer is not a release (bad JSON, no body, too long).
    BadRelease(String),
}

impl std::fmt::Display for NotesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotesError::NotConfigured => write!(f, "not-configured: this build makes no requests"),
            NotesError::BadVersion(v) => write!(f, "bad-version: {v}"),
            NotesError::Network(e) => write!(f, "network: {e}"),
            NotesError::BadRelease(e) => write!(f, "bad-release: {e}"),
        }
    }
}

/// The API base: always [`RELEASES_API`] in shipped builds.
pub fn releases_api() -> String {
    #[cfg(feature = "e2e")]
    if let Some(base) = std::env::var(E2E_RELEASES_API_ENV)
        .ok()
        .map(|b| b.trim().trim_end_matches('/').to_string())
        .filter(|b| !b.is_empty())
    {
        return format!("{base}/repos/brinkercode/freewheel/releases");
    }
    RELEASES_API.to_string()
}

/// Shipped builds never allow the loopback server.
const LOOPBACK: bool = cfg!(feature = "e2e");

/// The API URL of `version`'s release ("1.1.0" or "v1.1.0"), or None when it
/// is not a plain semver version.
pub fn release_url(base: &str, version: &str) -> Option<String> {
    // The same check as the release page link (updates.rs): only a version
    // ever lands in the URL.
    crate::updates::release_page(version)?;
    let v = version.strip_prefix('v').unwrap_or(version);
    Some(format!("{base}/tags/v{v}"))
}

#[derive(Deserialize)]
struct Release {
    body: Option<String>,
}

/// A releases API answer -> the release's body (None when it has none).
pub fn parse_release(json: &str) -> Result<Option<String>, NotesError> {
    if json.len() > MAX_BODY * 2 {
        return Err(NotesError::BadRelease("the answer is too long".into()));
    }
    let release: Release = serde_json::from_str(json)
        .map_err(|e| NotesError::BadRelease(format!("not a release: {e}")))?;
    let body = release
        .body
        .map(|b| b.replace("\r\n", "\n"))
        .filter(|b| !b.trim().is_empty());
    match body {
        Some(b) if b.len() > MAX_BODY => {
            Err(NotesError::BadRelease("the notes are too long".into()))
        }
        b => Ok(b),
    }
}

/// GETs `url` (https only; with `loopback` also the local test server):
/// the release's body, None when there is no such release (404).
pub async fn fetch_body(
    url: &str,
    timeout: Duration,
    loopback: bool,
) -> Result<Option<String>, NotesError> {
    if !selfupdate::url_allowed(url, loopback) {
        return Err(NotesError::Network(format!("not an https URL: {url}")));
    }
    // Belt and braces: reqwest itself refuses anything but https unless this
    // is the local test server (and redirects are off, so it can't be led away).
    let client = reqwest::Client::builder()
        .https_only(!loopback)
        .user_agent(concat!("freewheel/", env!("CARGO_PKG_VERSION")))
        .timeout(timeout)
        .connect_timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| NotesError::Network(e.to_string()))?;
    let res = client
        .get(url)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|e| NotesError::Network(e.to_string()))?;
    let status = res.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if status != reqwest::StatusCode::OK {
        return Err(NotesError::Network(format!("{url}: HTTP {status}")));
    }
    let text = res
        .text()
        .await
        .map_err(|e| NotesError::Network(e.to_string()))?;
    parse_release(&text)
}

/// The fetched bodies, per version, for as long as the app runs (the
/// settings window also keeps the cleaned notes, src/whatsNew.js).
#[derive(Default)]
pub struct NotesCache(Mutex<HashMap<String, Option<String>>>);

impl NotesCache {
    fn get(&self, version: &str) -> Option<Option<String>> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(version)
            .cloned()
    }

    fn put(&self, version: &str, body: Option<String>) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(version.to_string(), body);
    }
}

/// `version`'s release body from GitHub, through `cache`. `configured`:
/// whether this build has the release's update key (no request otherwise).
pub async fn notes_for(
    cache: &NotesCache,
    version: &str,
    configured: bool,
    base: &str,
    loopback: bool,
) -> Result<Option<String>, NotesError> {
    if !configured {
        return Err(NotesError::NotConfigured);
    }
    let url = release_url(base, version).ok_or_else(|| NotesError::BadVersion(version.into()))?;
    let v = version.strip_prefix('v').unwrap_or(version);
    if let Some(hit) = cache.get(v) {
        return Ok(hit);
    }
    let body = fetch_body(&url, TIMEOUT, loopback).await?;
    cache.put(v, body.clone());
    Ok(body)
}

/// What's new: `version`'s GitHub release body (raw markdown; the window
/// cleans it), or null when the release has none.
#[tauri::command]
pub async fn release_notes(
    cache: tauri::State<'_, NotesCache>,
    version: String,
) -> Result<Option<String>, String> {
    let base = releases_api();
    let notes = notes_for(&cache, &version, selfupdate::configured(), &base, LOOPBACK).await;
    match &notes {
        Ok(body) => {
            tracing::info!(version = %version, found = body.is_some(), "release notes: fetched")
        }
        Err(NotesError::NotConfigured) => {
            tracing::info!("release notes: this build makes no requests")
        }
        Err(e) => tracing::warn!(version = %version, error = %e, "release notes: not fetched"),
    }
    notes.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// The releases API answer the tests parse: GitHub's shape, with the
    /// shared fixture's body (scripts/fixtures/release-notes/github-release.md;
    /// src/releaseNotes.test.js checks what the app makes of it).
    const FIXTURE_BODY: &str =
        include_str!("../../scripts/fixtures/release-notes/github-release.md");

    fn api_json(body: &str) -> String {
        serde_json::json!({
            "url": "https://api.github.com/repos/brinkercode/freewheel/releases/1",
            "tag_name": "v1.1.0",
            "name": "v1.1.0",
            "draft": false,
            "prerelease": false,
            "body": body,
        })
        .to_string()
    }

    /// A one-request HTTP server on 127.0.0.1: answers `status` and `body`
    /// (or, with `hang`, accepts and never answers). Returns its base URL
    /// and the request it saw.
    fn serve(
        status: &'static str,
        body: String,
        hang: bool,
    ) -> (String, std::sync::mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let n = s.read(&mut buf).unwrap_or(0);
            let _ = tx.send(String::from_utf8_lossy(&buf[..n]).into_owned());
            if hang {
                std::thread::sleep(Duration::from_secs(5));
                return;
            }
            let _ = write!(
                s,
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        });
        (url, rx)
    }

    fn block<F: std::future::Future>(f: F) -> F::Output {
        tauri::async_runtime::block_on(f)
    }

    #[test]
    fn the_url_is_the_tag_s_release_in_the_releases_api() {
        assert_eq!(
            release_url(RELEASES_API, "1.1.0").as_deref(),
            Some("https://api.github.com/repos/brinkercode/freewheel/releases/tags/v1.1.0")
        );
        assert_eq!(
            release_url(RELEASES_API, "v1.0.0").as_deref(),
            Some("https://api.github.com/repos/brinkercode/freewheel/releases/tags/v1.0.0")
        );
        for bad in [
            "",
            "1.2",
            "1.2.3/../../x",
            "1.2.3?x=1",
            "https://evil.example",
        ] {
            assert_eq!(release_url(RELEASES_API, bad), None, "{bad}");
        }
    }

    #[test]
    fn shipped_builds_ask_github_only() {
        #[cfg(not(feature = "e2e"))]
        {
            const NAME: &str = "FREEWHEEL_E2E_RELEASES_API";
            // SAFETY: tests touching this variable run in this test only.
            unsafe { std::env::set_var(NAME, "http://127.0.0.1:1") };
            assert_eq!(releases_api(), RELEASES_API);
            const { assert!(!LOOPBACK) };
            unsafe { std::env::remove_var(NAME) };
        }
        assert!(RELEASES_API.starts_with("https://api.github.com/"));
    }

    #[test]
    fn the_body_is_read_from_the_release() {
        assert_eq!(
            parse_release(&api_json(FIXTURE_BODY)).unwrap().as_deref(),
            Some(FIXTURE_BODY)
        );
        assert_eq!(
            parse_release(&api_json("a\r\nb")).unwrap().as_deref(),
            Some("a\nb")
        );
        assert_eq!(
            parse_release(r#"{"tag_name":"v1.0.0","body":null}"#).unwrap(),
            None
        );
        assert_eq!(
            parse_release(r#"{"tag_name":"v1.0.0","body":"  \n"}"#).unwrap(),
            None
        );
        assert_eq!(parse_release(r#"{"tag_name":"v1.0.0"}"#).unwrap(), None);
    }

    #[test]
    fn bad_json_is_no_release() {
        for bad in [
            "",
            "not json",
            "[1,2]",
            r#"{"body": 3}"#,
            "<html>rate limited</html>",
        ] {
            assert!(
                matches!(parse_release(bad), Err(NotesError::BadRelease(_))),
                "{bad}"
            );
        }
        let long = "x".repeat(MAX_BODY + 1);
        assert!(matches!(
            parse_release(&api_json(&long)),
            Err(NotesError::BadRelease(_))
        ));
    }

    #[test]
    fn a_200_gives_the_body_with_github_s_headers() {
        let (base, seen) = serve("200 OK", api_json(FIXTURE_BODY), false);
        let url = release_url(&base, "1.1.0").unwrap();
        let body = block(fetch_body(&url, Duration::from_secs(5), true)).unwrap();
        assert_eq!(body.as_deref(), Some(FIXTURE_BODY));
        let req = seen.recv().unwrap().to_ascii_lowercase();
        assert!(req.starts_with("get /tags/v1.1.0 http/1.1"), "{req}");
        assert!(req.contains("accept: application/vnd.github+json"), "{req}");
        assert!(
            req.contains(concat!("user-agent: freewheel/", env!("CARGO_PKG_VERSION"))),
            "{req}"
        );
        assert!(!req.contains("authorization"), "{req}");
    }

    #[test]
    fn a_404_is_no_release_and_other_statuses_fail() {
        let (base, _) = serve("404 Not Found", r#"{"message":"Not Found"}"#.into(), false);
        assert_eq!(
            block(fetch_body(
                &format!("{base}/tags/v9.9.9"),
                Duration::from_secs(5),
                true
            )),
            Ok(None)
        );
        for status in [
            "403 Forbidden",
            "500 Internal Server Error",
            "302 Found",
            "204 No Content",
        ] {
            let (base, _) = serve(status, String::new(), false);
            let r = block(fetch_body(
                &format!("{base}/tags/v1.0.0"),
                Duration::from_secs(5),
                true,
            ));
            assert!(
                matches!(&r, Err(NotesError::Network(e)) if e.contains("HTTP")),
                "{status}: {r:?}"
            );
        }
        let (base, _) = serve("200 OK", "{oops".into(), false);
        let r = block(fetch_body(
            &format!("{base}/tags/v1.0.0"),
            Duration::from_secs(5),
            true,
        ));
        assert!(matches!(r, Err(NotesError::BadRelease(_))), "{r:?}");
    }

    #[test]
    fn a_server_that_never_answers_times_out() {
        let (base, _) = serve("200 OK", String::new(), true);
        let start = std::time::Instant::now();
        let r = block(fetch_body(
            &format!("{base}/tags/v1.0.0"),
            Duration::from_millis(600),
            true,
        ));
        assert!(matches!(r, Err(NotesError::Network(_))), "{r:?}");
        assert!(
            start.elapsed() < Duration::from_secs(4),
            "{:?}",
            start.elapsed()
        );
        const { assert!(TIMEOUT.as_secs() <= 10) };
    }

    #[test]
    fn only_https_is_asked_in_shipped_builds() {
        let r = block(fetch_body(
            "http://127.0.0.1:1/tags/v1.0.0",
            Duration::from_secs(1),
            false,
        ));
        assert!(
            matches!(&r, Err(NotesError::Network(e)) if e.contains("not an https URL")),
            "{r:?}"
        );
        let r = block(fetch_body(
            "http://example.com/tags/v1.0.0",
            Duration::from_secs(1),
            true,
        ));
        assert!(matches!(r, Err(NotesError::Network(_))), "{r:?}");
    }

    #[test]
    fn builds_without_the_update_key_make_no_request() {
        // A server that would answer: it must never be asked.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let cache = NotesCache::default();
        assert_eq!(
            block(notes_for(&cache, "1.1.0", false, &base, true)),
            Err(NotesError::NotConfigured)
        );
        assert!(
            listener.accept().is_err(),
            "a build without the key made a request"
        );
        // And the shipped command follows the build's key.
        assert_eq!(selfupdate::configured(), selfupdate::pubkey().is_some());
    }

    #[test]
    fn fetched_notes_are_kept_per_version() {
        let (base, _) = serve("200 OK", api_json("## 1.1.0\n\n* One"), false);
        let cache = NotesCache::default();
        let first = block(notes_for(&cache, "1.1.0", true, &base, true)).unwrap();
        assert_eq!(first.as_deref(), Some("## 1.1.0\n\n* One"));
        // The server answered once and is gone: the second read is the cache's.
        let again = block(notes_for(&cache, "v1.1.0", true, &base, true)).unwrap();
        assert_eq!(again, first);
        // Another version is asked for (and fails here: nothing listens).
        assert!(block(notes_for(&cache, "1.2.0", true, &base, true)).is_err());
        assert_eq!(
            block(notes_for(&cache, "nope", true, &base, true)),
            Err(NotesError::BadVersion("nope".into()))
        );
    }
}
