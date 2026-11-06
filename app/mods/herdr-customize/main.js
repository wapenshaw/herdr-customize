// herdr-customize's screen in the Herdr app: themes, keys and mods. Every read
// and write goes through the herdr-customize program (`app api`), so this file
// keeps nothing worth saving.
import { View, div, image, uniform_list } from "gpui-kit";
import { h_flex, v_flex, InputState, Input, TextView, VirtualListScrollHandle, v_virtual_list } from "gpui-base";
import { exec, home, appHome, theme, themes, settings, keys, commands, run, props, closeOverlay, window } from "herdr";

// `herdr-customize app install` fills this in; a copy added by hand looks.
const BIN = "@BIN@";
const REPO = "https://github.com/wapenshaw/herdr-customize";
const TABS = [["themes", "Themes"], ["keys", "Keys"], ["mods", "Mods"]];
const FILTERS = [["all", "All", () => true], ["dark", "Dark", r => !r.light], ["light", "Light", r => r.light], ["added", "In Settings", r => r.added]];
const MODIFIER_KEYS = ["shift", "control", "ctrl", "alt", "platform", "cmd", "super", "fn", "function", "meta"];
const plural = (n, word) => `${n} ${word}${n === 1 ? "" : "s"}`;
const reason = e => String(e?.message ?? e).replace(/^Error: /, "");
const value = input => input.value().trim();
const commandOf = action => action.split(":")[0];
const keyRowId = (section, key) => `k:${section}:${key}`;
const platform = () => window()?.platform;
// Ctrl, Alt or Cmd held: the key is a shortcut, not Tab or list navigation.
const chord = m => m.control || m.alt || m.platform;
// The app's keymap outlives this screen, so the keys are bound once.
let keysBound = false;
// The forms the program reviews: owner/repo[/folder], or a GitHub link to either.
const looksLikeRepo = text => /^(https?:\/\/)?(github\.com\/)?[\w.-]+\/[\w.-]+(\/.*)?$/.test(text.trim());

export default class Customize extends View {
  init(_props, cx) {
    this.tab = "themes";
    this.handle = cx.focus_handle();
    this.busy = "";
    this.error = "";
    this.notice = "";

    this.palettes = [];
    this.filter = "all";
    const cur = this.current();
    this.themeSel = cur[cur.showing] ?? "";
    this.themeScroll = VirtualListScrollHandle.new();
    this.themeSearch = InputState.new({ placeholder: "Search themes" });
    this.themeSearch.on("change", (_e, cx) => { this.themeSel = ""; cx.notify(); });

    this.keySearch = InputState.new({ placeholder: "Search keys and commands" });
    this.keySearch.on("change", (_e, cx) => cx.notify());
    this.sectionInput = InputState.new({ placeholder: "global" });
    this.keyInput = InputState.new({ placeholder: "ctrl-shift-k" });
    this.actionInput = InputState.new({ placeholder: "tab.new" });
    for (const input of [this.sectionInput, this.keyInput, this.actionInput]) {
      input.on("change", (_e, cx) => this.checkKey(cx));
      input.on("submit", (_e, cx) => this.saveKey(cx));
    }
    this.form = null;
    this.check = null;
    this.checkSeq = 0;
    this.recording = false;

    this.modView = "installed";
    this.packages = null;
    this.market = null;
    this.topic = "mods";
    this.sort = "stars";
    this.page = 1;
    this.marketSearch = InputState.new({ placeholder: "Search, or owner/repo to review" });
    this.marketSearch.on("submit", (_e, cx) => {
      const text = value(this.marketSearch);
      if (looksLikeRepo(text)) this.review(text, cx);
      else { this.page = 1; this.loadMarket(cx); }
    });
    this.reviewed = null;

    if (!keysBound) {
      try {
        cx.bind_keys([
          { keystroke: "tab", action: "customize.next_screen", context: "Customize" },
          { keystroke: "shift-tab", action: "customize.prev_screen", context: "Customize" },
        ]);
      } catch {}
      keysBound = true;
    }

    // The app's checker has no window, and a binary started there keeps the
    // checker's output open after it ends, which stalls `app install`.
    this.binFound = window() ? this.findBin() : Promise.resolve(null);
    if (window()) this.loadAll(cx);
    this.themeSearch.focus();
    cx.timer.after(300, cx => {
      this.themeSearch.focus();
      const at = this.themeRows().findIndex(r => r.name === this.themeSel);
      if (at >= 0) this.themeScroll.scroll_to_item(at, "center");
      cx.notify();
    });
  }

  // The first program that answers: BIN, else the one in a checkout at
  // ~/.local/share/herdr-customize, which keeps the palettes beside it, else
  // one on PATH. The app's PATH can be shorter than a shell's.
  async findBin() {
    if (!BIN.startsWith("@")) return BIN;
    const name = `herdr-customize${platform() === "windows" ? ".exe" : ""}`;
    this.stale = "";
    for (const bin of [`${home()}/.local/share/herdr-customize/target/release/${name}`, name]) {
      try {
        // The probe is the packages read, so its answer is kept.
        const r = await exec(bin, ["app", "--home", appHome(), "api", "packages"]);
        if (r.code === 0) {
          try { const data = JSON.parse(r.stdout); if (!data.error) this.packages = data; } catch {}
          return bin;
        }
        this.stale = bin;
      } catch {}
    }
    return null;
  }

