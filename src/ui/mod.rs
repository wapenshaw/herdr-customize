//! The popup, a page per area. Pages never block: slow work runs on a thread
//! and returns as a Msg every page sees, and anything a page wants the app to
//! do (status, work, timers, an editor, closing) goes through Ctx.

mod keys;
mod pluginconfig;
mod plugins;
mod settings;
mod themes;
mod values;
mod widgets;

use std::collections::VecDeque;
use std::io;
use std::path::PathBuf;
use std::process::{Command, ExitStatus};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::{execute, terminal};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::config::{CheckError, Doc, SaveResult, Store};
use crate::plugins::Plugin;
use crate::schema::{self, Reference};
use widgets::*;

pub const PAGES: [&str; 4] = ["settings", "keys", "themes", "plugins"];

/// Pages hidden until turned on in Settings, in PAGES order.
pub const OPTIONAL: [&str; 2] = ["themes", "plugins"];

/// Lists the optional pages turned on, one per line.
fn pages_file() -> PathBuf {
    crate::theme::state_dir().join("pages")
}

/// The optional pages turned on. A missing or unreadable file means none.
fn load_pages() -> Vec<&'static str> {
    let text = std::fs::read_to_string(pages_file()).unwrap_or_default();
    OPTIONAL.into_iter().filter(|p| text.lines().any(|l| l.trim() == *p)).collect()
}

/// One key press, named the way the pages match on it: "a", "A", " ",
/// "enter", "ctrl+c", "shift+tab", "pgdown". A paste comes whole.
#[derive(Debug, Clone, Default)]
pub struct Key {
    pub name: String,
    /// The typed character, for plain and shifted printable keys.
    pub ch: Option<char>,
    pub paste: Option<String>,
}

impl Key {
    pub fn named(name: &str) -> Key {
        let mut chars = name.chars();
        let ch = match (chars.next(), chars.next()) {
            (Some(c), None) => Some(c),
            _ => None,
        };
        Key { name: name.into(), ch, paste: None }
    }

    fn from_event(e: KeyEvent) -> Option<Key> {
        if e.kind == KeyEventKind::Release {
            return None;
        }
        let m = e.modifiers;
        let base = match e.code {
            KeyCode::Char(c) if m.contains(KeyModifiers::CONTROL) => return Some(Key { name: format!("ctrl+{}", c.to_ascii_lowercase()), ..Default::default() }),
            KeyCode::Char(c) if m.contains(KeyModifiers::ALT) => return Some(Key { name: format!("alt+{c}"), ..Default::default() }),
            KeyCode::Char(c) => return Some(Key::named(&c.to_string())),
            KeyCode::Enter => "enter",
            KeyCode::Esc => "esc",
            KeyCode::Tab => "tab",
            KeyCode::BackTab => return Some(Key::named("shift+tab")),
            KeyCode::Backspace => "backspace",
            KeyCode::Delete => "delete",
            KeyCode::Insert => "insert",
            KeyCode::Up => "up",
            KeyCode::Down => "down",
            KeyCode::Left => "left",
            KeyCode::Right => "right",
            KeyCode::Home => "home",
            KeyCode::End => "end",
            KeyCode::PageUp => "pgup",
            KeyCode::PageDown => "pgdown",
            KeyCode::F(n) => return Some(Key::named(&format!("f{n}"))),
            _ => return None,
        };
        let mut name = String::new();
        for (flag, label) in [(KeyModifiers::CONTROL, "ctrl+"), (KeyModifiers::ALT, "alt+"), (KeyModifiers::SHIFT, "shift+")] {
            if m.contains(flag) {
                name.push_str(label);
            }
        }
        name.push_str(base);
        Some(Key { name, ..Default::default() })
    }

    pub fn is(&self, name: &str) -> bool {
        self.name == name
    }
}

/// A config save, reported to every page.
#[derive(Debug, Default)]
pub struct Saved {
    pub what: String,
    /// The dotted key the save changed; None when it replaced the whole file.
    pub key: Option<String>,
    pub result: SaveResult,
    pub err: Option<anyhow::Error>,
    /// The config as the save left it, decoded; None when it failed.
    pub config: Option<toml::Table>,
}

