import { div } from "gpui-kit";
import { h_flex, v_flex, InputState } from "gpui-base";
import * as herdr from "herdr";
import { page, value } from "../lib.js";

const fields = ["key", "name", "run", "match", "tile", "color", "icon"];
const labels = { key: "Key", name: "Name", run: "Command the + menu runs", match: "Other process names (comma separated)", tile: "Tile color", color: "Icon color", icon: "Icon file in the app folder" };
const helps = {
  key: "A short id such as mytool. A built-in program's key changes that program instead.",
  name: "Shown in the + menu, tabs and notifications.",
  run: "A program or shell function, never an alias. Quote arguments that hold spaces.",
  match: "Processes that also count as this program, like mytool-dev.",
  tile: "#rrggbb, the colored square behind the icon.",
  color: "#rrggbb, the icon's own color when it is tinted.",
  icon: "A file inside the app folder, like icons/mytool.svg.",
};
const holders = { key: "mytool", name: "My Tool", run: "mytool --flag", match: "", tile: "#334155", color: "#ffffff", icon: "icons/mytool.svg" };
const quote = word => /[\s'"]/.test(word) ? `"${word.replaceAll("\\", "\\\\").replaceAll('"', '\\"')}"` : word;
const runText = run => (run ?? []).map(quote).join(" ");

export default function create(app) {
  class Page {
    id = "programs";
    title = "Programs";
    init() {
      this.data = null;
      this.agents = null;
      this.machine = null;
      this.agentStatus = "Checking…";
      this.checking = false;
      this.editing = false;
      this.selected = null;
      this.original = {};
      this.builtin = null;
      this.tint = false;
      this.inputs = {};
      this.resetInputs({});
    }
    resetInputs(row) {
      for (const name of fields) {
        this.inputs[name]?.release();
        const text = name === "run" ? runText(row.run) : name === "match" ? (row.match ?? []).join(", ") : row[name] ?? "";
        const input = InputState.new({ placeholder: holders[name], value: text });
        input.on("change", (_e, cx) => cx.notify());
        this.inputs[name] = input;
      }
      this.tint = row.tint === true;
    }
    personal(key) {
      return this.data?.programs.find(p => p.key === key);
    }
    // key null starts a new program; a built-in agent shows its values, then its overrides on top.
    pick(key, cx) {
      const own = key ? this.personal(key) : null;
      const builtin = key ? this.agents?.find(a => a.key === key) ?? null : null;
      this.editing = true;
      this.selected = key;
      this.builtin = builtin;
      this.original = { ...own };
      delete this.original.key;
      this.resetInputs({ ...builtin, ...own, key: key ?? "" });
      (key ? this.inputs.name : this.inputs.key).focus();
      cx.notify();
    }
    stopEditing(cx) {
      this.editing = false;
      this.selected = null;
      this.focusPage();
      cx.notify();
    }
    checkAgents(cx) {
      if (typeof herdr.knownAgents !== "function") { this.agentStatus = "This version of the app cannot list them."; return; }
      this.agentStatus = "Checking…";
      this.checking = true;
      const on = this.machine;
      cx.spawn(async cx => {
        try {
          const agents = await herdr.knownAgents({ on });
          // A check for a machine no longer picked would show that machine's list.
          if (on !== this.machine) return;
          this.agents = agents;
          this.agentStatus = "";
          if (this.builtin) this.builtin = this.agents.find(a => a.key === this.selected) ?? this.builtin;
        } catch {
          if (on !== this.machine) return;
          this.agents = null;
          this.agentStatus = `Could not check ${on ?? "this computer"}.`;
        }
        this.checking = false;
        cx.notify();
      });
    }
    pickMachine(name, cx) {
      if (name === this.machine) return;
      this.machine = name;
      this.agents = null;
      this.checkAgents(cx);
      cx.notify();
    }
    // The app reloads programs.json a moment after it is written; asking at once returns the old table.
    recheckAgents(cx) {
      cx.timer.after(1500, c => this.checkAgents(c));
    }
    save(cx) {
      const key = value(this.inputs.key);
      const entry = { ...this.original };
      const b = this.builtin;
      // A built-in value is written only when changed, so app updates still reach what you left alone.
      const differs = (name, text) => !b || text !== String(b[name] ?? "") || name in this.original;
      for (const name of ["name", "tile", "color", "icon"]) {
        const text = value(this.inputs[name]);
        if (text && differs(name, text)) entry[name] = text; else delete entry[name];
      }
      const matches = value(this.inputs.match);
      if (matches) entry.match = matches.split(",").map(s => s.trim()); else delete entry.match;
      if (this.tint !== (b?.tint === true) || "tint" in this.original) entry.tint = this.tint; else delete entry.tint;
      const run = value(this.inputs.run);
      const runLine = b && run === runText(b.run) && !("run" in this.original) ? "" : run;
      if (b && !Object.keys(entry).length && !runLine) { this.notice = "Nothing changed from the built-in."; cx.notify(); return; }
      this.task(cx, "Saving program", ["program-set", "--key", key, "--json", JSON.stringify(entry), "--run-line", runLine], (data, cx) => {
        this.data = data;
        this.notice = "Program saved.";
        this.pick(key, cx);
        this.recheckAgents(cx);
      });
    }
    remove(cx) {
      this.task(cx, this.builtin ? "Resetting program" : "Removing program", ["program-remove", "--key", this.selected], (data, cx) => {
        this.data = data;
        this.notice = this.builtin ? "Back to the built-in program." : "Program removed.";
        this.stopEditing(cx);
        this.recheckAgents(cx);
      });
    }
    focus() { this.handle.focus(); }
    show(cx) {
      if (!this.data) this.load(cx, ["programs"], (data, cx) => { this.data = data; cx.notify(); });
      // Coming back to the page retries a check that failed, as after a machine reconnects.
      if (!this.agents && !this.checking) this.checkAgents(cx);
    }
    row(id, key, title, note) {
      const t = herdr.theme();
      return h_flex().id(id).px(10).py(8).gap(8).items_center().rounded(6).cursor_pointer()
        .bg(this.editing && key === this.selected ? t.selected : t.popover).hover(st => st.bg(t.hover)).on_click((_e, cx) => this.pick(key, cx))
        .child(div().flex_1().min_w(0).text_size(13).child(title))
        .child(div().text_size(11).text_color(t.dim).child(note));
    }
    explainer() {
      const t = herdr.theme();
      return v_flex().flex_1().min_w(0).gap(10)
        .child(div().text_size(18).font_semibold().child("Programs"))
        .child(div().text_size(12).text_color(t.muted).child("The app keeps one table of programs. It decides what the + menu can start, and the name, icon and color shown for whatever runs in a terminal."))
        .child(div().text_size(12).text_color(t.muted).child("Click an agent on the left to rename it, recolor it or change the command the + menu runs. Only what you change is saved; the rest stays as the app ships it."))
        .child(div().text_size(12).text_color(t.muted).child("Or add a program of your own, such as a tool you start often, and it shows up in the + menu."))
        .child(div().text_size(12).text_color(t.muted).child("Installed shows what the + menu finds on the machine picked on the left. The table itself lives on this computer, so a change here applies wherever an agent starts."))
        .child(h_flex().child(this.button("program-new", "Add a program", (_e, cx) => this.pick(null, cx), { primary: true })));
    }
    form() {
      const t = herdr.theme();
      const own = this.selected && this.personal(this.selected);
      const title = !this.selected ? "Add a program" : this.builtin ? `Customize ${this.builtin.name}` : "Edit program";
      const head = v_flex().min_w(0).gap(8)
        .child(h_flex().gap(10).items_center()
          .child(this.button("program-back", "Back", (_e, cx) => this.stopEditing(cx)))
          .child(div().text_size(18).font_semibold().child(title))
          .child(div().flex_1())
          .when(Boolean(own), el => el.child(this.button("program-remove", this.builtin ? "Back to built-in" : "Remove", (_e, cx) => this.remove(cx), { danger: true })))
          .child(this.button("program-save", "Save", (_e, cx) => this.save(cx), { primary: true })))
        .child(div().text_size(11.5).text_color(t.muted).child(this.builtin ? `Built into the app (${this.selected}). Fields you leave as they are keep the app's values.` : "Your own entry; the app's built-in programs are not touched."))
        .when(Boolean(this.data.error), el => el.child(div().text_size(11).text_color(t.blocked).child(this.data.error)));
      const body = v_flex().id("program-form").flex_1().min_h(0).overflow_y_scrollbar().gap(8);
      for (const name of fields) {
        if (name === "key" && this.selected) continue;
        const input = h_flex().gap(6).items_center().child(div().flex_1().child(this.input(this.inputs[name])));
        if ((name === "tile" || name === "color") && /^#[0-9a-f]{6}([0-9a-f]{2})?$/i.test(value(this.inputs[name]))) input.child(div().w(20).h(20).rounded(4).bg(value(this.inputs[name])));
        body.child(v_flex().gap(3).child(this.label(labels[name])).child(input).child(div().text_size(11).text_color(t.dim).child(helps[name])));
      }
      body.child(h_flex().gap(8).items_center()
        .child(this.chip("program-tint", "Tint the icon", this.tint, (_e, cx) => { this.tint = !this.tint; cx.notify(); }))
        .child(div().text_size(11).text_color(t.dim).child("For a one-color glyph; leave off for a picture.")));
      return v_flex().flex_1().min_w(0).h_full().gap(8).child(head).child(body);
    }
    render() {
      const t = herdr.theme();
      if (!this.data) return this.empty("Reading programs…");
      const agentKeys = new Set((this.agents ?? []).map(a => a.key));
      const mine = this.data.programs.filter(p => !agentKeys.has(p.key));
      const left = [this.button("program-add", "Add a program", (_e, cx) => this.pick(null, cx))];
      // Installed status is per machine; programs.json, and so every customization, is shared by all.
      const remotes = typeof herdr.machines === "function" ? herdr.machines().filter(m => m.name) : [];
      if (remotes.length || this.machine) {
        const listed = remotes.some(m => m.name === this.machine) || !this.machine ? remotes : [...remotes, { name: this.machine, layout: "unavailable" }];
        left.push(h_flex().gap(6).flex_wrap().items_center()
          .child(div().text_size(11.5).text_color(t.muted).child("Installed on"))
          .child(this.chip("programs-here", "This computer", !this.machine, (_e, cx) => this.pickMachine(null, cx)))
          .children(listed.map(m => this.chip(`programs-on-${m.name}`, m.layout === "unavailable" ? `${m.name} (offline)` : m.name, this.machine === m.name, (_e, cx) => this.pickMachine(m.name, cx)))));
      }
      if (mine.length) left.push(this.heading("Your programs"), ...mine.map(p => this.row(`program-${p.key}`, p.key, p.name || p.key, p.run ? "+ menu" : "")));
      if (this.agentStatus) left.push(this.heading("Agents in the + menu · click to customize"), div().text_size(11).text_color(t.dim).child(this.agentStatus));
      if (!this.agents && !this.checking && typeof herdr.knownAgents === "function") left.push(h_flex().child(this.button("programs-retry", "Check again", (_e, cx) => this.checkAgents(cx))));
      // The app lists them most picked first; that order holds within each group.
      for (const [installed, heading] of [[true, "Installed agents · click to customize"], [false, "Not installed"]]) {
        const group = (this.agents ?? []).filter(a => Boolean(a.installed) === installed);
        if (group.length) left.push(this.heading(heading), ...group.map(a => this.row(`agent-${a.key}`, a.key, a.name, this.personal(a.key) ? "customized" : "")));
      }
      return this.split(380, left, this.editing ? this.form() : this.explainer());
    }
  }
  return page(app, new Page());
}