  loadAll(cx) {
    cx.spawn(async cx => {
      this.bin = await this.binFound;
      if (this.bin) {
        this.load(cx, ["palettes"], data => { this.palettes = data.palettes; });
        if (!this.packages) this.refreshPackages(cx);
      }
      cx.notify();
    });
  }

  refreshPackages(cx) {
    this.load(cx, ["packages"], data => { this.packages = data; });
  }

  async call(args) {
    const bin = await this.binFound;
    if (!bin) throw new Error("herdr-customize is not installed");
    const r = await exec(bin, ["app", "--home", appHome(), "api", ...args]);
    let data;
    try { data = JSON.parse(r.stdout); } catch { throw new Error(r.stderr.trim() || `herdr-customize exited with ${r.code}`); }
    if (data.error) throw new Error(data.error);
    return data;
  }

  // A background read: it shows its failure but blocks no button.
  load(cx, args, done) {
    cx.spawn(async cx => {
      try { done(await this.call(args), cx); } catch (e) { this.error = reason(e); }
      cx.notify();
    });
  }

  // A change the person asked for: one at a time, named while it runs.
  task(cx, label, args, done) {
    if (this.busy) return;
    this.busy = label;
    this.error = "";
    this.notice = "";
    cx.notify();
    cx.spawn(async cx => {
      try { await done(await this.call(args), cx); } catch (e) { this.error = reason(e); }
      this.busy = "";
      cx.notify();
    });
  }

  close() {
    if (props()?.overlay) closeOverlay(props().overlay);
  }

  stepTab(by, cx) {
    const at = TABS.findIndex(([id]) => id === this.tab);
    this.showTab(TABS[(at + by + TABS.length) % TABS.length][0], cx);
  }

  showTab(tab, cx) {
    this.tab = tab;
    this.error = "";
    this.notice = "";
    this.recording = false;
    this.focusPage();
    if (tab === "mods" && this.modView === "browse" && !this.market) this.loadMarket(cx);
    cx.notify();
  }

  // A box that isn't drawn takes keys and gives none back, Tab included.
  focusPage() {
    const box = { themes: this.themeSearch, keys: this.keySearch, mods: this.modView === "browse" && !this.reviewed ? this.marketSearch : null }[this.tab];
    if (box) box.focus(); else this.handle.focus();
  }

  closeReview(cx) {
    this.reviewed = null;
    this.focusPage();
    cx.notify();
  }

  onKey(e, cx) {
    if (this.recording) {
      const key = this.keyString(e);
      if (e.key === "escape") this.recording = false;
      else if (key) {
        this.recording = false;
        this.keyInput.set_value(key);
        this.checkKey(cx);
      }
      cx.stop_propagation();
      cx.notify();
      return;
    }
    if (e.key === "escape") {
      if (this.reviewed) this.closeReview(cx); else this.close();
      return;
    }
    if (chord(e.modifiers)) return;
    if (e.key === "tab") {
      this.stepTab(e.modifiers.shift ? -1 : 1, cx);
      cx.stop_propagation();
      return;
    }
    if (this.tab !== "themes") return;
    const rows = this.themeRows();
    const at = Math.max(0, rows.findIndex(r => r.name === this.themeSel));
    const move = { up: -1, down: 1, pageup: -10, pagedown: 10 }[e.key];
    if (move && rows.length) {
      const next = Math.min(rows.length - 1, Math.max(0, at + move));
      this.themeSel = rows[next].name;
      this.themeScroll.scroll_to_item(next);
      cx.notify();
    } else if (e.key === "enter" && rows[at]) this.useTheme(rows[at], cx);
  }

  // The key as keys.json spells it: the platform key is cmd on macOS, and
  // elsewhere the app writes ctrl for both.
  keyString(e) {
    const key = (e.key ?? "").toLowerCase();
    if (!key || MODIFIER_KEYS.includes(key)) return null;
    const m = e.modifiers;
    const mac = platform() === "macos";
    return [m.control || (!mac && m.platform) ? "ctrl" : "", m.alt ? "alt" : "", m.shift ? "shift" : "", mac && m.platform ? "cmd" : "", key].filter(Boolean).join("-");
  }

  button(id, text, onClick, { primary = false, enabled = true, danger = false, always = false } = {}) {
    const t = theme();
    const live = enabled && (always || !this.busy);
    return h_flex().id(id).h(30).px(12).gap(6).items_center().justify_center().rounded(7).flex_none()
      .bg(primary ? t.solid : t.raised).text_color(primary ? t.solid_foreground : danger ? t.blocked : t.foreground)
      .text_size(12).font_medium().opacity(live ? 1 : 0.42)
      .when(live, el => el.cursor_pointer().hover(s => s.opacity(0.84)).on_click(onClick))
      .child(text);
  }

  chip(id, text, active, onClick) {
    const t = theme();
    return div().id(id).px(9).py(4).rounded(6).text_size(11.5).cursor_pointer().flex_none()
      .bg(active ? t.selected : t.surface).text_color(active ? t.foreground : t.muted)
      .hover(s => s.bg(t.hover)).on_click(onClick).child(text);
  }

  segment(prefix, options, current, pick) {
    return options.map(([id, text]) => this.chip(`${prefix}-${id}`, text, current === id, (_e, cx) => pick(id, cx)));
  }