impl Saved {
    /// The status line for the save.
    fn describe(&self) -> (String, bool) {
        if let Some(e) = &self.err {
            if let Some(check) = e.downcast_ref::<CheckError>() {
                return (format!("not saved: {check}"), true);
            }
            return (format!("not saved: {e:#}"), true);
        }
        if !self.result.changed {
            return (format!("{}: no change", self.what), false);
        }
        if self.result.reload_err.is_some() {
            return (format!("{} saved; Herdr did not reload, press prefix+shift+r", self.what), false);
        }
        if !self.result.check_skipped.is_empty() {
            return (format!("{} saved (herdr config check unavailable)", self.what), false);
        }
        (format!("{} saved and reloaded", self.what), false)
    }
}

/// What background work reports back. Pages take what is theirs.
pub enum Msg {
    /// The key reference matching the installed Herdr.
    Reference(Reference),
    Saved(Saved),
    PluginList(Result<Vec<Plugin>>),
    /// The optional pages were saved, or why not.
    PagesSaved(Result<()>),
    /// Asks the Keys page for a binding to a plugin action.
    BindAction {
        qualified: String,
        title: String,
    },
    /// A pick's delay is over; the screen holding pick seq saves it.
    PendingSave {
        seq: u64,
    },
    Themes(themes::Msg),
    Plugins(plugins::Msg),
    Settings(settings::Msg),
}

/// State every page reads: the config file, the key reference and the
/// optional pages turned on.
pub struct Shared {
    pub store: Arc<Store>,
    pub reference: Reference,
    pub pages: Vec<&'static str>,
}

type Job = Box<dyn FnOnce() -> Msg + Send>;
/// A finished job's message, and whether the job was a write.
type Done = (Msg, bool);
type After = Box<dyn FnOnce() -> Result<()>>;
type ExecDone = Box<dyn FnOnce(Result<()>) -> Msg>;

enum Effect {
    Status(String, bool),
    QuitThen(After),
    Goto(usize, Option<Msg>),
    ShowPage(&'static str, bool),
    Spawn(Job),
    Write(Job),
    Timer(Duration, Msg),
    Exec(Command, ExecDone),
    Send(Msg),
}

/// What a page can ask of the app while it handles a key or a message.
pub struct Ctx<'a> {
    pub sh: &'a Shared,
    fx: Vec<Effect>,
}

