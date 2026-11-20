import { div, uniform_list } from "gpui-kit";
import { h_flex, v_flex, InputState } from "gpui-base";
import { theme, keys, commands } from "herdr";
import { page, value, reason, commandOf, keyRowId, MODIFIER_KEYS, platform } from "../lib.js";

export default function create(app) {
  class Page {
    id = "keys";
    title = "Keys";
    init(cx) {
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
    this.shipped = null;
    this.presets = null;
    this.presetsDismissed = false;
    this.argumentInputs = [];
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

  keyRows() {
    const query = value(this.keySearch).toLowerCase();
    const match = (...words) => !query || words.some(w => w.toLowerCase().includes(query));
    const all = keys();
    const sections = new Map();
    for (const k of all) sections.set(k.keys, (sections.get(k.keys) ?? 0) + 1);
    const bound = all.filter(k => match(k.keys, k.action, k.section));
    const used = new Set(all.map(k => commandOf(k.action)));
    const free = commands().filter(c => !used.has(c.name) && match(c.name, c.about));
    const rows = [];
    let section = null;
    for (const k of [...bound].sort((x, y) => (x.section === "global" ? "" : x.section).localeCompare(y.section === "global" ? "" : y.section))) {
      if (k.section !== section) rows.push({ id: `h:${k.section}`, header: k.section });
      section = k.section;
      rows.push({ id: keyRowId(k.section, k.keys), section: k.section, key: k.keys, action: k.action, shared: sections.get(k.keys) > 1 });
    }
    if (free.length) rows.push({ id: "h:free", header: "commands without a key" });
    for (const c of free) rows.push({ id: `c:${c.name}`, section: "global", key: "", action: c.name, about: c.about, args: c.args });
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
    this.argumentInputs = (row.args ?? []).map(arg => {
      const input = InputState.new({ placeholder: arg.name });
      input.on("change", (_e, cx) => this.checkKey(cx));
      input.on("submit", (_e, cx) => this.saveKey(cx));
      return { ...arg, input };
    });
    this.form.command = row.args?.length ? row.action : null;
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
  action() {
    if (!this.form?.command) return value(this.actionInput);
    const args = {};
    for (const arg of this.argumentInputs) {
      const text = value(arg.input);
      if (!text) continue;
      args[arg.name] = text === "true" ? true : text === "false" ? false : /^-?(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?$/i.test(text) && Number.isFinite(Number(text)) ? Number(text) : text;
    }
    return JSON.stringify({ command: this.form.command, args });
  }


  keyArgs() {
    const args = ["--section", this.section(), "--key", value(this.keyInput), "--action", this.action()];
    if (this.form?.old) args.push("--old", this.form.old);
    return args;
  }

  // Only the check of the form as it is now counts: Enter can beat it, and an
  // older check may be of another key, with --replace for that key's holder.
  canSave() {
    return Boolean(this.form && value(this.keyInput) && this.action() && this.check?.seq === this.checkSeq && !this.check.errors?.length);
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
    const action = this.action();
    this.task(cx, "Saving the key", args, () => {
      this.form = { old: key, section: this.section() };
      this.actionInput.set_value(action);
      this.notice = `${key} runs ${action}.`;
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

  // The app stops adding its new keys once keys.json is edited, so this offers
  // them. Offline, or with an app the program can't ask, it offers nothing.
  loadShipped(cx) {
    this.shipped = { keys: [] };
    cx.spawn(async cx => {
      try { this.shipped = await this.call(["key-shipped"]); } catch { return; }
      cx.notify();
    });
  }

  changeShipped(cx, change) {
    const n = this.shipped.keys.length;
    this.task(cx, change === "add" ? "Adding the keys" : "Dismissing", [`key-shipped-${change}`], data => {
      this.shipped = data;
      this.notice = change === "add" ? `${n === 1 ? "The key is" : `${n} keys are`} in keys.json.` : "Those keys won't be offered again.";
    });
  }

  shippedCard() {
    const t = theme();
    const s = this.shipped;
    const n = s.keys.length;
    return v_flex().id("shipped-keys").p(12).gap(8).rounded(9).bg(t.surface)
      .child(div().text_size(13).font_semibold().child(`Herdr app ${s.version} has ${n === 1 ? "a key" : `${n} keys`} you don't have`))
      .child(div().text_size(11.5).text_color(t.muted).child("The app adds its new keys only to a keys.json nobody has changed."))
      .children(s.keys.map((k, i) => h_flex().id(`shipped-${i}`).gap(10).text_size(12)
        .child(div().w(150).flex_none().child(k.key))
        .child(div().flex_1().min_w(0).text_color(t.muted).whitespace_nowrap().text_ellipsis().child(k.section === "global" ? k.action : `${k.action} · ${k.section}`))))
      .child(h_flex().gap(8)
        .child(this.button("shipped-add", n === 1 ? "Add it" : "Add them", (_e, cx) => this.changeShipped(cx, "add"), { primary: true }))
        .child(this.button("shipped-dismiss", "Dismiss", (_e, cx) => this.changeShipped(cx, "dismiss"))));
  }
  loadPresets(cx) {
    this.presets = { presets: [] };
    cx.spawn(async cx => {
      try { this.presets = await this.call(["key-presets"]); } catch { return; }
      cx.notify();
    });
  }

  applyPreset(cx, id) {
    this.task(cx, "Applying preset", ["key-presets-apply", "--id", id], (data, cx) => {
      this.notice = data.bindings.map(b => `${b.key}: ${b.status}${b.reason ? ` (${b.reason})` : ""}`).join("; ");
      this.loadPresets(cx);
    });
  }

  presetsCard() {
    const t = theme();
    return v_flex().id("key-presets").p(12).gap(8).rounded(9).bg(t.surface)
      .child(h_flex().gap(8).child(div().flex_1().font_semibold().child("Presets"))
        .child(this.button("dismiss-presets", "Dismiss", (_e, cx) => { this.presetsDismissed = true; cx.notify(); })))
      .children(this.presets.presets.map(p => v_flex().id(`preset-${p.id}`).gap(4)
        .child(h_flex().gap(8).child(div().flex_1().font_semibold().child(p.title))
          .child(this.button(`apply-${p.id}`, "Apply", (_e, cx) => this.applyPreset(cx, p.id))))
        .child(div().text_size(11).text_color(t.dim).child(p.about))
        .children(p.bindings.map((b, i) => div().id(`preset-${p.id}-${i}`).text_size(11).text_color(t.muted)
          .child(`${b.key} (${b.section}): ${b.status}${b.reason ? ` — ${b.reason}` : ""}`)))));
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
    ], this.form ? this.keyForm() : v_flex().id("keys-info").flex_1().min_h(0).overflow_y_scrollbar().gap(10)
      .when(this.shipped?.keys.length > 0, el => el.child(this.shippedCard()))
      .when(!this.presetsDismissed && this.presets?.presets.length > 0, el => el.child(this.presetsCard()))
      .child(this.empty("Pick a binding to change it, or a command without a key to give it one."))
      .child(div().text_size(11).text_color(t.dim).child("Keys are saved to keys.json, and the app picks them up at once.")));
  }

  keyForm() {
    const t = theme();
    const check = this.check;
    const name = this.form.command || commandOf(value(this.actionInput));
    const about = commands().find(c => c.name === name)?.about;
    return v_flex().id("key-form").flex_1().min_w(0).min_h(0).overflow_y_scrollbar().gap(10)
      .child(div().text_size(18).font_semibold().child(this.form.old ? `Change ${this.form.old}` : "New binding"))
      .child(v_flex().gap(5).child(this.label("Key"))
        .child(h_flex().gap(8)
          .child(div().flex_1().child(this.input(this.keyInput)))
          .child(this.button("record-key", this.recording ? "Press a key…" : "Record", (_e, cx) => { this.recording = !this.recording; this.handle.focus(); cx.notify(); }, { primary: this.recording }))))
      .when(!this.form.command, el => el.child(v_flex().gap(5).child(this.label("Action")).child(this.input(this.actionInput))
        .when(Boolean(about), el => el.child(div().text_size(11).text_color(t.dim).child(about)))))
      .when(Boolean(this.form.command), el => el.child(v_flex().gap(5).child(this.label(`Action: ${this.form.command}`))
        .when(Boolean(about), el => el.child(div().text_size(11).text_color(t.dim).child(about)))
        .children(this.argumentInputs.map(arg => v_flex().gap(5).child(this.label(`${arg.name}${arg.required ? " (required)" : " (optional)"}`)).child(this.input(arg.input))))))
      .child(v_flex().gap(5).child(this.label("Section")).child(this.input(this.sectionInput))
        .child(div().text_size(11).text_color(t.dim).child("global works everywhere; terminal only while a terminal has focus; mod:<name> inside that mod.")))
      .children((check?.errors ?? []).map((e, i) => div().id(`key-error-${i}`).text_size(11.5).text_color(t.blocked).child(e)))
      .children((check?.warnings ?? []).map((w, i) => div().id(`key-warning-${i}`).text_size(11.5).text_color(t.working).child(w)))
      .when(Boolean(check?.held), el => el.child(div().text_size(11.5).text_color(t.working).child(`${value(this.keyInput)} runs ${check.held} now; saving replaces it.`)))
      .child(h_flex().gap(8).mt(4)
        .child(this.button("save-key", check?.held ? "Replace" : "Save", (_e, cx) => this.saveKey(cx), { primary: true, enabled: this.canSave() }))
        .when(Boolean(this.form.old), el => el.child(this.button("remove-key", "Unbind", (_e, cx) => this.removeKey(cx), { danger: true }))));
  }

  focus() { this.keySearch.focus(); }
  show(cx) { if (!this.shipped) this.loadShipped(cx); if (!this.presets) this.loadPresets(cx); }
  record(e, cx) {
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
  render() { return this.keysPage(); }
  }
  return page(app, new Page());
}