  split(width, left, right) {
    return h_flex().flex_1().min_h(0).gap(16).items_start()
      .child(v_flex().w(width).h_full().flex_none().min_h(0).gap(8).children(left))
      .child(right);
  }

  badge(text, color) {
    return div().px(6).py(2).rounded(5).bg(`${color}22`).text_size(10.5).font_medium().text_color(color).flex_none().child(text);
  }

  input(state) {
    const t = theme();
    return Input.new(state).h(34).px(10).rounded(8).border_1().border_color(t.border).bg(t.background).text_size(12.5).text_color(t.foreground);
  }

  label(text) {
    return div().text_size(11).font_medium().text_color(theme().muted).child(text);
  }

  heading(text) {
    return div().text_size(11).font_semibold().text_color(theme().dim).mt(6).child(text.toUpperCase());
  }

  empty(text) {
    const t = theme();
    return div().p(14).rounded(8).bg(t.surface).text_size(12).text_color(t.muted).child(text);
  }

  themeRows() {
    const out = [];
    const added = new Set();
    for (const th of themes()) {
      const c = th.colors;
      added.add(th.name);
      out.push({ name: th.name, light: th.light, added: true, bg: c.background, fg: c.terminal_foreground ?? c.foreground, ansi: [...Array(16).keys()].map(i => c[`ansi${i}`]) });
    }
    for (const p of this.palettes) {
      if (added.has(p.name) || p.error) continue;
      out.push({ name: p.name, light: !p.dark, added: false, bg: p.background, fg: p.foreground, ansi: p.ansi });
    }
    const query = value(this.themeSearch).toLowerCase();
    const keep = FILTERS.find(([id]) => id === this.filter)[2];
    return out.filter(r => (!query || r.name.includes(query)) && keep(r));
  }

  current() {
    const s = settings();
    return { dark: s.theme_dark, light: s.theme_light, showing: s.dark ? "dark" : "light" };
  }

  useTheme(row, cx) {
    const use = () => {
      run("app.theme", { name: row.name });
      this.notice = `${row.name} is now your ${row.light ? "light" : "dark"} theme.`;
    };
    if (row.added) { use(); cx.notify(); return; }
    // The app picks up a new theme file on its next look at the folder.
    this.addTheme(row, cx, (_d, cx) => cx.timer.after(400, cx => { use(); cx.notify(); }));
  }

  addTheme(row, cx, done) {
    this.task(cx, `Adding ${row.name}`, ["theme-write", row.name], done);
  }

  closeAndRun(command, args) {
    this.close();
    run(command, args);
  }

  swatch(row, size) {
    return h_flex().size(size).rounded(4).overflow_hidden().bg(row.bg).border_1().border_color(theme().border).flex_none()
      .items_end().p(2).gap(1)
      .children([1, 2, 4, 5].map(i => div().flex_1().h(size / 3).rounded(1).bg(row.ansi[i] ?? row.fg)));
  }

  themesPage() {
    const t = theme();
    const rows = this.themeRows();
    const cur = this.current();
    const sel = rows.find(r => r.name === this.themeSel) ?? rows[0];
    const list = v_virtual_list("theme-list", rows.length, 34, i => rows[i].name, (range, _cx) => {
      const out = [];
      for (let i = range.start; i < range.end; i++) {
        const r = rows[i];
        const on = r.name === sel?.name;
        out.push(h_flex().h(34).px(9).gap(9).items_center().rounded(6).bg(on ? t.selected : t.popover)
          .child(this.swatch(r, 20))
          .child(div().flex_1().min_w(0).text_size(12.5).text_ellipsis().whitespace_nowrap().text_color(r.added ? t.foreground : t.muted).child(r.name))
          .when(cur.dark === r.name, el => el.child(this.badge("dark", t.accent)))
          .when(cur.light === r.name, el => el.child(this.badge("light", t.accent))));
      }
      return out;
    }).track_scroll(this.themeScroll).size_full()
      .on_item_click((name, cx) => { this.themeSel = name; cx.notify(); });
    return this.split(300, [
      this.input(this.themeSearch),
      h_flex().gap(5).children(this.segment("filter", FILTERS, this.filter, (id, cx) => { this.filter = id; cx.notify(); })),
      v_flex().flex_1().min_h(0).child(list),
      div().text_size(10.5).text_color(t.dim).child(`${plural(rows.length, "theme")} · ↑↓ to browse, Enter to use`),
    ], sel ? this.themeDetail(sel, cur) : v_flex().flex_1().child(this.empty("No theme matches.")));
  }

