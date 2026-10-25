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
    let mut q = format!("topic:{MARKET_TOPIC}");
    if !query.trim().is_empty() {
        q += &format!(" {}", query.trim());
    }
    // The 50-result page is a few hundred KB; slow links need the time.
    let mut req = ureq::get(&format!("{}/search/repositories", api_base()))
        .query("q", &q)
        .query("sort", sort)
        .query("order", "desc")
        .query("per_page", &per_page.to_string())
        .query("page", &page.to_string())
        .set("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(20));
    if let Some(token) = github_token() {
        req = req.set("Authorization", &format!("Bearer {token}"));
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
    let resp = match req.call() {
        Ok(resp) => resp,
        Err(ureq::Error::Status(code, resp)) => {
            let reason = if code == 403 || code == 429 { "GitHub rate limit" } else { "marketplace search failed" };
            let message = resp.into_string().ok().and_then(|s| serde_json::from_str::<Data>(&s).ok()).map(|d| d.message).unwrap_or_default();
            if message.is_empty() {
                bail!("{reason} (HTTP {code})");
            }
            bail!("{reason} (HTTP {code}): {message}");
        }
        Err(ureq::Error::Transport(t)) if t.kind() == ureq::ErrorKind::Io => bail!("marketplace search timed out or lost its connection"),
        Err(_) => bail!("marketplace search failed: no connection"),
    };
    let data: Data = serde_json::from_str(&resp.into_string()?).map_err(|e| anyhow!("marketplace search: {e}"))?;
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
