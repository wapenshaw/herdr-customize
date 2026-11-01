//! The Browse list: repositories GitHub search finds by topic, each opened up
//! into the packages it holds, named and pictured from their recipes the way
//! herdr.dev's Mods page lists them. See docs/app.md.

use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::mods::{self, Checkout, PREVIEWS};
use super::{read_record, write_record};
use crate::{fsutil, plugins};

/// What was found in each repository, by full name.
const RECORD: &str = "browse.json";

/// How many repositories are read at once.
const PARALLEL: usize = 8;

/// One package in a repository, as Browse lists it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Found {
    /// Its folder in the repository; empty for the root.
    pub path: String,
    pub name: String,
    pub blurb: String,
    /// Its picture, as the app's image() takes it.
    pub preview: Option<String>,
    pub has_mod: bool,
    pub has_layout: bool,
}

impl Found {
    /// Whether Layouts (else Mods) lists it. A package with a layout is a
    /// layout even when it ships mods for it, as herdr.dev sorts them.
    fn listed(&self, layouts: bool) -> bool {
        if layouts { self.has_layout } else { self.has_mod && !self.has_layout }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Crawled {
    pushed_at: String,
    packages: Vec<Found>,
}

/// One row of Browse.
pub struct Row {
    pub repo: plugins::Repo,
    pub found: Found,
}

impl Row {
    /// What a review takes: owner/repo, then the folder.
    pub fn spec(&self) -> String {
        if self.found.path.is_empty() { self.repo.full_name.clone() } else { format!("{}/{}", self.repo.full_name, self.found.path) }
    }
}

/// One page of the repositories tagged herdr-layout (else herdr-mod), as
/// the packages they hold. A repository is read again only once pushed to.
pub fn page(layouts: bool, query: &str, sort: &str, page: usize, per_page: usize) -> Result<(usize, Vec<Row>)> {
    let topic = if layouts { "herdr-layout" } else { "herdr-mod" };
    let results = plugins::search(topic, query, sort, page, per_page)?;
    let mut known: BTreeMap<String, Crawled> = read_record(RECORD)?;
    let stale: Vec<&plugins::Repo> = results.repos.iter().filter(|r| known.get(&r.full_name).is_none_or(|c| c.pushed_at != r.pushed_at)).collect();
    // Workers take the next repository as they finish, so one slow clone
    // never holds up the rest.
    let next = AtomicUsize::new(0);
    let read: Vec<OnceLock<Vec<Found>>> = stale.iter().map(|_| OnceLock::new()).collect();
    std::thread::scope(|s| {
        for _ in 0..PARALLEL.min(stale.len()) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(repo) = stale.get(i) else { break };
                    if let Ok(found) = packages(&repo.full_name) {
                        let _ = read[i].set(found);
                    }
                }
            });
        }
    });
    for (repo, found) in stale.iter().zip(read) {
        // A repository that could not be read is tried again next time.
        if let Some(packages) = found.into_inner() {
            known.insert(repo.full_name.clone(), Crawled { pushed_at: repo.pushed_at.clone(), packages });
        }
    }
    if !stale.is_empty() {
        write_record(RECORD, &known)?;
    }
    let rows = results
        .repos
        .into_iter()
        .flat_map(|repo| {
            let found = known.get(&repo.full_name).map(|c| c.packages.clone()).unwrap_or_default();
            found.into_iter().filter(|f| f.listed(layouts)).map(move |found| Row { repo: repo.clone(), found })
        })
        .collect();
    Ok((results.total, rows))
}

