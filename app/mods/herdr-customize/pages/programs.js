import { div } from "gpui-kit";
import { h_flex, v_flex, InputState } from "gpui-base";
import * as herdr from "herdr";
import { page, value } from "../lib.js";

const fields = ["key", "name", "run", "match", "tile", "color", "icon"];
const labels = { key: "Key", name: "Name", run: "Command line", match: "Match names (comma separated)", tile: "Tile color", color: "Icon color", icon: "Icon path in app home" };
const quote = word => /[\s'"]/.test(word) ? `"${word.replaceAll("\\", "\\\\").replaceAll('"', '\\"')}"` : word;

export default function create(app) {
  class Page {
    id = "programs";
    title = "Programs";
    init() {
      this.data = null;
      this.agents = null;
      this.agentStatus = "Checking…";
      this.selected = null;
      this.original = {};
      this.tint = false;
      this.inputs = {};
      this.resetInputs({});
    }
    resetInputs(row) {
      for (const name of fields) {
        this.inputs[name]?.release();
        const text = name === "run" ? (row.run ?? []).map(quote).join(" ") : name === "match" ? (row.match ?? []).join(", ") : row[name] ?? "";
        const input = InputState.new({ placeholder: "", value: text });
        input.on("change", (_e, cx) => cx.notify());
        this.inputs[name] = input;
      }
      this.tint = row.tint === true;
    }
    pick(row, cx) {
      this.selected = row?.key ?? null;
      this.original = { ...row };
      delete this.original.key;
      this.resetInputs(row ?? {});
      this.inputs.key.focus();
      cx.notify();
    }
    checkAgents(cx) {
      if (typeof herdr.knownAgents !== "function") { this.agentStatus = "Not supported by this app."; return; }
      this.agentStatus = "Checking…";
      cx.spawn(async cx => {
        try { this.agents = await herdr.knownAgents(); this.agentStatus = ""; }
        catch { this.agents = null; this.agentStatus = "Could not check."; }
        cx.notify();
      });
    }
    save(cx) {
      const entry = { ...this.original };
      for (const name of ["name", "tile", "color", "icon"]) {
        const text = value(this.inputs[name]);
        if (text) entry[name] = text; else delete entry[name];
      }
      const matches = value(this.inputs.match);
      if (matches) entry.match = matches.split(",").map(s => s.trim()); else delete entry.match;
      if (this.tint) entry.tint = true; else if ("tint" in entry) entry.tint = false;
      this.task(cx, "Saving program", ["program-set", "--key", value(this.inputs.key), "--json", JSON.stringify(entry), "--run-line", value(this.inputs.run)], (data, cx) => {
        this.data = data;
        this.notice = "Program saved.";
        this.pick(data.programs.find(p => p.key === value(this.inputs.key)), cx);
        this.checkAgents(cx);
      });
    }
    remove(cx) {
      this.task(cx, "Removing program", ["program-remove", "--key", this.selected], (data, cx) => {
        this.data = data;
        this.notice = "Program removed.";
        this.pick(null, cx);
        this.checkAgents(cx);
      });
    }
    focus() { this.inputs.key.focus(); }
    show(cx) {
      if (!this.data) this.load(cx, ["programs"], (data, cx) => { this.data = data; cx.notify(); });
      if (!this.agents && this.agentStatus === "Checking…") this.checkAgents(cx);
    }
    render() {
      const t = herdr.theme();
      if (!this.data) return this.empty("Reading programs…");
      const rows = this.data.programs.map(row => h_flex().id(`program-${row.key}`).px(10).py(8).gap(8).rounded(6).cursor_pointer()
        .bg(row.key === this.selected ? t.selected : t.popover).hover(st => st.bg(t.hover)).on_click((_e, cx) => this.pick(row, cx))
        .child(v_flex().min_w(0).gap(2)
          .child(div().text_size(13).child(row.name || row.key))
          .child(div().text_size(11).text_color(t.dim).child(`${row.key}  ${Array.isArray(row.run) ? row.run.join(" ") : ""}`))));
      const detail = v_flex().flex_1().min_w(0).gap(7)
        .child(div().text_size(18).font_semibold().child(this.selected ? "Edit program" : "Add program"))
        .child(div().text_size(11).text_color(t.dim).child("Overrides add to the built-in table. Use programs or shell functions, never aliases."));
      for (const name of fields) {
        const input = h_flex().gap(6).items_center().child(div().flex_1().child(this.input(this.inputs[name])));
        if (name === "tile" && /^#[0-9a-f]{6}([0-9a-f]{2})?$/i.test(value(this.inputs.tile))) input.child(div().w(20).h(20).rounded(4).bg(value(this.inputs.tile)));
        detail.child(v_flex().gap(3).child(this.label(labels[name])).child(input));
      }
      detail.child(this.chip("program-tint", "Tint icon", this.tint, (_e, cx) => { this.tint = !this.tint; cx.notify(); }))
        .when(Boolean(this.data.error), el => el.child(div().text_size(11).text_color(t.blocked).child(this.data.error)))
        .child(h_flex().gap(8).child(this.button("program-save", "Save", (_e, cx) => this.save(cx), { primary: true }))
          .when(Boolean(this.selected), el => el.child(this.button("program-remove", "Remove", (_e, cx) => this.remove(cx), { danger: true }))));
      const agents = (this.agents ?? []).map(a => div().text_size(11.5).text_color(a.installed ? t.foreground : t.dim).child(`${a.name} — ${a.installed ? "Installed" : "Not installed"}`));
      return this.split(380, [this.button("program-new", "Add program", (_e, cx) => this.pick(null, cx)), ...rows,
        this.heading("Agents the + menu can start"),
        div().text_size(11).text_color(t.dim).child(this.agentStatus), ...agents], detail);
    }
  }
  return page(app, new Page());
}
