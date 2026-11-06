//! Mods and layouts for the app from GitHub. A repository holds one package
//! or several, each a folder laid out like the app's home: mods/<name>/,
//! layouts/<name>.json, themes/*.json and a keys.json to merge.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use regex::Regex;
use serde::{Deserialize, Serialize};

use super::keys::{self, Binding};
use super::{digest, home, read_object, read_record, state_dir, write_record};
use crate::{fsutil, proc};

/// The installed packages.
const RECORD: &str = "packages.json";

/// owner/repo, then an optional folder inside it.
static SPEC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)((?:/[A-Za-z0-9_.-]+)*)$").unwrap());

/// One package herdr-customize installed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Package {
    pub repo: String,
    /// The package's folder in the repository, "" for its root.
    pub subdir: String,
    pub commit: String,
    pub mods: Vec<String>,
    pub layouts: Vec<String>,
    pub themes: Vec<String>,
    /// Bindings it added to keys.json, as section, key and action.
    pub keys: Vec<[String; 3]>,
    /// Every file it wrote, relative to the home, with its digest.
    pub files: BTreeMap<String, String>,
}

impl Package {
    /// owner/repo, with the folder when the package has one.
    pub fn label(&self) -> String {
        label(&self.repo, &self.subdir)
    }
}

fn label(repo: &str, subdir: &str) -> String {
    if subdir.is_empty() { repo.to_string() } else { format!("{repo}/{subdir}") }
}

pub fn packages() -> Result<Vec<Package>> {
    read_record(RECORD)
}

/// The mod folders in the home.
pub fn mod_names() -> Vec<String> {
    names_in(&home().join("mods"), |p| p.join("main.js").is_file())
}

/// Entry names in dir that pass keep, sorted, without hidden ones.
fn names_in(dir: &Path, keep: impl Fn(&Path) -> bool) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let path = e.ok()?.path();
            let name = path.file_name()?.to_str()?.to_string();
            (!name.starts_with('.') && keep(&path)).then_some(name)
        })
        .collect();
    names.sort();
    names
}

fn layout_names(dir: &Path) -> Vec<String> {
    names_in(dir, |p| p.extension().is_some_and(|e| e == "json")).into_iter().map(|n| n.trim_end_matches(".json").to_string()).collect()
}

/// A mod or a layout in the home.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Mod,
    Layout,
}

/// Mods and layouts the person added by hand: neither shipped with the app
/// nor installed here.
pub fn others(packages: &[Package]) -> Vec<(Kind, String)> {
    let shipped = super::shipped();
    let mut out = Vec::new();
    for name in mod_names() {
        if name != super::MOD_NAME && !shipped.contains(&name) && !packages.iter().any(|p| p.mods.contains(&name)) {
            out.push((Kind::Mod, name));
        }
    }
    for name in layout_names(&home().join("layouts")) {
        if !shipped.contains(&format!("layout:{name}")) && !packages.iter().any(|p| p.layouts.contains(&name)) {
            out.push((Kind::Layout, name));
        }
    }
    out
}

/// A cloned repository, removed when the last plan using it goes.
#[derive(Debug)]
pub struct Checkout(pub(super) PathBuf);

impl Drop for Checkout {
    fn drop(&mut self) {
        remove_tree(&self.0);
    }
}

/// Deletes dir, clearing the read-only flag git sets on Windows first.
fn remove_tree(dir: &Path) {
    if fs::remove_dir_all(dir).is_ok() || !dir.exists() {
        return;
    }
    for file in files_under(dir, true) {
        if let Ok(meta) = fs::metadata(&file) {
            let mut perms = meta.permissions();
            #[allow(clippy::permissions_set_readonly_false, reason = "only to delete our own temporary checkout")]
            perms.set_readonly(false);
            let _ = fs::set_permissions(&file, perms);
        }
    }
    let _ = fs::remove_dir_all(dir);
}

/// Every file under dir, skipping .git and node_modules unless all is set.
fn files_under(dir: &Path, all: bool) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).into_iter().flatten().flatten() {
            let path = e.path();
            let skip = !all && matches!(e.file_name().to_str(), Some(".git" | "node_modules"));
            match e.file_type() {
                Ok(t) if t.is_dir() && !skip => stack.push(path),
                Ok(t) if t.is_file() => out.push(path),
                _ => {}
            }
        }
    }
    out.sort();
    out
}