  themeDetail(row, cur) {
    const t = theme();
    const slot = row.light ? "light" : "dark";
    const using = cur[slot] === row.name;
    const a = i => row.ansi[i] ?? row.fg;
    const line = (...parts) => h_flex().children(parts.map(([text, color]) => div().whitespace_nowrap().text_color(color ?? row.fg).child(text)));
    return v_flex().flex_1().min_w(0).gap(12)
      .child(h_flex().gap(8).items_center()
        .child(div().text_size(20).font_semibold().child(row.name))
        .child(this.badge(slot, t.muted))
        .child(this.badge(row.added ? "in Settings" : "not added yet", row.added ? t.done : t.dim)))
      .child(v_flex().p(16).gap(5).rounded(10).bg(row.bg).font_family(String(settings().font_family ?? "monospace")).text_size(12.5).border_1().border_color(t.border)
        .child(line(["~/work/herdr ", a(4)], ["on ", null], ["main ", a(5)], ["❯ ", a(2)], ["cargo test", null]))
        .child(line(["   Compiling ", a(2)], ["herdr-customize", null]))
        .child(line(["test keys::conflicts ... ", null], ["ok", a(2)]))
        .child(line(["test theme::apply ... ", null], ["FAILED", a(1)]))
        .child(line(["warning", a(3)], [": unused variable ", null], ["`x`", a(6)]))
        .child(line(["// bright black, for comments", a(8)]))
        .child(v_flex().mt(8).gap(3).children([0, 8].map(start => h_flex().gap(3)
          .children([...Array(8).keys()].map(i => div().flex_1().h(16).rounded(3).bg(a(start + i))))))))
      .child(h_flex().gap(8).items_center()
        .child(this.button("use-theme", using ? `Your ${slot} theme` : `Use as ${slot} theme`, (_e, cx) => this.useTheme(row, cx), { primary: true, enabled: !using }))
        .when(!row.added, el => el.child(this.button("add-theme", "Add to Settings only", (_e, cx) => this.addTheme(row, cx, () => { this.notice = `${row.name} is in Settings > Look.`; }))))
        .child(div().flex_1())
        .child(this.button("open-look", "Settings > Look", () => this.closeAndRun("app.settings", { section: "look" }))))
      .when(cur.showing !== slot, el => el.child(div().text_size(11.5).text_color(t.muted)
        .child(`The app is ${cur.showing} now, so a ${slot} theme shows when it switches to ${slot}.`)))
      .child(div().text_size(11).text_color(t.dim).child("The app keeps a dark and a light theme and follows your system between them. Only the themes in Settings appear there; the rest wait here until you use one."));
  }

  keyRows() {
    const query = value(this.keySearch).toLowerCase();
    const match = (...words) => !query || words.some(w => w.toLowerCase().includes(query));
    const all = keys();
    const sections = new Map();
    for (const k of all) sections.set(k.keys, (sections.get(k.keys) ?? 0) + 1);
    const bound = all.filter(k => match(k.keys, k.action, k.section));
    const used = new Set(all.map(k => commandOf(k.action)));
    const free = commands().filter(c => !used.has(c.name) && c.args.every(a => !a.required) && match(c.name, c.about));
    const rows = [];
    let section = null;
    for (const k of [...bound].sort((x, y) => (x.section === "global" ? "" : x.section).localeCompare(y.section === "global" ? "" : y.section))) {
      if (k.section !== section) rows.push({ id: `h:${k.section}`, header: k.section });
      section = k.section;
      rows.push({ id: keyRowId(k.section, k.keys), section: k.section, key: k.keys, action: k.action, shared: sections.get(k.keys) > 1 });
    }
    if (free.length) rows.push({ id: "h:free", header: "commands without a key" });
    for (const c of free) rows.push({ id: `c:${c.name}`, section: "global", key: "", action: c.name, about: c.about });
    return rows;
  }

  // A command without a key starts recording one.
  editKey(row, cx) {
    if (!row || row.header) return;
    this.openForm(row, !row.key, cx);
  }

  newKey(cx) {
    this.openForm({ section: "global", key: "", action: "" }, false, cx);
    this.actionInput.focus();
  }

  openForm(row, record, cx) {
    this.form = { old: row.key || null, section: row.section };
    this.sectionInput.set_value(row.section);
    this.keyInput.set_value(row.key);
    this.actionInput.set_value(row.action);
    this.recording = record;
    if (record) this.handle.focus();
    this.check = null;
    this.checkKey(cx);
  }

  section() {
    return value(this.sectionInput) || "global";
  }

  keyArgs() {
    const args = ["--section", this.section(), "--key", value(this.keyInput), "--action", value(this.actionInput)];
    if (this.form?.old) args.push("--old", this.form.old);
    return args;
  }

  // Only the check of the form as it is now counts: Enter can beat it, and an
  // older check may be of another key, with --replace for that key's holder.
  canSave() {
    return Boolean(this.form && value(this.keyInput) && value(this.actionInput) && this.check?.seq === this.checkSeq && !this.check.errors?.length);
  }

  checkKey(cx) {
    const seq = ++this.checkSeq;
    if (!value(this.keyInput)) { this.check = null; cx.notify(); return; }
    // Each check starts the program, so it waits for typing to pause.
    cx.timer.after(200, cx => {
      if (seq !== this.checkSeq) return;
      cx.spawn(async cx => {
        let check;
        try { check = await this.call(["key-check", ...this.keyArgs()]); } catch (e) { check = { errors: [reason(e)], warnings: [] }; }
        if (seq === this.checkSeq) { this.check = { ...check, seq }; cx.notify(); }
      });
    });
  }

  saveKey(cx) {
    if (!this.canSave()) return;
    const args = ["key-set", ...this.keyArgs()];
    if (this.check?.held) args.push("--replace");
    const key = value(this.keyInput);
    this.task(cx, "Saving the key", args, () => {
      this.form = { old: key, section: this.section() };
      this.notice = `${key} runs ${value(this.actionInput)}.`;
      this.check = null;
    });
  }