impl Ctx<'_> {
    pub fn status(&mut self, text: impl Into<String>) {
        self.fx.push(Effect::Status(text.into(), false));
    }

    pub fn error(&mut self, err: impl std::fmt::Display) {
        self.fx.push(Effect::Status(format!("{err:#}"), true));
    }

    /// Runs job on a thread; its Msg comes back to every page.
    pub fn spawn(&mut self, job: impl FnOnce() -> Msg + Send + 'static) {
        self.fx.push(Effect::Spawn(Box::new(job)));
    }

    /// Runs job, which writes a file, after every write queued before it. The
    /// popup waits for writes before it exits.
    pub fn write(&mut self, job: impl FnOnce() -> Msg + Send + 'static) {
        self.fx.push(Effect::Write(Box::new(job)));
    }

    /// Delivers msg to every page after delay.
    pub fn after(&mut self, delay: Duration, msg: Msg) {
        self.fx.push(Effect::Timer(delay, msg));
    }

    /// Delivers msg to every page next.
    pub fn send(&mut self, msg: Msg) {
        self.fx.push(Effect::Send(msg));
    }

    /// Closes the popup and runs then afterwards, with the terminal restored.
    pub fn quit_then(&mut self, then: impl FnOnce() -> Result<()> + 'static) {
        self.fx.push(Effect::QuitThen(Box::new(then)));
    }

    /// Switches to the named page and then delivers msg to it.
    pub fn goto(&mut self, page: &str, msg: Option<Msg>) {
        if let Some(i) = PAGES.iter().position(|p| *p == page) {
            self.fx.push(Effect::Goto(i, msg));
        }
    }

    /// Turns an optional page on or off, for this popup and the next ones.
    pub fn show_page(&mut self, page: &'static str, on: bool) {
        self.fx.push(Effect::ShowPage(page, on));
    }

    /// Delivers Msg::PendingSave for pick seq once PICK_DELAY has passed.
    pub fn debounced(&mut self, seq: u64) {
        self.after(values::PICK_DELAY, Msg::PendingSave { seq });
    }

    /// Runs cmd on the terminal, with the popup suspended until it exits.
    pub fn exec(&mut self, cmd: Command, done: impl FnOnce(Result<()>) -> Msg + 'static) {
        self.fx.push(Effect::Exec(cmd, Box::new(done)));
    }

    /// Opens path in the user's editor with the popup suspended, then passes
    /// done the file's text or why the edit failed.
    pub fn edit_file(&mut self, path: PathBuf, done: impl FnOnce(Result<String>) -> Msg + 'static) {
        match crate::proc::editor_command(&path) {
            Ok(cmd) => self.exec(cmd, move |ok| {
                let text = ok.and_then(|()| std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display())));
                done(text.context("editor"))
            }),
            Err(e) => self.send(done(Err(e.context("editor")))),
        }
    }

    /// Applies edit, which changes key, to the config in the background.
    pub fn save(&mut self, what: impl Into<String>, key: String, edit: impl FnOnce(&mut Doc) -> Result<()> + Send + 'static) {
        let (store, what) = (self.sh.store.clone(), what.into());
        self.write(move || Msg::Saved(save(&store, what, Some(key), edit)));
    }
}

/// Loads the config for a page, recording the failure in err; a page then
/// shows an empty document with the error above it.
pub fn load_doc(store: &Store, err: &mut Option<String>) -> Doc {
    match store.load() {
        Ok(doc) => {
            *err = None;
            doc
        }
        Err(e) => {
            *err = Some(format!("{e:#}"));
            Doc::default()
        }
    }
}

/// Saves through store.
fn save(store: &Store, what: String, key: Option<String>, edit: impl FnOnce(&mut Doc) -> Result<()>) -> Saved {
    // Decoded here, off the UI thread, and before the lock is let go, so it
    // is exactly what the file holds.
    let mut config = None;
    let edit = |d: &mut Doc| {
        edit(d)?;
        config = Some(d.decoded());
        Ok(())
    };
    match store.update(edit) {
        Ok(result) => Saved { what, key, result, err: None, config },
        Err(e) => Saved { what, key, err: Some(e), ..Default::default() },
    }
}

/// One line of a page's key sheet; an empty key starts a group.
pub type Hint = (&'static str, &'static str);

/// One tab of the popup.
pub trait Page {
    fn title(&self) -> &'static str;
    fn init(&mut self, _ctx: &mut Ctx) {}
    /// Runs each time the page becomes the visible one.
    fn activated(&mut self, _ctx: &mut Ctx) {}
    fn update(&mut self, _msg: &mut Msg, _ctx: &mut Ctx) {}
    fn key(&mut self, key: &Key, ctx: &mut Ctx);
    fn view(&mut self, sh: &Shared, width: usize, height: usize) -> Vec<Line<'static>>;
    fn help(&self) -> String;
    /// A text field, prompt or sub-view needs every key, including the ones
    /// that otherwise switch pages or quit.
    fn capturing(&self) -> bool;
    /// Takes a key that would otherwise switch pages.
    fn claims(&self, _key: &Key) -> bool {
        false
    }
    /// Steps back inside the page on esc; false closes the popup.
    fn back(&mut self) -> bool {
        false
    }
    /// The keys listed under ?.
    fn sheet(&self) -> Vec<Hint> {
        Vec::new()
    }
}