/// The packages in a repository, read from a clone of its file list alone:
/// only each recipe and picture is downloaded.
fn packages(full_name: &str) -> Result<Vec<Found>> {
    let dir = fsutil::temp_path(&std::env::temp_dir(), "herdr-customize-browse-");
    let _checkout = Checkout(dir.clone());
    let d = dir.to_string_lossy();
    let url = format!("{}/{full_name}", mods::git_base());
    mods::git(&["clone", "--depth", "1", "--filter=blob:none", "--no-checkout", "--quiet", &url, &d], Duration::from_secs(60))?;
    let files = mods::git(&["-C", &d, "ls-tree", "-r", "--name-only", "HEAD"], Duration::from_secs(20))?;
    let files: Vec<&str> = files.lines().collect();
    let mut out = Vec::new();
    for root in roots(&files) {
        let at = |name: &str| if root.is_empty() { name.to_string() } else { format!("{root}/{name}") };
        let show = |name: &str| mods::git_bytes(&["-C", &d, "show", &format!("HEAD:{}", at(name))], Duration::from_secs(30));
        let recipe = ["RECIPE.md", "README.md"].iter().find(|f| files.contains(&at(f).as_str())).and_then(|f| show(f).ok());
        let recipe = recipe.map(|b| String::from_utf8_lossy(&b).replace("\r\n", "\n")).unwrap_or_default();
        let preview = PREVIEWS.iter().find(|f| files.contains(&at(f).as_str())).and_then(|f| {
            let ext = f.rsplit('.').next().unwrap_or("png");
            show(f).ok().and_then(|b| mods::save_preview(&b, ext).ok())
        });
        let label = if root.is_empty() { full_name.to_string() } else { format!("{full_name}/{root}") };
        // What review counts: mods/<name>/main.js, and layouts/<name>.json.
        let (mods_dir, layouts_dir) = (at("mods/"), at("layouts/"));
        let has_mod = files.iter().filter_map(|f| f.strip_prefix(mods_dir.as_str())).any(|rest| rest.split_once('/').is_some_and(|(_, file)| file == "main.js"));
        let has_layout = files.iter().filter_map(|f| f.strip_prefix(layouts_dir.as_str())).any(|rest| !rest.contains('/') && rest.ends_with(".json"));
        out.push(Found { name: mods::title(&recipe, &label), blurb: mods::summary(&recipe), preview, path: root, has_mod, has_layout });
    }
    Ok(out)
}

/// The package folders in a file list, by the rule review uses: a folder with
/// mods/ or layouts/ in it, at most two levels down, never inside another.
fn roots(files: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for f in files {
        let parts: Vec<&str> = f.split('/').collect();
        let Some(at) = parts.iter().position(|p| *p == "mods" || *p == "layouts") else { continue };
        if at > 2 || at + 1 >= parts.len() || parts[..at].contains(&"node_modules") {
            continue;
        }
        let root = parts[..at].join("/");
        if !out.contains(&root) {
            out.push(root);
        }
    }
    let all = out.clone();
    out.retain(|r| !all.iter().any(|o| o != r && (o.is_empty() || r.starts_with(&format!("{o}/")))));
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Browse must offer exactly the packages a review of the repository
    // would, or a listed package would fail to review.
    #[test]
    fn roots_match_review() {
        assert_eq!(roots(&["README.md", "mods/a/main.js"]), [""]);
        assert_eq!(roots(&["app/mods/a/main.js", "app/RECIPE.md", "docs/x.png"]), ["app"]);
        assert_eq!(roots(&["one/layouts/x.json", "two/mods/b/main.js", "two/mods/b/mod.json"]), ["one", "two"]);
        assert_eq!(roots(&["mods/a/main.js", "extra/mods/b/main.js"]), [""], "nothing inside a package");
        assert!(roots(&["a/b/c/mods/x/main.js", "node_modules/m/mods/x/main.js"]).is_empty());
    }

    // A repository lists each package under its recipe's name, with its
    // picture copied where the app can load it.
    #[test]
    fn packages_are_named_from_their_recipes() {
        let _guard = crate::ui::tests::isolated();
        let base = std::path::PathBuf::from(crate::config::env("HERDR_CUSTOMIZE_GIT_BASE").unwrap());
        mods::tests::repo(
            &base,
            "o/kit",
            &[
                ("insights/mods/insights/main.js", ""),
                ("insights/RECIPE.md", "# Insights\n\nToken usage.\n"),
                ("insights/preview.png", "png"),
                ("desk/layouts/desk.json", "{}"),
                ("desk/mods/clock/main.js", ""),
            ],
        );
        let found = packages("o/kit").unwrap();
        let got: Vec<(&str, &str, &str)> = found.iter().map(|f| (f.path.as_str(), f.name.as_str(), f.blurb.as_str())).collect();
        assert_eq!(got, [("desk", "desk", ""), ("insights", "Insights", "Token usage.")]);
        let lists = |layouts| found.iter().filter(|f| f.listed(layouts)).map(|f| f.path.as_str()).collect::<Vec<_>>();
        assert_eq!((lists(false), lists(true)), (vec!["insights"], vec!["desk"]), "a layout's own mods leave it under Layouts");
        let preview = found[1].preview.as_deref().unwrap();
        assert_eq!(std::fs::read(super::super::home().join(preview)).unwrap(), b"png");
        assert!(found[0].preview.is_none());
    }
}