  removeKey(cx) {
    const { old, section } = this.form;
    this.task(cx, "Removing the key", ["key-remove", "--section", section, "--key", old], () => {
      this.notice = `${old} is unbound.`;
      this.form = null;
    });
  }

  keysPage() {
    const t = theme();
    const rows = this.keyRows();
    const selected = this.form?.old ? keyRowId(this.form.section, this.form.old) : null;
    const list = uniform_list("key-list", rows.length, i => rows[i].id, (range, _cx) => {
      const out = [];
      for (let i = range.start; i < range.end; i++) {
        const r = rows[i];
        out.push(r.header
          ? h_flex().h(30).px(4).items_end().pb(4).text_size(10.5).font_semibold().text_color(t.dim).child(r.header.toUpperCase())
          : h_flex().h(30).px(8).gap(10).items_center().rounded(6).bg(r.id === selected ? t.selected : t.popover)
            .child(div().w(150).flex_none().text_size(12).whitespace_nowrap().text_ellipsis().text_color(r.key ? t.foreground : t.dim).child(r.key ? `${r.key}${r.shared ? " ◆" : ""}` : "—"))
            .child(div().flex_1().min_w(0).text_size(12).whitespace_nowrap().text_ellipsis().text_color(t.muted).child(r.action)));
      }
      return out;
    }).size_full().on_item_click((id, cx) => this.editKey(rows.find(r => r.id === id), cx));
    return this.split(420, [
      h_flex().gap(8).child(div().flex_1().child(this.input(this.keySearch))).child(this.button("new-key", "New binding", (_e, cx) => this.newKey(cx))),
      v_flex().flex_1().min_h(0).child(list),
    ], this.form ? this.keyForm() : v_flex().flex_1().gap(10)
      .child(this.empty("Pick a binding to change it, or a command without a key to give it one."))
      .child(div().text_size(11).text_color(t.dim).child("Keys are saved to keys.json, and the app picks them up at once.")));
  }

  keyForm() {
    const t = theme();
    const check = this.check;
    const name = commandOf(value(this.actionInput));
    const about = commands().find(c => c.name === name)?.about;
    return v_flex().flex_1().min_w(0).gap(10)
      .child(div().text_size(18).font_semibold().child(this.form.old ? `Change ${this.form.old}` : "New binding"))
      .child(v_flex().gap(5).child(this.label("Key"))
        .child(h_flex().gap(8)
          .child(div().flex_1().child(this.input(this.keyInput)))
          .child(this.button("record-key", this.recording ? "Press a key…" : "Record", (_e, cx) => { this.recording = !this.recording; this.handle.focus(); cx.notify(); }, { primary: this.recording }))))
      .child(v_flex().gap(5).child(this.label("Action")).child(this.input(this.actionInput))
        .when(Boolean(about), el => el.child(div().text_size(11).text_color(t.dim).child(about))))
      .child(v_flex().gap(5).child(this.label("Section")).child(this.input(this.sectionInput))
        .child(div().text_size(11).text_color(t.dim).child("global works everywhere; terminal only while a terminal has focus; mod:<name> inside that mod.")))
      .children((check?.errors ?? []).map((e, i) => div().id(`key-error-${i}`).text_size(11.5).text_color(t.blocked).child(e)))
      .children((check?.warnings ?? []).map((w, i) => div().id(`key-warning-${i}`).text_size(11.5).text_color(t.working).child(w)))
      .when(Boolean(check?.held), el => el.child(div().text_size(11.5).text_color(t.working).child(`${value(this.keyInput)} runs ${check.held} now; saving replaces it.`)))
      .child(h_flex().gap(8).mt(4)
        .child(this.button("save-key", check?.held ? "Replace" : "Save", (_e, cx) => this.saveKey(cx), { primary: true, enabled: this.canSave() }))
        .when(Boolean(this.form.old), el => el.child(this.button("remove-key", "Unbind", (_e, cx) => this.removeKey(cx), { danger: true }))));
  }

  loadMarket(cx) {
    const args = ["market", "--topic", this.topic, "--sort", this.sort, "--page", String(this.page), "--query", value(this.marketSearch)];
    this.task(cx, "Reading GitHub", args, data => { this.market = data; });
  }

  // from is the Browse row it came from, for the repository's stars.
  review(spec, cx, from = null) {
    this.task(cx, `Reading ${spec}`, ["review", spec], data => {
      if (!data.plans.length) throw new Error(`${spec} has no mods, layouts or themes`);
      this.reviewed = { plans: data.plans, at: 0, takeKeys: false, replaceEdits: false, from };
      this.focusPage();
    });
  }

  install(cx) {
    const r = this.reviewed;
    const p = r.plans[r.at];
    const args = ["install", "--repo", p.repo, "--subdir", p.subdir, "--commit", p.commit];
    if (r.takeKeys) args.push("--take-keys");
    this.task(cx, `Installing ${p.label}`, args, async data => {
      const failed = data.checks.filter(c => c.error);
      this.notice = failed.length ? "" : `${p.label} is installed.`;
      if (failed.length) this.error = failed.map(c => `${c.name}: ${c.error}`).join("\n");
      this.reviewed = null;
      this.modView = "installed";
      this.focusPage();
      // Read before the busy label clears, so the list already shows the change.
      this.packages = await this.call(["packages"]);
    });
  }