pub struct App {
    sh: Shared,
    pages: Vec<Box<dyn Page>>,
    /// Which pages are on the tab bar, in PAGES order.
    visible: Vec<bool>,
    active: usize,
    status: String,
    status_err: bool,
    /// Shows every key of the current page in place of its body.
    sheet: bool,
    quit: bool,
    /// Quitting, with the last writes still running.
    closing: bool,
    /// The first write that failed while closing, shown again when the close
    /// is called off.
    close_failed: Option<String>,
    /// Ctrl+C while closing: leave without waiting for the writes.
    abandon: bool,
    after: Option<After>,
    /// Messages to deliver before the next frame.
    queue: VecDeque<Msg>,
    jobs: Vec<Job>,
    writes: Vec<Job>,
    timers: Vec<(Instant, Msg)>,
    execs: Vec<(Command, ExecDone)>,
}

impl App {
    pub fn new(tab: &str) -> App {
        let sh = Shared { store: Arc::new(Store::default_store()), reference: schema::bundled(), pages: load_pages() };
        // In PAGES order.
        let pages: Vec<Box<dyn Page>> = vec![Box::new(settings::SettingsPage::new()), Box::new(keys::KeysPage::new()), Box::new(themes::ThemesPage::new()), Box::new(plugins::PluginsPage::new())];
        let active = PAGES.iter().position(|p| p.eq_ignore_ascii_case(tab)).unwrap_or(0);
        let visible = PAGES.iter().map(|p| !OPTIONAL.contains(p) || sh.pages.contains(p)).collect();
        let mut app = App {
            sh,
            pages,
            visible,
            active,
            status: String::new(),
            status_err: false,
            sheet: false,
            quit: false,
            closing: false,
            close_failed: None,
            abandon: false,
            after: None,
            queue: VecDeque::new(),
            jobs: Vec::new(),
            writes: Vec::new(),
            timers: Vec::new(),
            execs: Vec::new(),
        };
        app.apply(vec![Effect::Spawn(Box::new(|| Msg::Reference(schema::for_installed()))), Effect::Spawn(Box::new(load_plugins))]);
        app.each_page(|page, ctx| page.init(ctx));
        app.switch(active);
        app
    }

    /// The pages on the tab bar, in order.
    fn shown(&self) -> Vec<usize> {
        (0..self.pages.len()).filter(|&i| self.visible[i]).collect()
    }

    /// Shows page i, even a hidden one: asking for a page by name shows it
    /// for this popup.
    fn switch(&mut self, i: usize) {
        self.visible[i] = true;
        self.active = i;
        self.with_page(i, |page, ctx| page.activated(ctx));
    }

    /// Runs f on page i with a fresh Ctx and applies its effects.
    fn with_page(&mut self, i: usize, f: impl FnOnce(&mut dyn Page, &mut Ctx)) {
        let mut ctx = Ctx { sh: &self.sh, fx: Vec::new() };
        f(self.pages[i].as_mut(), &mut ctx);
        let fx = ctx.fx;
        self.apply(fx);
    }

    /// Runs f on every page, then applies all their effects at once.
    fn each_page(&mut self, mut f: impl FnMut(&mut dyn Page, &mut Ctx)) {
        let mut fx = Vec::new();
        for page in &mut self.pages {
            let mut ctx = Ctx { sh: &self.sh, fx: Vec::new() };
            f(page.as_mut(), &mut ctx);
            fx.extend(ctx.fx);
        }
        self.apply(fx);
    }

    fn apply(&mut self, fx: Vec<Effect>) {
        for e in fx {
            match e {
                Effect::Status(text, err) => (self.status, self.status_err) = (text, err),
                Effect::QuitThen(f) => {
                    self.after = Some(f);
                    self.quit = true;
                }
                Effect::Goto(i, msg) => {
                    self.switch(i);
                    if let Some(mut msg) = msg {
                        self.with_page(i, |page, ctx| page.update(&mut msg, ctx));
                    }
                }
                Effect::ShowPage(page, on) => self.show_page(page, on),
                Effect::Spawn(job) => self.jobs.push(job),
                Effect::Write(job) => self.writes.push(job),
                Effect::Timer(delay, msg) => self.timers.push((Instant::now() + delay, msg)),
                Effect::Exec(cmd, done) => self.execs.push((cmd, done)),
                Effect::Send(msg) => self.queue.push_back(msg),
            }
        }
    }

