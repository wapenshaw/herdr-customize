import { div } from "gpui-kit";
import { h_flex, v_flex, InputState } from "gpui-base";
import * as herdr from "herdr";
import { page, platform, value } from "../lib.js";

export default function create(app) {
  class Page {
    id = "popups";
    title = "Popups";
    init() {
      this.popups = null;
      this.problem = "";
      this.persistent = false;
      this.nameInput = InputState.new({ placeholder: "git" });
      this.commandInput = InputState.new({ placeholder: "lazygit" });
      this.widthInput = InputState.new({ value: "0.8", placeholder: "0.8" });
      this.heightInput = InputState.new({ value: "0.8", placeholder: "0.8" });
      this.keyInput = InputState.new({ placeholder: "ctrl-shift-g" });
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
    example(name, command, key, cx) {
      this.nameInput.set_value(name);
      this.commandInput.set_value(command);
      this.keyInput.set_value(key);
      this.problem = "";
      cx.notify();
    }
    render() {
      if (!this.popups) return this.empty("Loading popups…");
      const t = herdr.theme();
      const field = (label, input, hint) => v_flex().gap(4).child(this.label(label)).child(this.input(input)).child(div().text_size(11).text_color(t.dim).child(hint));
      const examples = [["git", "lazygit", "ctrl-shift-g"], ["monitor", platform() === "windows" ? "btop4win --tty_on" : "btop --tty_on", "ctrl-shift-u"], ["files", "yazi", "ctrl-shift-r"]];
      const form = v_flex().gap(10).p(14).rounded(9).bg(t.surface)
        .child(div().text_size(15).font_semibold().child("Add a popup"))
        .child(h_flex().gap(6).items_center().flex_wrap()
          .child(div().text_size(11.5).text_color(t.muted).child("Start from an example:"))
          .children(examples.map(([name, command, key]) => this.chip(`example-${name}`, command, value(this.commandInput) === command, (_e, cx) => this.example(name, command, key, cx)))))
        .child(h_flex().gap(12).items_start()
          .child(v_flex().flex_1().min_w(0).child(field("Name", this.nameInput, "Lowercase, like git.")))
          .child(v_flex().flex_1().min_w(0).child(field("Command", this.commandInput, "What runs in the window, like lazygit. Quote arguments that hold spaces."))))
        .child(h_flex().gap(12).items_start()
          .child(v_flex().flex_1().min_w(0).child(field("Width", this.widthInput, "A fraction of the window, 0.2 to 1.")))
          .child(v_flex().flex_1().min_w(0).child(field("Height", this.heightInput, "A fraction of the window, 0.2 to 1.")))
          .child(v_flex().flex_1().min_w(0).child(field("Shortcut (optional)", this.keyInput, "Like ctrl-shift-g. Without one, open it with Try."))))
        .child(h_flex().gap(10).items_center()
          .child(this.label("When you close the window"))
          .children(this.segment("popup-persistent", [["false", "End the program"], ["true", "Keep it running"]], String(this.persistent), (v, cx) => { this.persistent = v === "true"; cx.notify(); })))
        .when(Boolean(this.problem), el => el.child(div().text_size(11.5).text_color(t.blocked).child(this.problem)))
        .child(h_flex().child(this.button("popup-add", "Add popup", (_e, cx) => this.add(cx), { primary: true })));
      const list = v_flex().gap(8)
        .child(this.heading("Your popups"))
        .when(!this.popups.length, el => el.child(div().text_size(12).text_color(t.muted).child("None yet. Add one above; it then shows here with Try and Remove.")))
        .children(this.popups.map(popup => h_flex().id(`popup-${popup.name}`).p(12).gap(10).items_center().rounded(9).bg(t.surface)
          .child(v_flex().flex_1().min_w(0).gap(4)
            .child(div().text_size(13).font_semibold().child(popup.name))
            .child(div().text_size(12).text_color(t.muted).child(popup.command))
            .child(div().text_size(11).text_color(t.dim).child(`${popup.width} × ${popup.height} · ${popup.key || "No shortcut"} · ${popup.persistent ? "Keeps running" : "Ends on close"}${popup.changed ? " · Edited" : ""}`)))
          .child(this.button(`try-${popup.name}`, "Try", () => herdr.run("mod.open", { name: `popup-${popup.name}` })))
          .child(this.button(`remove-${popup.name}`, "Remove", (_e, cx) => this.remove(popup, cx), { danger: true }))));
      return v_flex().id("popup-page").flex_1().min_h(0).overflow_y_scrollbar().gap(12)
        .child(div().text_size(12).text_color(t.muted).child("A popup is a small floating terminal that runs one command, like lazygit or btop, over your work. A shortcut opens it and it goes away when you are done."))
        .child(form)
        .child(list);
    }
  }
  return page(app, new Page());
}