  remove(pkg, cx) {
    const args = ["remove", "--repo", pkg.repo];
    if (pkg.subdir) args.push("--subdir", pkg.subdir);
    this.task(cx, `Removing ${pkg.label}`, args, async data => {
      this.notice = data.kept.length ? `${pkg.label} is removed; your edited files stay: ${data.kept.join(", ")}.` : `${pkg.label} is removed.`;
      this.packages = await this.call(["packages"]);
    });
  }

  openThing(kind, name) {
    this.closeAndRun(kind === "layout" ? "layout.switch" : "mod.open", { name });
  }

  modsPage() {
    const t = theme();
    const body = this.reviewed ? this.reviewPanel() : this.modView === "browse" ? this.browse() : this.installed();
    return v_flex().flex_1().min_h(0).gap(10)
      .when(!this.reviewed, el => el.child(h_flex().gap(5)
        .children(this.segment("mods", [["installed", "Installed"], ["browse", "Browse GitHub"]], this.modView, (view, cx) => {
          this.modView = view;
          this.focusPage();
          if (view === "browse" && !this.market) this.loadMarket(cx);
          cx.notify();
        }))
        .child(div().flex_1())
        .child(div().text_size(11).text_color(t.dim).child("Mods and layouts from the herdr-mod and herdr-layout topics."))))
      .child(body);
  }

  installed() {
    const t = theme();
    const p = this.packages;
    if (!p) return this.empty("Loading…");
    const counts = pkg => [plural(pkg.mods.length, "mod"), plural(pkg.layouts.length, "layout"), plural(pkg.themes.length, "theme"), plural(pkg.keys.length, "key")]
      .filter(s => !s.startsWith("0 ")).join(" · ");
    // The app's own mods are parts of its window, not screens to open.
    const open = (kind, name, i, opens = true) => h_flex().id(`open-${kind}-${i}`).px(9).py(5).gap(6).items_center().rounded(6).bg(t.surface)
      .when(opens, el => el.cursor_pointer().hover(s => s.bg(t.hover)).on_click(() => this.openThing(kind, name)))
      .child(div().text_size(12).text_color(opens ? t.foreground : t.muted).child(name)).child(div().text_size(10.5).text_color(t.dim).child(kind));
    const shipped = p.shipped.map(s => s.startsWith("layout:") ? ["layout", s.slice(7)] : ["mod", s]);
    return v_flex().id("installed").flex_1().min_h(0).overflow_y_scrollbar().gap(8)
      .child(this.heading("Installed here"))
      // This screen updates with the program, not as a package, so it has no buttons of its own.
      .child(h_flex().id("pkg-self").p(12).gap(10).items_center().rounded(9).bg(t.surface)
        .child(v_flex().flex_1().min_w(0).gap(4)
          .child(div().text_size(13).font_semibold().child("Customize"))
          .child(div().text_size(11.5).text_color(t.muted).child(`This screen · herdr-customize ${p.version ?? ""} · updates with the program`)))
        .child(this.button("self-releases", "What's new", (_e, cx) => cx.open_url(`${REPO}/releases`), { always: true })))
      .when(!p.installed.length, el => el.child(this.empty("No packages from GitHub yet. Browse GitHub to find mods and layouts.")))
      .children(p.installed.map((pkg, i) => h_flex().id(`pkg-${i}`).p(12).gap(10).items_center().rounded(9).bg(t.surface)
        .child(v_flex().flex_1().min_w(0).gap(4)
          .child(div().text_size(13).font_semibold().child(pkg.label))
          .child(div().text_size(11.5).text_color(t.muted).child(`${counts(pkg)} · ${pkg.commit.slice(0, 7)}`)))
        .child(this.button(`update-${i}`, "Check for update", (_e, cx) => this.review(pkg.subdir ? `${pkg.repo}/${pkg.subdir}` : pkg.repo, cx)))
        .child(this.button(`remove-${i}`, "Remove", (_e, cx) => this.remove(pkg, cx), { danger: true }))))
      .when(p.added.length > 0, el => el.child(this.heading("Added by hand"))
        .child(h_flex().flex_wrap().gap(6).children(p.added.map((a, i) => open(a.kind, a.name, `a${i}`)))))
      .child(this.heading("Shipped with the app"))
      .child(h_flex().flex_wrap().gap(6).children(shipped.map(([kind, name], i) => open(kind, name, `s${i}`, kind === "layout"))));
  }