    fn show_page(&mut self, page: &'static str, on: bool) {
        let Some(i) = PAGES.iter().position(|p| *p == page) else { return };
        self.visible[i] = on;
        self.sh.pages.retain(|p| *p != page);
        if on {
            self.sh.pages.push(page);
        }
        let text: String = OPTIONAL.iter().filter(|p| self.sh.pages.contains(p)).map(|p| format!("{p}\n")).collect();
        self.writes.push(Box::new(move || Msg::PagesSaved(crate::fsutil::atomic_write(&pages_file(), text.as_bytes(), 0o644))));
        let title = self.pages[i].title();
        (self.status, self.status_err) = (format!("{title} page {}", if on { "on" } else { "off" }), false);
    }

    /// Delivers msg to the app and then to every page.
    pub fn dispatch(&mut self, mut msg: Msg) {
        match &mut msg {
            Msg::Reference(r) => self.sh.reference = std::mem::take(r),
            Msg::Saved(saved) => (self.status, self.status_err) = saved.describe(),
            Msg::PagesSaved(Err(e)) => (self.status, self.status_err) = (format!("pages not saved: {e:#}"), true),
            _ => {}
        }
        self.each_page(|page, ctx| page.update(&mut msg, ctx));
    }

    /// Delivers queued messages.
    pub fn settle(&mut self) {
        while let Some(msg) = self.queue.pop_front() {
            self.dispatch(msg);
        }
    }

    pub fn key(&mut self, key: &Key) {
        // Only ctrl+c counts while closing: a pick made now would wait on a
        // delay the close does not wait for. Leaving at once also drops what
        // was to run after closing, which could wait on the same writes.
        if self.closing {
            if key.is("ctrl+c") {
                (self.abandon, self.after) = (true, None);
            }
            return;
        }
        self.status.clear();
        if key.is("ctrl+c") {
            self.quit = true;
            return;
        }
        // Any key closes the key sheet.
        if self.sheet {
            self.sheet = false;
            return;
        }
        let page = &mut self.pages[self.active];
        if !page.claims(key) && !page.capturing() {
            let shown = self.shown();
            let at = shown.iter().position(|&i| i == self.active).unwrap_or(0);
            let n = shown.len();
            match key.name.as_str() {
                "esc" => {
                    if !self.pages[self.active].back() {
                        self.quit = true;
                    }
                    return;
                }
                "q" => {
                    self.quit = true;
                    return;
                }
                "?" if !self.pages[self.active].sheet().is_empty() => {
                    self.sheet = true;
                    return;
                }
                // Arrows belong to the page; only Tab and the digits switch.
                "tab" => return self.switch(shown[(at + 1) % n]),
                "shift+tab" => return self.switch(shown[(at + n - 1) % n]),
                "1" | "2" | "3" | "4" => {
                    if let Some(&i) = shown.get((key.name.as_bytes()[0] - b'1') as usize) {
                        self.switch(i);
                    }
                    return;
                }
                _ => {}
            }
        }
        self.with_page(self.active, |page, ctx| page.key(key, ctx));
    }

