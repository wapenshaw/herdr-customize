import { div } from "gpui-kit";
import { h_flex, v_flex, InputState } from "gpui-base";
import * as herdr from "herdr";
import { page, value } from "../lib.js";

export default function create(app) {
  class Page {
    id = "popups";
    title = "Popups";
    init() {
      this.popups = null;
      this.problem = "";
      this.persistent = false;
      this.nameInput = InputState.new({ placeholder: "Name" });
      this.commandInput = InputState.new({ placeholder: "Command and arguments" });
      this.widthInput = InputState.new({ value: "0.8", placeholder: "0.8" });
      this.heightInput = InputState.new({ value: "0.8", placeholder: "0.8" });
      this.keyInput = InputState.new({ placeholder: "Optional shortcut" });
      for (const input of [this.nameInput, this.commandInput, this.widthInput, this.heightInput, this.keyInput]) {
        input.on("change", (_e, cx) => { this.problem = ""; cx.notify(); });
        input.on("submit", (_e, cx) => this.add(cx));
      }
    }
    focus() { this.handle.focus(); }
    show(cx) { this.load(cx, ["popups"], data => { this.popups = data.popups; }); }
    add(cx) {
      const name = value(this.nameInput);
      const command = value(this.commandInput);
      const width = value(this.widthInput) || "0.8";
      const height = value(this.heightInput) || "0.8";
      const key = value(this.keyInput);
      if (!name || !command) {
        this.problem = "Enter a name and command.";
        cx.notify();
        return;
      }
      const args = ["popup-add", "--name", name, "--command", command, "--width", width, "--height", height, "--key", key, "--persistent", String(this.persistent)];
      this.task(cx, `Adding ${name}`, args, async () => {
        this.notice = `${name} added.`;
        this.nameInput.set_value("");
        this.commandInput.set_value("");
        this.keyInput.set_value("");
        this.popups = (await this.call(["popups"])).popups;
      });
    }
    remove(popup, cx) {
      this.task(cx, `Removing ${popup.name}`, ["popup-remove", "--name", popup.name], async data => {
        this.notice = data.kept.length ? `Edited files kept: ${data.kept.join(", ")}` : `${popup.name} removed.`;
        this.popups = (await this.call(["popups"])).popups;
      });
    }
    render() {
      if (!this.popups) return this.empty("Loading popups…");
      const t = herdr.theme();
      const field = (label, input) => v_flex().gap(5).child(this.label(label)).child(this.input(input));
      const list = v_flex().gap(8)
        .child(this.heading("Popups"))
        .when(!this.popups.length, el => el.child(div().text_size(12).text_color(t.muted).child("No popups yet. Add one to open a command in a window.")))
        .children(this.popups.map(popup => h_flex().id(`popup-${popup.name}`).p(12).gap(10).items_center().rounded(9).bg(t.surface)
          .child(v_flex().flex_1().min_w(0).gap(4)
            .child(div().text_size(13).font_semibold().child(popup.name))
            .child(div().text_size(12).text_color(t.muted).child(popup.command))
            .child(div().text_size(11).text_color(t.dim).child(`${popup.width} × ${popup.height} · ${popup.key || "No shortcut"} · ${popup.persistent ? "Persistent" : "Ends on close"}${popup.changed ? " · Edited" : ""}`)))
          .child(this.button(`try-${popup.name}`, "Try", () => herdr.run("mod.open", { name: `popup-${popup.name}` })))
          .child(this.button(`remove-${popup.name}`, "Remove", (_e, cx) => this.remove(popup, cx), { danger: true }))));
      const form = v_flex().gap(10)
        .child(this.heading("Add a popup"))
        .child(field("Name", this.nameInput))
        .child(field("Command", this.commandInput))
        .child(h_flex().gap(10)
          .child(v_flex().flex_1().min_w(0).child(field("Width", this.widthInput)))
          .child(v_flex().flex_1().min_w(0).child(field("Height", this.heightInput))))
        .child(div().text_size(11).text_color(t.dim).child("Sizes up to 1 are fractions of the window."))
        .child(field("Key (optional)", this.keyInput))
        .child(this.label("Keep the terminal running after closing"))
        .child(h_flex().gap(6).children(this.segment("popup-persistent", [["false", "Off"], ["true", "On"]], String(this.persistent), (v, cx) => { this.persistent = v === "true"; cx.notify(); })))
        .when(Boolean(this.problem), el => el.child(div().text_size(11.5).text_color(t.blocked).child(this.problem)))
        .child(this.button("popup-add", "Add popup", (_e, cx) => this.add(cx), { primary: true }));
      return h_flex().flex_1().min_h(0).gap(16)
        .child(v_flex().flex_1().min_w(0).overflow_y_scrollbar().child(list))
        .child(v_flex().w(320).min_w(0).overflow_y_scrollbar().child(form));
    }
  }
  return page(app, new Page());
}