  browse() {
    const t = theme();
    const m = this.market;
    const pages = m ? Math.max(1, Math.ceil(Math.min(m.total, 1000) / m.per_page)) : 1;
    const go = (page, cx) => { this.page = page; this.loadMarket(cx); };
    return v_flex().flex_1().min_h(0).gap(8)
      .child(h_flex().gap(6).items_center()
        .child(div().flex_1().child(this.input(this.marketSearch)))
        .children(this.segment("topic", [["mods", "Mods"], ["layouts", "Layouts"]], this.topic, (topic, cx) => { this.topic = topic; go(1, cx); }))
        .child(div().w(8))
        .children(this.segment("sort", [["stars", "Stars"], ["updated", "Updated"]], this.sort, (sort, cx) => { this.sort = sort; go(1, cx); })))
      .child(v_flex().id("market").flex_1().min_h(0).overflow_y_scrollbar().gap(6)
        .when(!m, el => el.child(this.empty(this.busy ? "Reading GitHub…" : "Press Enter to search.")))
        .when(Boolean(m) && !m.repos.length, el => el.child(this.empty("Nothing found.")))
        .children((m?.repos ?? []).map((r, i) => {
          return h_flex().id(`repo-${i}`).p(10).gap(12).items_center().rounded(9).bg(t.surface).cursor_pointer().hover(s => s.bg(t.hover))
            .on_click((_e, cx) => this.review(r.spec, cx, r))
            .child(div().w(128).h(80).flex_none().rounded(6).overflow_hidden().bg(t.raised)
              .when(Boolean(r.preview), el => el.child(image(r.preview).size_full())))
            .child(v_flex().flex_1().min_w(0).gap(3)
              .child(h_flex().gap(8).items_center()
                .child(div().text_size(13.5).font_semibold().child(r.name))
                .when(r.stars > 0, el => el.child(div().text_size(11).text_color(t.dim).child(`★ ${r.stars}`))))
              .child(div().text_size(11).text_color(t.dim).child(`${r.owner} · ${r.spec}`))
              .when(Boolean(r.blurb), el => el.child(div().text_size(11.5).text_color(t.muted).max_h(34).overflow_hidden().child(r.blurb))))
            .child(this.button(`review-${i}`, "Review", (_e, cx) => this.review(r.spec, cx, r)));
        })))
      .when(Boolean(m) && pages > 1, el => el.child(h_flex().gap(8).items_center().justify_center()
        .child(this.button("page-prev", "Previous", (_e, cx) => go(this.page - 1, cx), { enabled: this.page > 1 }))
        .child(div().text_size(11.5).text_color(t.muted).child(`Page ${this.page} of ${pages}`))
        .child(this.button("page-next", "Next", (_e, cx) => go(this.page + 1, cx), { enabled: this.page < pages }))));
  }