    pub fn view(&mut self, width: usize, height: usize) -> Vec<Line<'static>> {
        let mut tabs = Vec::new();
        for (n, i) in self.shown().into_iter().enumerate() {
            let label = format!(" {} {} ", n + 1, self.pages[i].title());
            tabs.push(if i == self.active { styled(TAB_ACTIVE, label) } else { dim(label) });
        }
        let hidden: Vec<&str> = (0..self.pages.len()).filter(|&i| !self.visible[i]).map(|i| self.pages[i].title()).collect();
        if !hidden.is_empty() {
            tabs.push(dim(format!("  {} can be turned on in Settings", hidden.join(" and "))));
        }
        let page = &mut self.pages[self.active];
        // Tab leads, so a narrow popup clips the page's own keys instead.
        let mut help = page.help();
        if !page.capturing() {
            help = format!("tab next page · {help} · esc close");
        }
        let status = match (self.status.is_empty(), self.status_err) {
            _ if self.closing => line(dim("saving… ctrl+c closes without waiting")),
            (true, _) => Line::default(),
            (false, true) => line(error(format!("✗ {}", self.status))),
            (false, false) => line(ok(self.status.clone())),
        };
        let body_height = height.saturating_sub(4).max(1);
        let body = if self.sheet && !page.sheet().is_empty() {
            help = "any key closes".into();
            render_sheet(page.title(), &page.sheet())
        } else {
            page.view(&self.sh, width, body_height)
        };
        let mut lines = vec![Line::from(tabs)];
        lines.extend(fit(body, width, body_height));
        lines.push(clip(status, width));
        lines.push(clip(line(dim(help)), width));
        lines
    }

    /// Runs queued jobs on threads and queued writes on writer, sending
    /// their results to tx. Returns how many jobs and writes it started.
    fn start_jobs(&mut self, tx: &Sender<Done>, writer: &Sender<Job>) -> (usize, usize) {
        let n = (self.jobs.len(), self.writes.len());
        for job in self.jobs.drain(..) {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send((job(), false));
            });
        }
        for job in self.writes.drain(..) {
            let _ = writer.send(job);
        }
        n
    }

    /// Delivers a write's result, noting a failure that happens while closing.
    fn written(&mut self, msg: Msg) {
        self.dispatch(msg);
        if self.closing && self.status_err {
            self.close_failed.get_or_insert_with(|| self.status.clone());
        }
    }

    /// Whether the popup may close with `writing` writes still running. A write
    /// that failed while closing cancels the close, and what was to run after
    /// it, so its error stays on screen.
    fn ready_to_close(&mut self, writing: usize) -> bool {
        if writing > 0 {
            return false;
        }
        if let Some(failed) = self.close_failed.take() {
            (self.quit, self.closing, self.after) = (false, false, None);
            (self.status, self.status_err) = (failed, true);
            return false;
        }
        true
    }

    /// Starts closing, or keeps it going, by saving every pick still waiting
    /// on its delay.
    fn shutdown(&mut self) {
        if !self.closing {
            (self.closing, self.status_err) = (true, false);
        }
        let picks: Vec<_> = self.timers.extract_if(.., |(_, msg)| matches!(msg, Msg::PendingSave { .. })).collect();
        for (_, msg) in picks {
            self.dispatch(msg);
        }
        self.settle();
    }

    /// Delivers timers that are due. Returns whether any were, and the time
    /// to the next one.
    fn fire_timers(&mut self) -> (bool, Option<Duration>) {
        let now = Instant::now();
        let due: Vec<_> = self.timers.extract_if(.., |(at, _)| *at <= now).collect();
        let fired = !due.is_empty();
        for (_, msg) in due {
            self.dispatch(msg);
        }
        (fired, self.timers.iter().map(|(at, _)| at.saturating_duration_since(now)).min())
    }
}

fn render_sheet(title: &str, hints: &[Hint]) -> Vec<Line<'static>> {
    let w = hints.iter().map(|(k, _)| width(k)).max().unwrap_or(0).max(3);
    let mut lines = vec![line(heading(format!("Keys · {title}"))), Line::default()];
    let hint = |k: &str, what: &str| Line::from(vec![raw("  "), accent(pad(k, w)), raw(format!("  {what}"))]);
    lines.push(hint("tab", "next page (shift+tab back, 1–4 jump)"));
    lines.push(Line::default());
    for (k, what) in hints {
        if k.is_empty() {
            lines.push(Line::default());
            lines.push(line(dim(*what)));
        } else {
            lines.push(hint(k, what));
        }
    }
    lines.push(Line::default());
    lines.push(hint("esc", "close (q too, except while typing a search)"));
    lines
}

fn load_plugins() -> Msg {
    Msg::PluginList(crate::plugins::list())
}

