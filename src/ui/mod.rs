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

pub const PAGES: [&str; 4] = ["themes", "plugins", "keys", "settings"];

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

/// State every page reads: the config file and the key reference.
pub struct Shared {
    pub store: Arc<Store>,
    pub reference: Reference,
}

type Job = Box<dyn FnOnce() -> Msg + Send>;
type After = Box<dyn FnOnce() -> Result<()>>;
type ExecDone = Box<dyn FnOnce(Result<()>) -> Msg>;

enum Effect {
    Status(String, bool),
    QuitThen(After),
    Goto(usize, Option<Msg>),
    Spawn(Job),
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
        self.spawn(move || Msg::Saved(save(&store, what, Some(key), edit)));
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

/// Saves through store; Store keeps saves in order.
fn save(store: &Store, what: String, key: Option<String>, edit: impl FnOnce(&mut Doc) -> Result<()>) -> Saved {
    match store.update(edit) {
        Ok(result) => Saved { what, key, result, err: None },
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
    active: usize,
    status: String,
    status_err: bool,
    /// Shows every key of the current page in place of its body.
    sheet: bool,
    quit: bool,
    after: Option<After>,
    /// Messages to deliver before the next frame.
    queue: VecDeque<Msg>,
    jobs: Vec<Job>,
    timers: Vec<(Instant, Msg)>,
    execs: Vec<(Command, ExecDone)>,
}

impl App {
    pub fn new(tab: &str) -> App {
        let sh = Shared { store: Arc::new(Store::default_store()), reference: schema::bundled() };
        let pages: Vec<Box<dyn Page>> = vec![Box::new(themes::ThemesPage::new()), Box::new(plugins::PluginsPage::new()), Box::new(keys::KeysPage::new()), Box::new(settings::SettingsPage::new())];
        let active = PAGES.iter().position(|p| p.eq_ignore_ascii_case(tab)).unwrap_or(0);
        let mut app = App {
            sh,
            pages,
            active,
            status: String::new(),
            status_err: false,
            sheet: false,
            quit: false,
            after: None,
            queue: VecDeque::new(),
            jobs: Vec::new(),
            timers: Vec::new(),
            execs: Vec::new(),
        };
        app.apply(vec![Effect::Spawn(Box::new(|| Msg::Reference(schema::for_installed()))), Effect::Spawn(Box::new(load_plugins))]);
        app.each_page(|page, ctx| page.init(ctx));
        app.switch(active);
        app
    }

    fn switch(&mut self, i: usize) {
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
                Effect::Spawn(job) => self.jobs.push(job),
                Effect::Timer(delay, msg) => self.timers.push((Instant::now() + delay, msg)),
                Effect::Exec(cmd, done) => self.execs.push((cmd, done)),
                Effect::Send(msg) => self.queue.push_back(msg),
            }
        }
    }

    /// Delivers msg to the app and then to every page.
    pub fn dispatch(&mut self, mut msg: Msg) {
        match &mut msg {
            Msg::Reference(r) => self.sh.reference = std::mem::take(r),
            Msg::Saved(saved) => (self.status, self.status_err) = saved.describe(),
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
            let n = self.pages.len();
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
                "tab" => return self.switch((self.active + 1) % n),
                "shift+tab" => return self.switch((self.active + n - 1) % n),
                "1" | "2" | "3" | "4" => return self.switch((key.name.as_bytes()[0] - b'1') as usize),
                _ => {}
            }
        }
        self.with_page(self.active, |page, ctx| page.key(key, ctx));
    }

    pub fn view(&mut self, width: usize, height: usize) -> Vec<Line<'static>> {
        let mut tabs = Vec::new();
        for (i, p) in self.pages.iter().enumerate() {
            let label = format!(" {} {} ", i + 1, p.title());
            tabs.push(if i == self.active { styled(TAB_ACTIVE, label) } else { dim(label) });
        }
        let page = &mut self.pages[self.active];
        let mut help = page.help();
        if !page.capturing() {
            help += " · tab next page · esc close";
        }
        let status = match (self.status.is_empty(), self.status_err) {
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

    /// Runs queued jobs on threads, sending their results to tx.
    fn start_jobs(&mut self, tx: &Sender<Msg>) -> usize {
        let n = self.jobs.len();
        for job in self.jobs.drain(..) {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(job());
            });
        }
        n
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
    for (k, what) in hints {
        if k.is_empty() {
            lines.push(Line::default());
            lines.push(line(dim(*what)));
        } else {
            lines.push(hint(k, what));
        }
    }
    lines.push(Line::default());
    lines.push(hint("tab", "next page (shift+tab back, 1–4 jump)"));
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
    let result = event_loop(&mut app, &mut term, &tx, &rx);
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

fn event_loop(app: &mut App, term: &mut Term, tx: &Sender<Msg>, rx: &Receiver<Msg>) -> Result<()> {
    let mut running = 0;
    let mut dirty = true;
    loop {
        let (fired, next_timer) = app.fire_timers();
        dirty |= fired;
        app.settle();
        running += app.start_jobs(tx);
        if app.quit {
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
        while let Ok(msg) = rx.try_recv() {
            running -= 1;
            app.dispatch(msg);
            dirty = true;
        }
    }
}

#[cfg(test)]
mod tests;