/// What installing one package would do.
#[derive(Debug, Clone)]
pub struct Plan {
    pub repo: String,
    pub subdir: String,
    pub commit: String,
    pub mods: Vec<String>,
    pub layouts: Vec<String>,
    pub themes: Vec<String>,
    pub keys: Vec<Binding>,
    /// Its keys the person already uses for something else, with their action.
    pub key_clashes: Vec<(Binding, String)>,
    /// Names something other than this package already holds.
    pub taken: Vec<String>,
    /// Files of an earlier install of it that changed since.
    pub edited: Vec<String>,
    /// What its code reaches beyond its own folder.
    pub uses: Vec<&'static str>,
    /// The first paragraph of its RECIPE.md or README.md.
    pub summary: String,
    /// The heading of that file, else the package's folder.
    pub title: String,
    /// That whole file, as written.
    pub recipe: String,
    /// The repository's default branch, which install reads.
    pub branch: String,
    /// Its preview.png (or .jpg and the like), in the clone.
    pub preview: Option<PathBuf>,
    /// It is installed already, so this updates it.
    pub update: bool,
    /// Source file → path relative to the home.
    files: Vec<(PathBuf, String)>,
    _checkout: Arc<Checkout>,
}

impl Plan {
    pub fn label(&self) -> String {
        label(&self.repo, &self.subdir)
    }
}

/// Where git clone finds owner/repo; replaced in tests.
pub(super) fn git_base() -> String {
    crate::config::env("HERDR_CUSTOMIZE_GIT_BASE").unwrap_or_else(|| "https://github.com".into())
}

pub(super) fn git(args: &[&str], timeout: Duration) -> Result<String> {
    Ok(String::from_utf8_lossy(&git_bytes(args, timeout)?).trim().to_string())
}