/// Shows the popup until the user leaves it.
pub fn run(tab: &str) -> Result<()> {
    let mut app = App::new(tab);
    let (tx, rx) = mpsc::channel();
    // Installed once: enter() runs again after every editor exit, and a hook
    // per run would chain a leave() for each of them.
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = leave();
        hook(info);
    }));
    let mut term = enter()?;
    let (writer, done) = start_writer(tx.clone());
    let result = event_loop(&mut app, &mut term, &tx, &rx, &writer);
    // An exit while a write runs could leave it half done, so the writes
    // finish first unless ctrl+c said not to wait.
    drop(writer);
    if !app.abandon {
        let _ = done.join();
    }
    leave()?;
    result?;
    match app.after.take() {
        Some(after) => after(),
        None => Ok(()),
    }
}

type Term = Terminal<CrosstermBackend<io::Stdout>>;

fn enter() -> Result<Term> {
    terminal::enable_raw_mode()?;
    execute!(io::stdout(), terminal::EnterAlternateScreen, event::EnableBracketedPaste)?;
    Ok(Terminal::new(CrosstermBackend::new(io::stdout()))?)
}

fn leave() -> Result<()> {
    execute!(io::stdout(), event::DisableBracketedPaste, terminal::LeaveAlternateScreen, crossterm::cursor::Show)?;
    terminal::disable_raw_mode()?;
    Ok(())
}

/// Runs writes one at a time in the order they arrive, so a later save never
/// lands before an earlier one; the thread ends once the sender is dropped.
fn start_writer(tx: Sender<Done>) -> (Sender<Job>, std::thread::JoinHandle<()>) {
    let (writer, writes) = mpsc::channel::<Job>();
    let done = std::thread::spawn(move || {
        for job in writes {
            let _ = tx.send((job(), true));
        }
    });
    (writer, done)
}

fn event_loop(app: &mut App, term: &mut Term, tx: &Sender<Done>, rx: &Receiver<Done>, writer: &Sender<Job>) -> Result<()> {
    let (mut running, mut writing) = (0, 0);
    let mut dirty = true;
    loop {
        let (fired, next_timer) = app.fire_timers();
        dirty |= fired;
        app.settle();
        let (jobs, writes) = app.start_jobs(tx, writer);
        (running, writing) = (running + jobs + writes, writing + writes);
        if app.quit {
            app.shutdown();
            let (jobs, writes) = app.start_jobs(tx, writer);
            (running, writing) = (running + jobs + writes, writing + writes);
        }
        if app.closing && (app.abandon || app.ready_to_close(writing)) {
            return Ok(());
        }
        if dirty {
            term.draw(|f| {
                let area = f.area();
                let lines = app.view(area.width as usize, area.height as usize);
                f.render_widget(Paragraph::new(lines), area);
            })?;
            dirty = false;
        }
        // Run programs such as the editor with the popup suspended.
        if let Some((mut cmd, done)) = app.execs.pop() {
            leave()?;
            let status: io::Result<ExitStatus> = cmd.status();
            enter()?;
            term.clear()?;
            let ok = match status {
                Ok(s) if s.success() => Ok(()),
                Ok(s) => Err(anyhow::anyhow!("exited with {s}")),
                Err(e) => Err(e.into()),
            };
            app.dispatch(done(ok));
            dirty = true;
            continue;
        }
        // Wait for a key, a timer, or (checked often) background work.
        let mut wait = next_timer.unwrap_or(Duration::from_secs(1));
        if running > 0 {
            wait = wait.min(Duration::from_millis(10));
        }
        if event::poll(wait)? {
            match event::read()? {
                Event::Key(k) if let Some(key) = Key::from_event(k) => {
                    app.key(&key);
                    dirty = true;
                }
                Event::Paste(text) => {
                    app.key(&Key { paste: Some(text), ..Default::default() });
                    dirty = true;
                }
                Event::Resize(..) => dirty = true,
                _ => {}
            }
        }
        while let Ok((msg, write)) = rx.try_recv() {
            running -= 1;
            if write {
                writing -= 1;
                app.written(msg);
            } else {
                app.dispatch(msg);
            }
            dirty = true;
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