  reviewPanel() {
    const t = theme();
    const r = this.reviewed;
    const p = r.plans[r.at];
    const list = (title, items, color) => items.length ? v_flex().gap(3).child(this.heading(title))
      .children(items.map((item, i) => div().id(`${title}-${i}`).text_size(12).text_color(color ?? t.foreground).child(item))) : div();
    const check = (id, field, text) => h_flex().id(id).gap(8).items_center().cursor_pointer().on_click((_e, cx) => { r[field] = !r[field]; cx.notify(); })
      .child(div().size(14).rounded(3).border_1().border_color(t.border).bg(r[field] ? t.accent : t.input))
      .child(div().text_size(12).child(text));
    const blocked = p.taken.length > 0 || (p.edited.length > 0 && !r.replaceEdits);
    const folder = `https://github.com/${p.repo}${p.subdir ? `/tree/${p.branch}/${p.subdir}` : ""}`;
    const line = `Read ~/.config/herdr-app/AGENTS.md, then add this ${p.mods.length || !p.layouts.length ? "mod" : "layout"}: ${folder}`;
    // The title is in the header, and pictures beside the recipe can't load here.
    const recipe = p.recipe.replace(/^# .*\n+/, "").replace(/!\[[^\]]*\]\((?![a-z]+:)[^)]*\)\s*/gi, "");
    const blob = `https://github.com/${p.repo}/blob/${p.commit}/${p.subdir ? `${p.subdir}/` : ""}`;
    const stars = r.from?.stars ?? 0;
    return v_flex().flex_1().min_h(0).gap(12)
      .child(h_flex().gap(10).items_center()
        .child(this.button("review-back", "Back", (_e, cx) => this.closeReview(cx)))
        .child(div().text_size(20).font_semibold().child(p.title))
        .child(this.badge(p.update ? "update" : "new", p.update ? t.working : t.done))
        .child(div().text_size(11.5).text_color(t.dim).child(`${p.label} · ${p.commit.slice(0, 7)}`))
        .when(stars > 0, el => el.child(div().text_size(11.5).text_color(t.dim).child(`★ ${stars}`)))
        .child(div().flex_1())
        .child(this.button("open-github", "View on GitHub", (_e, cx) => cx.open_url(folder))))
      .when(r.plans.length > 1, el => el.child(h_flex().flex_wrap().gap(5).children(r.plans.map((q, i) => this.chip(`plan-${i}`, q.title, i === r.at, (_e, cx) => { r.at = i; cx.notify(); })))))
      .child(h_flex().flex_1().min_h(0).gap(18).items_start()
        .child(v_flex().id("review-doc").flex_1().min_w(0).h_full().overflow_y_scrollbar().gap(14).pr(8)
          .when(Boolean(p.summary), el => el.child(div().text_size(13).text_color(t.muted).child(p.summary)))
          .when(Boolean(p.preview), el => el.child(div().w_full().h(340).flex_none().rounded(10).overflow_hidden().bg(t.surface)
            .child(image(p.preview).size_full())))
          .when(Boolean(recipe.trim()), el => el.child(this.heading("Recipe"))
            .child(div().text_size(13).text_color(t.foreground)
              .child(TextView.markdown(`recipe-${p.label}`, recipe).selectable()
                .on_link_click((url, cx) => cx.open_url(/^[a-z][a-z0-9+.-]*:/i.test(url) ? url : blob + url))))))
        .child(v_flex().id("review-side").w(340).flex_none().h_full().overflow_y_scrollbar().gap(12).pr(10)
          .child(list("Mods", p.mods)).child(list("Layouts", p.layouts)).child(list("Themes", p.themes))
          .child(list("Keys", p.keys.map(k => `${k.key} → ${k.action}`)))
          .child(list("Its code", p.uses, t.working))
          .when(p.key_clashes.length > 0, el => el
            .child(list("Keys you already use", p.key_clashes.map(c => `${c.key} runs ${c.held} now; it wants ${c.action}`), t.working))
            .child(check("take-keys", "takeKeys", "Give these keys to the package")))
          .child(list("Already taken", p.taken.map(n => `${n} exists and is not from this package; remove it first`), t.blocked))
          .when(p.edited.length > 0, el => el
            .child(list("Files you changed", p.edited, t.working))
            .child(check("replace-edits", "replaceEdits", "Replace my changes")))
          .child(this.button("install", p.update ? "Update" : "Install", (_e, cx) => this.install(cx), { primary: true, enabled: !blocked }))
          .child(div().text_size(11).text_color(t.dim).child("Read what its code reaches before you install it; mods run with your permissions."))
          .child(div().h(1).bg(t.border))
          .child(this.heading("Or tell your agent"))
          .child(div().text_size(11).text_color(t.dim).child("For a package whose recipe sets something up first, an agent in the app follows it."))
          .child(div().p(10).rounded(7).bg(t.surface).font_family(String(settings().font_family ?? "monospace")).text_size(11.5).child(line))
          .child(this.button("copy-line", "Copy", (_e, cx) => { cx.write_to_clipboard(line); this.notice = "Copied: paste it to an agent in the app."; cx.notify(); }))));
  }

  // Copied in by hand, the screen can't find the program it runs.
  missing() {
    const t = theme();
    const windows = platform() === "windows";
    const step = (n, text) => h_flex().gap(10).items_start()
      .child(h_flex().size(22).rounded(11).bg(t.raised).flex_none().items_center().justify_center().text_size(11.5).font_semibold().child(String(n)))
      .child(div().flex_1().min_w(0).pt(2).text_size(12.5).child(text));
    return v_flex().max_w(640).gap(14).pt(12)
      .child(div().text_size(18).font_semibold().child("This screen needs the herdr-customize program"))
      .child(div().text_size(12.5).text_color(t.muted).child(this.stale
        ? `It found ${this.stale}, which is older than this screen.`
        : "It looked in ~/.local/share/herdr-customize and on your PATH and found nothing."))
      .child(step(1, `git clone ${REPO} ~/.local/share/herdr-customize`))
      .child(step(2, windows ? "In that folder: powershell -File scripts/fetch-or-build.ps1" : "In that folder: sh scripts/fetch-or-build.sh"))
      .child(step(3, "Then: target/release/herdr-customize app install"))
      .child(div().text_size(11.5).text_color(t.dim).child("The script downloads the binary for this version, or builds it with cargo. Install rewrites this mod with the program's place and adds the themes; your keys stay as they are."))
      .child(h_flex().gap(8)
        .child(this.button("find-again", "Look again", (_e, cx) => { this.binFound = this.findBin(); this.bin = undefined; this.loadAll(cx); cx.notify(); }, { primary: true }))
        .child(this.button("open-repo", "Open on GitHub", (_e, cx) => cx.open_url(REPO))));
  }

  render() {
    const t = theme();
    const page = this.bin === null ? this.missing()
      : { themes: () => this.themesPage(), keys: () => this.keysPage(), mods: () => this.modsPage() }[this.tab]();
    return v_flex().size_full().min_h(0).bg(t.popover).text_color(t.foreground)
      .key_context("Customize")
      .on_action("customize.next_screen", (_e, cx) => this.stepTab(1, cx))
      .on_action("customize.prev_screen", (_e, cx) => this.stepTab(-1, cx))
      .track_focus(this.handle).on_key_down((e, cx) => this.onKey(e, cx))
      .child(h_flex().px(20).h(52).gap(14).items_center().border_b(1).border_color(t.border).flex_none()
        .child(div().text_size(14).font_semibold().child("Customize"))
        .child(h_flex().gap(2).p(3).rounded(8).bg(t.surface).children(TABS.map(([id, text]) => div().id(`tab-${id}`).px(12).py(5).rounded(6)
          .text_size(12.5).font_medium().cursor_pointer().bg(this.tab === id ? t.raised : t.surface).text_color(this.tab === id ? t.foreground : t.muted)
          .hover(s => s.text_color(t.foreground)).on_click((_e, cx) => this.showTab(id, cx)).child(text))))
        .child(div().flex_1())
        .when(Boolean(this.busy), el => el.child(div().text_size(11.5).text_color(t.muted).child(`${this.busy}…`)))
        .child(this.button("close", "Close", () => this.close(), { always: true })))
      .child(v_flex().px(20).pt(14).pb(12).flex_1().min_h(0).child(page))
      .when(Boolean(this.error || this.notice), el => el.child(div().mx(20).mb(14).p(10).rounded(8).flex_none()
        .bg(this.error ? `${t.blocked}18` : `${t.done}18`).text_size(11.5).text_color(this.error ? t.blocked : t.done).child(this.error || this.notice)));
  }
}
