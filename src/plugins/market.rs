//! The marketplace: GitHub repositories tagged with the plugin topic.

use std::process::Command;
use std::sync::LazyLock;
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use regex::Regex;
use serde::Deserialize;

use crate::config::env;
use crate::proc;

/// The GitHub topic that lists a repository in the marketplace.
pub const MARKET_TOPIC: &str = "herdr-plugin";

/// How far GitHub search pages: it serves at most 1000 results.
const MARKET_LIMIT: usize = 1000;

/// The GitHub REST API; replaced in tests.
fn api_base() -> String {
    env("HERDR_CUSTOMIZE_API_BASE").unwrap_or_else(|| "https://api.github.com".into())
}

/// One marketplace search result.
#[derive(Debug, Clone, Default)]
pub struct Repo {
    pub full_name: String,
    pub stars: u64,
    pub description: String,
}

/// One page of a marketplace search.
#[derive(Debug, Clone, Default)]
pub struct Results {
    pub total: usize,
    pub repos: Vec<Repo>,
}

/// A result name reaches herdr plugin install and the browser, so only
/// plain owner/repo slugs are accepted from the API.
static SLUG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$").unwrap());

/// Finds repositories tagged with the topic. sort is "stars" or "updated";
/// page counts from 1.
pub fn search(query: &str, sort: &str, page: usize, per_page: usize) -> Result<Results> {
    search_at(&api_base(), github_token(), query, sort, page, per_page)
}

fn search_at(base: &str, token: Option<String>, query: &str, sort: &str, page: usize, per_page: usize) -> Result<Results> {
    let mut q = format!("topic:{MARKET_TOPIC}");
    if !query.trim().is_empty() {
        q += &format!(" {}", query.trim());
    }
    // The 50-result page is a few hundred KB; slow links need the time.
    let mut req = ureq::get(&format!("{base}/search/repositories"))
        .query("q", &q)
        .query("sort", sort)
        .query("order", "desc")
        .query("per_page", per_page.to_string())
        .query("page", page.to_string())
        .header("Accept", "application/vnd.github+json");
    if let Some(token) = token {
        req = req.header("Authorization", format!("Bearer {token}"));
    }
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Data {
        message: String,
        total_count: usize,
        items: Vec<Item>,
    }
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Item {
        full_name: String,
        stargazers_count: u64,
        description: Option<String>,
    }
    // Error statuses come back as responses so the API's message can be shown.
    let mut resp = match req.config().timeout_global(Some(Duration::from_secs(20))).http_status_as_error(false).build().call() {
        Ok(resp) => resp,
        Err(ureq::Error::Timeout(_) | ureq::Error::Io(_)) => bail!("marketplace search timed out or lost its connection"),
        Err(_) => bail!("marketplace search failed: no connection"),
    };
    let code = resp.status().as_u16();
    let body = resp.body_mut().read_to_string();
    if code >= 400 {
        let reason = if code == 403 || code == 429 { "GitHub rate limit" } else { "marketplace search failed" };
        let message = body.ok().and_then(|s| serde_json::from_str::<Data>(&s).ok()).map(|d| d.message).unwrap_or_default();
        if message.is_empty() {
            bail!("{reason} (HTTP {code})");
        }
        bail!("{reason} (HTTP {code}): {message}");
    }
    let data: Data = serde_json::from_str(&body?).map_err(|e| anyhow!("marketplace search: {e}"))?;
    Ok(Results {
        total: data.total_count.min(MARKET_LIMIT),
        repos: data
            .items
            .into_iter()
            .filter(|i| SLUG.is_match(&i.full_name))
            .map(|i| Repo { full_name: i.full_name, stars: i.stargazers_count, description: sanitize(&i.description.unwrap_or_default()) })
            .collect(),
    })
}

fn sanitize(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect::<String>().trim().to_string()
}

/// A token raises the search rate limit: GH_TOKEN, GITHUB_TOKEN, then the gh
/// CLI's login. HERDR_CUSTOMIZE_NO_TOKEN=1 keeps searches anonymous.
fn github_token() -> Option<String> {
    if env("HERDR_CUSTOMIZE_NO_TOKEN").as_deref() == Some("1") {
        return None;
    }
    if let Some(token) = env("GH_TOKEN").or_else(|| env("GITHUB_TOKEN")) {
        return Some(token);
    }
    let gh = proc::look_path("gh")?;
    let out = Command::new(gh).args(["auth", "token"]).output().ok()?;
    let token = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !token.is_empty()).then_some(token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    /// Serves one canned HTTP response and returns the server's base URL
    /// and the request line it received.
    fn serve(status: &str, body: &'static str) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let status = status.to_string();
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream);
            let mut head = String::new();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                    break;
                }
                head += &line;
            }
            let reply = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            reader.get_mut().write_all(reply.as_bytes()).unwrap();
            head
        });
        (base, handle)
    }

    #[test]
    fn search_parses_results_and_sends_token() {
        let (base, server) = serve("200 OK", r#"{"total_count":2,"items":[{"full_name":"a/b","stargazers_count":5,"description":"one\ntwo"},{"full_name":"bad slug/x","stargazers_count":1}]}"#);
        let r = search_at(&base, Some("t0k".into()), "theme", "stars", 2, 50).unwrap();
        let head = server.join().unwrap();
        assert!(head.starts_with("GET /search/repositories?q=topic%3Aherdr-plugin%20theme&sort=stars&order=desc&per_page=50&page=2 "), "{head}");
        assert!(head.to_lowercase().contains("authorization: bearer t0k"), "{head}");
        assert_eq!(r.total, 2);
        assert_eq!(r.repos.len(), 1);
        assert_eq!((r.repos[0].full_name.as_str(), r.repos[0].stars, r.repos[0].description.as_str()), ("a/b", 5, "one two"));
    }

    #[test]
    fn search_reports_rate_limit_message() {
        let (base, server) = serve("403 Forbidden", r#"{"message":"API rate limit exceeded"}"#);
        let err = search_at(&base, None, "", "stars", 1, 50).unwrap_err().to_string();
        server.join().unwrap();
        assert_eq!(err, "GitHub rate limit (HTTP 403): API rate limit exceeded");
    }

    #[test]
    fn search_reports_status_without_message() {
        let (base, server) = serve("500 Internal Server Error", "oops");
        let err = search_at(&base, None, "", "stars", 1, 50).unwrap_err().to_string();
        server.join().unwrap();
        assert_eq!(err, "marketplace search failed (HTTP 500)");
    }

    #[test]
    fn search_reports_no_connection() {
        // Nothing listens on the discard port.
        let err = search_at("http://127.0.0.1:9", None, "", "stars", 1, 50).unwrap_err().to_string();
        assert!(err.starts_with("marketplace search"), "{err}");
    }
}