/// What git printed, as bytes: a file `git show` read can be an image.
pub(super) fn git_bytes(args: &[&str], timeout: Duration) -> Result<Vec<u8>> {
    let mut cmd = Command::new("git");
    cmd.args(args).env("GIT_TERMINAL_PROMPT", "0").env("GIT_HTTP_LOW_SPEED_LIMIT", "1000").env("GIT_HTTP_LOW_SPEED_TIME", "10");
    let out = proc::output_within(&mut cmd, timeout).context("git")?;
    if !out.status.success() {
        bail!("git {}: {}", args[0], String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(out.stdout)
}

/// Clones spec (owner/repo, owner/repo/folder, or a GitHub link to either,
/// such as github.com/owner/repo/tree/main/folder) and plans each package in
/// it. Nothing is written to the home yet.
pub fn fetch(spec: &str) -> Result<Vec<Plan>> {
    let spec = spec.trim();
    let spec = spec.strip_prefix("https://").or_else(|| spec.strip_prefix("http://")).unwrap_or(spec);
    let spec = spec.strip_prefix("github.com/").unwrap_or(spec).trim_end_matches('/');
    let m = SPEC.captures(spec).ok_or_else(|| anyhow!("{spec:?} is not owner/repo or owner/repo/folder"))?;
    let (repo, mut want) = (m[1].trim_end_matches(".git").to_string(), m[2].trim_start_matches('/').to_string());
    let branch = want.strip_prefix("tree/").map(|rest| rest.split_once('/').unwrap_or((rest, ""))).map(|(b, folder)| (b.to_string(), folder.to_string()));
    if let Some((_, folder)) = &branch {
        want = folder.clone();
    }
    let dir = fsutil::temp_path(&std::env::temp_dir(), "herdr-customize-mod-");
    let checkout = Arc::new(Checkout(dir.clone()));
    let dir_text = dir.to_string_lossy();
    git(&["clone", "--depth", "1", "--quiet", "--no-tags", &format!("{}/{repo}", git_base()), &dir_text], Duration::from_secs(120)).with_context(|| format!("download {repo}"))?;
    let head = git(&["-C", &dir_text, "rev-parse", "--abbrev-ref", "HEAD"], Duration::from_secs(10))?;
    // Install and updates read the default branch, so a link to another one
    // would review files that never get installed.
    if let Some((b, _)) = branch
        && b != head
    {
        bail!("{repo}: only its default branch, {head}, can be installed, not {b}");
    }
    let commit = git(&["-C", &dir_text, "rev-parse", "HEAD"], Duration::from_secs(10))?;
    let installed = packages()?;
    let mut plans = Vec::new();
    for root in package_dirs(&dir) {
        let subdir = root.strip_prefix(&dir).unwrap_or(&root).to_string_lossy().replace('\\', "/");
        if !want.is_empty() && subdir != want {
            continue;
        }
        let previous = installed.iter().find(|p| p.repo.eq_ignore_ascii_case(&repo) && p.subdir == subdir);
        let mut p = plan(&root, &repo, &subdir, &commit, previous, checkout.clone())?;
        p.branch.clone_from(&head);
        plans.push(p);
    }
    if plans.is_empty() {
        bail!("{} has no mods/ or layouts/ folder{}", repo, if want.is_empty() { String::new() } else { format!(" at {want}") });
    }
    Ok(plans)
}

/// Folders laid out like the app's home: the root, or folders a few levels
/// down for a repository of several.
fn package_dirs(root: &Path) -> Vec<PathBuf> {
    let is_package = |d: &Path| d.join("mods").is_dir() || d.join("layouts").is_dir();
    let mut out = Vec::new();
    let mut level = vec![root.to_path_buf()];
    for _ in 0..3 {
        let mut next = Vec::new();
        for d in level {
            if is_package(&d) {
                out.push(d);
                continue;
            }
            next.extend(names_in(&d, Path::is_dir).into_iter().filter(|n| n != "node_modules").map(|n| d.join(n)));
        }
        level = next;
    }
    out
}

fn plan(root: &Path, repo: &str, subdir: &str, commit: &str, previous: Option<&Package>, checkout: Arc<Checkout>) -> Result<Plan> {
    let mods = names_in(&root.join("mods"), |p| p.join("main.js").is_file());
    let layouts = layout_names(&root.join("layouts"));
    let themes = layout_names(&root.join("themes"));
    let mut files = Vec::new();
    for m in &mods {
        let dir = root.join("mods").join(m);
        for f in files_under(&dir, false) {
            let rel = f.strip_prefix(&dir).unwrap_or(&f).to_string_lossy().replace('\\', "/");
            files.push((f, format!("mods/{m}/{rel}")));
        }
    }
    for folder in ["layouts", "themes", "fonts"] {
        for name in names_in(&root.join(folder), Path::is_file) {
            files.push((root.join(folder).join(&name), format!("{folder}/{name}")));
        }
    }
    let home = home();
    let theirs = |rel: &str| previous.is_some_and(|p| p.files.contains_key(rel));
    let mut taken = Vec::new();
    for (kind, names) in [("mods", &mods), ("layouts", &layouts), ("themes", &themes)] {
        for n in names {
            let rel = if kind == "mods" { format!("mods/{n}") } else { format!("{kind}/{n}.json") };
            let mine = previous.is_some_and(|p| p.mods.contains(n) || p.layouts.contains(n) || p.themes.contains(n));
            if home.join(&rel).exists() && !mine {
                taken.push(rel);
            }
        }
    }
    for (_, rel) in &files {
        if home.join(rel).exists() && !theirs(rel) && !taken.iter().any(|t| rel.starts_with(t.as_str())) {
            taken.push(rel.clone());
        }
    }
    let edited = previous.map(|p| p.files.iter().filter(|(rel, d)| fs::read(home.join(rel)).is_ok_and(|b| digest(&b) != **d)).map(|(rel, _)| rel.clone()).collect()).unwrap_or_default();
    let keys = package_keys(&root.join("keys.json"))?;
    let current = keys::load()?;
    let recipe = ["RECIPE.md", "README.md"].iter().find_map(|f| fs::read_to_string(root.join(f)).ok()).unwrap_or_default().replace("\r\n", "\n");
    let added_before = |b: &Binding| previous.is_some_and(|p| p.keys.iter().any(|k| k[0] == b.section && k[1] == b.key));
    let key_clashes = keys
        .iter()
        .filter_map(|b| {
            let held = current.iter().find(|c| c.section == b.section && keys::normalize(&c.key) == keys::normalize(&b.key))?;
            (held.action != b.action && !added_before(b)).then(|| (b.clone(), held.action.clone()))
        })
        .collect();
    Ok(Plan {
        repo: repo.into(),
        subdir: subdir.into(),
        commit: commit.into(),
        uses: uses(&files),
        summary: summary(&recipe),
        title: title(&recipe, &label(repo, subdir)),
        recipe,
        branch: String::new(),
        preview: PREVIEWS.iter().map(|n| root.join(n)).find(|p| p.is_file()),
        update: previous.is_some(),
        mods,
        layouts,
        themes,
        keys,
        key_clashes,
        taken,
        edited,
        files,
        _checkout: checkout,
    })
}

fn package_keys(path: &Path) -> Result<Vec<Binding>> {
    Ok(keys::bindings(read_object(path).context("the package's keys.json")?))
}

/// What a package's code reaches, as the app's guide asks to say before
/// adding one.
fn uses(files: &[(PathBuf, String)]) -> Vec<&'static str> {
    let code: String = files.iter().filter(|(f, _)| f.extension().is_some_and(|e| e == "js")).filter_map(|(f, _)| fs::read_to_string(f).ok()).collect();
    let mut out = Vec::new();
    for (needles, what) in [(&["exec(", "hc("][..], "runs programs"), (&["fetch("][..], "reaches the network"), (&["readTextFile(", "writeTextFile(", "\"fs/promises\""][..], "reads or writes files")]
    {
        if needles.iter().any(|n| code.contains(n)) {
            out.push(what);
        }
    }
    out
}

/// The names a package's picture goes by, as herdr.dev looks for it.
pub(super) const PREVIEWS: [&str; 4] = ["preview.png", "preview.jpg", "preview.jpeg", "preview.webp"];

/// Copies a plan's preview where the app's image() finds it, and returns that
/// path relative to the home.
pub fn keep_preview(plan: &Plan) -> Result<Option<String>> {
    let Some(src) = &plan.preview else { return Ok(None) };
    let ext = src.extension().map_or_else(|| "png".into(), |e| e.to_string_lossy().to_lowercase());
    save_preview(&fs::read(src)?, &ext).map(Some)
}

/// Writes a picture where the app's image() finds it, named by its bytes so
/// the app never shows a cached older one, and returns that path relative to
/// the home.
pub(super) fn save_preview(bytes: &[u8], ext: &str) -> Result<String> {
    let path = state_dir().join("previews").join(format!("{}.{ext}", digest(bytes)));
    if !path.is_file() {
        fs::create_dir_all(path.parent().unwrap_or(&path))?;
        fsutil::atomic_write(&path, bytes, 0o644)?;
    }
    Ok(super::home_relative(&path))
}

/// A package's name, as herdr.dev shows it: the recipe's heading, else the
/// last part of its label.
pub(super) fn title(recipe: &str, label: &str) -> String {
    recipe.lines().find_map(|l| l.strip_prefix("# ")).map_or_else(|| label.rsplit('/').next().unwrap_or_default().to_string(), |t| t.trim().to_string())
}

/// The first paragraph of the package's recipe or readme, in plain words.
pub(super) fn summary(text: &str) -> String {
    let para = text.split("\n\n").map(str::trim).find(|p| !p.is_empty() && !p.starts_with('#') && !p.starts_with('!') && !p.starts_with('<')).unwrap_or("");
    let plain: String = para.split_whitespace().collect::<Vec<_>>().join(" ").replace(['*', '`'], "");
    plain.chars().take(400).collect()
}

/// What install did beyond copying.
#[derive(Debug, Default)]
pub struct Report {
    /// Each mod's check: None when it passed.
    pub checks: Vec<(String, Option<String>)>,
    /// Keys left as the person had them.
    pub kept_keys: usize,
}

/// Copies plan's package into the home, merges its keys (taking the
/// person's own only with replace_keys), and checks each mod.
pub fn install(plan: &Plan, replace_keys: bool) -> Result<Report> {
    if let Some(t) = plan.taken.first() {
        bail!("{t} already exists and is not from {}", plan.label());
    }
    let home = home();
    let mut record = packages()?;
    let old = record.iter().position(|p| p.repo.eq_ignore_ascii_case(&plan.repo) && p.subdir == plan.subdir).map(|i| record.remove(i));
    if let Some(old) = &old {
        for rel in old.files.keys().filter(|rel| !plan.files.iter().any(|(_, r)| r == *rel)) {
            let _ = fs::remove_file(home.join(rel));
        }
    }
    let mut pkg = Package {
        repo: plan.repo.clone(),
        subdir: plan.subdir.clone(),
        commit: plan.commit.clone(),
        mods: plan.mods.clone(),
        layouts: plan.layouts.clone(),
        themes: plan.themes.clone(),
        ..Default::default()
    };
    for (src, rel) in &plan.files {
        let data = fs::read(src).with_context(|| format!("read {rel}"))?;
        let dest = home.join(rel);
        fs::create_dir_all(dest.parent().unwrap_or(&home))?;
        fsutil::atomic_write(&dest, &data, 0o644).with_context(|| format!("write {rel}"))?;
        pkg.files.insert(rel.clone(), digest(&data));
    }
    let mut report = Report::default();
    let current = keys::load()?;
    for b in &plan.keys {
        let held = current.iter().find(|c| c.section == b.section && keys::normalize(&c.key) == keys::normalize(&b.key));
        let before = old.as_ref().is_some_and(|p| p.keys.iter().any(|k| k[0] == b.section && k[1] == b.key));
        match held {
            Some(h) if h.action == b.action => {}
            Some(_) if !replace_keys && !before => {
                report.kept_keys += 1;
                continue;
            }
            _ => keys::set(&b.section, held.map(|h| h.key.as_str()), &b.key, &b.action)?,
        }
        pkg.keys.push([b.section.clone(), b.key.clone(), b.action.clone()]);
    }
    record.push(pkg);
    write_record(RECORD, &record)?;
    for m in &plan.mods {
        report.checks.push((m.clone(), super::check_mod(&home.join("mods").join(m)).err().map(|e| format!("{e:#}"))));
    }
    Ok(report)
}

/// Removes an installed package: the files it wrote that nobody changed and
/// the keys it added. Returns the files kept because they were edited.
pub fn remove(repo: &str, subdir: &str) -> Result<Vec<String>> {
    let home = home();
    let mut record = packages()?;
    let i = record.iter().position(|p| p.repo == repo && p.subdir == subdir).ok_or_else(|| anyhow!("{} is not installed", label(repo, subdir)))?;
    let pkg = record.remove(i);
    let shown = read_object(&home.join("shared.json")).ok().and_then(|m| m.get("layout")?.as_str().map(String::from));
    if shown.is_some_and(|l| pkg.layouts.contains(&l)) {
        // Best effort: with the app closed there is nothing to switch.
        let _ = super::run("layout.switch", &[("name", "main")]);
    }
    let mut kept = Vec::new();
    for (rel, d) in &pkg.files {
        let path = home.join(rel);
        match fs::read(&path) {
            Ok(b) if digest(&b) == *d => fs::remove_file(&path).with_context(|| format!("delete {rel}"))?,
            Ok(_) => kept.push(rel.clone()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).with_context(|| format!("read {rel}")),
        }
    }
    for m in &pkg.mods {
        let dir = home.join("mods").join(m);
        if !dir.join("main.js").exists() {
            for f in GENERATED {
                let _ = fs::remove_file(dir.join(f));
            }
        }
        prune(&dir);
    }
    let current = keys::load()?;
    for [section, key, action] in &pkg.keys {
        if current.iter().any(|c| c.section == *section && c.key == *key && c.action == *action) {
            keys::remove(section, key)?;
        }
    }
    write_record(RECORD, &record)?;
    Ok(kept)
}

/// Files the app writes into every mod folder for editors.
const GENERATED: [&str; 3] = ["gpui-kit.d.ts", "herdr.d.ts", "jsconfig.json"];

/// Removes dir and the folders under it that are left empty.
fn prune(dir: &Path) {
    for d in names_in(dir, Path::is_dir) {
        prune(&dir.join(d));
    }
    let _ = fs::remove_dir(dir);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A git repository at base/owner/name holding files.
    pub(crate) fn repo(base: &Path, name: &str, files: &[(&str, &str)]) {
        let dir = base.join(name);
        let _ = fs::remove_dir_all(&dir);
        for (rel, text) in files {
            let path = dir.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        let d = dir.to_string_lossy().to_string();
        for args in [&["init", "--quiet", &d][..], &["-C", &d, "add", "-A"], &["-C", &d, "-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "x"]] {
            assert!(Command::new("git").args(args).status().unwrap().success(), "git {args:?}");
        }
    }

    // A mod installs with its keys, never over the person's own key or
    // files, and goes away leaving what the person changed.
    #[test]
    fn install_update_and_remove() {
        let _guard = crate::ui::tests::isolated();
        let home = home();
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();
        fs::write(keys::path(), r#"{"global":{"ctrl-shift-j":"tab.new"}}"#).unwrap();
        let base = PathBuf::from(crate::config::env("HERDR_CUSTOMIZE_GIT_BASE").unwrap());
        let main = "import { View } from \"gpui-kit\";\nimport { exec } from \"herdr\";\nexec(\"git\");\n";
        repo(
            &base,
            "o/one",
            &[
                ("RECIPE.md", "# One\n\nA *board* of\nthings.\n"),
                ("mods/board/main.js", main),
                ("mods/board/icon.svg", "<svg/>"),
                ("layouts/board.json", "{}"),
                ("keys.json", r#"{"global":{"ctrl-shift-j":"board.open","ctrl-shift-q":"mod.open:board"}}"#),
            ],
        );
        let plans = fetch("o/one").unwrap();
        let p = &plans[0];
        assert_eq!(
            (p.mods.clone(), p.layouts.clone(), p.uses.clone(), p.summary.as_str(), p.update),
            (vec!["board".to_string()], vec!["board".to_string()], vec!["runs programs"], "A board of things.", false)
        );
        assert_eq!(p.key_clashes.len(), 1);
        let report = install(p, false).unwrap();
        assert_eq!(report.kept_keys, 1);
        assert_eq!(report.checks, [("board".to_string(), None)]);
        assert!(home.join("mods/board/icon.svg").is_file() && home.join("layouts/board.json").is_file());
        let keys: Vec<String> = keys::load().unwrap().into_iter().map(|b| format!("{}={}", b.key, b.action)).collect();
        assert_eq!(keys, ["ctrl-shift-j=tab.new", "ctrl-shift-q=mod.open:board"]);
        // Installing again is an update; an edited file is reported.
        fs::write(home.join("mods/board/icon.svg"), "<svg>mine</svg>").unwrap();
        fs::write(home.join("mods/board/jsconfig.json"), "{}").unwrap();
        let again = fetch("o/one").unwrap();
        assert!(again[0].update && again[0].taken.is_empty());
        assert_eq!(again[0].edited, ["mods/board/icon.svg"]);
        // Someone else's mod by the same name is never written over.
        repo(&base, "o/two", &[("mods/board/main.js", main)]);
        let other = fetch("o/two").unwrap();
        assert_eq!(other[0].taken, ["mods/board"]);
        assert!(install(&other[0], true).is_err());
        let kept = remove("o/one", "").unwrap();
        assert_eq!(kept, ["mods/board/icon.svg"]);
        assert!(!home.join("mods/board/main.js").exists() && !home.join("layouts/board.json").exists());
        assert!(!home.join("mods/board/jsconfig.json").exists(), "the app's editor files go with the mod");
        let keys: Vec<String> = keys::load().unwrap().into_iter().map(|b| b.key).collect();
        assert_eq!(keys, ["ctrl-shift-j"]);
        assert!(packages().unwrap().is_empty());
        let _ = fs::remove_dir_all(&home);
    }

    // A repository of several packages offers each, and a folder picks one.
    #[test]
    fn several_packages() {
        let _guard = crate::ui::tests::isolated();
        let base = PathBuf::from(crate::config::env("HERDR_CUSTOMIZE_GIT_BASE").unwrap());
        repo(&base, "o/many", &[("a/layouts/a.json", "{}"), ("b/mods/b/main.js", ""), ("README.md", "x")]);
        let labels: Vec<String> = fetch("o/many").unwrap().iter().map(Plan::label).collect();
        assert_eq!(labels, ["o/many/a", "o/many/b"]);
        assert_eq!(fetch("https://github.com/o/many/b").unwrap().len(), 1);
        // The link herdr.dev gives reviews; one to another branch would review
        // files that install never takes.
        let dir = base.join("o/many").to_string_lossy().to_string();
        let head = String::from_utf8(Command::new("git").args(["-C", &dir, "rev-parse", "--abbrev-ref", "HEAD"]).output().unwrap().stdout).unwrap();
        let plans = fetch(&format!("github.com/o/many/tree/{}/b", head.trim())).unwrap();
        assert_eq!(plans.iter().map(Plan::label).collect::<Vec<_>>(), ["o/many/b"]);
        assert!(fetch("https://github.com/o/many/tree/other/b").unwrap_err().to_string().contains("default branch"));
        assert!(fetch("o/many/c").is_err());
        assert!(fetch("not a repo").is_err());
    }
}
