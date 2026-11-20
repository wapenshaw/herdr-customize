import { div } from "gpui-kit";
import { h_flex, v_flex, InputState } from "gpui-base";
import { theme, shared, run } from "herdr";
import { page, value } from "../lib.js";

const FIELDS = [
  ["theme", "Theme", "Choose one theme or a light and dark pair."],
  ["font", "Terminal font", "Font family for terminals."],
  ["font_size", "Font size", "A positive size in pixels."],
  ["ui_font", "UI font", "Font family for the interface."],
  ["font_weight", "Font weight", "Integer from 100 to 900."],
  ["terminal_padding", "Terminal padding", "A number or [vertical, horizontal]."],
  ["terminal_opacity", "Terminal opacity", "From 0 to 1."],
  ["terminal_line_height", "Line height", "From 0.8 to 2."],
  ["terminal_cell_width", "Cell width", "From 0.8 to 2."],
  ["terminal_scroll_speed", "Scroll speed", "From 0.1 to 5."],
  ["window_blur", "Window blur", "Blur the desktop behind the window where supported."],
  ["titlebar", "Title bar", "Use the system title bar or hide it."],
  ["terminal_menu", "Terminal menu", "A mod that replaces the terminal context menu."],
  ["toast", "Notifications", "A mod that shows notifications."],
  ["link_handlers", "Link handlers", "Mods handle terminal links in the order selected."],
];

export default function create(app) {
  class Page {
    id = "layouts";
    title = "Layouts";
    init() {
      this.data = null;
      this.selected = "";
      this.field = "theme";
      this.cloneName = InputState.new({ placeholder: "New layout name (optional)" });
      this.edit = InputState.new({ placeholder: "Value" });
      this.edit.on("submit", (_e, cx) => this.save(cx, value(this.edit)));
    }
    current() { return this.data?.layouts.find(x => x.name === this.selected); }
    refresh(cx) {
      this.load(cx, ["layouts"], (data, cx) => {
        this.data = data;
        if (!this.current()) this.selected = data.layouts[0]?.name ?? "";
        this.pickField(this.field, cx);
      });
    }
    show(cx) { if (!this.data) this.refresh(cx); }
    focus() { this.handle.focus(); }
    pickField(field, cx) {
      this.field = field;
      const current = this.current()?.root[field];
      this.edit.release();
      this.edit = InputState.new({ placeholder: "Value", value: current == null ? "" : typeof current === "string" ? current : JSON.stringify(current) });
      this.edit.on("submit", (_e, cx) => this.save(cx, value(this.edit)));
      cx.notify();
    }
    save(cx, text) {
      const layout = this.current();
      if (!layout?.personal || layout.error) return;
      this.task(cx, "Saving layout", ["layout-set", "--name", layout.name, "--field", this.field, "--value", text], (data, cx) => {
        this.data = data;
        this.notice = "Layout saved.";
        this.pickField(this.field, cx);
      });
    }
    reset(cx) {
      const layout = this.current();
      if (!layout?.personal || layout.error) return;
      this.task(cx, "Resetting layout field", ["layout-set", "--name", layout.name, "--field", this.field, "--default"], (data, cx) => {
        this.data = data;
        this.notice = "Layout field removed.";
        this.pickField(this.field, cx);
      });
    }
    render() {
      const t = theme();
      if (!this.data) return this.empty("Reading layouts…");
      const layout = this.current();
      const active = shared("layout");
      const left = v_flex().id("layout-list").flex_1().min_h(0).overflow_y_scrollbar().gap(4)
        .children(this.data.layouts.map(row => h_flex().id(`layout-${row.name}`).h(36).px(10).gap(8).items_center().rounded(6).cursor_pointer()
          .bg(row.name === this.selected ? t.selected : t.popover).hover(s => s.bg(t.hover))
          .on_click((_e, cx) => { this.selected = row.name; this.pickField(this.field, cx); })
          .child(div().flex_1().min_w(0).text_size(12.5).child(row.name))
          .when(row.name === active, el => el.child(this.badge("Active", t.done)))
          .when(row.shipped, el => el.child(this.badge("Shipped", t.muted)))));
      if (!layout) return this.split(300, [this.heading("Layouts"), left, this.button("layouts-refresh", "Refresh", (_e, cx) => this.refresh(cx))], this.empty("No layouts found."));
      const field = FIELDS.find(f => f[0] === this.field);
      const current = layout.root[this.field];
      const detail = v_flex().id("layout-detail").flex_1().min_w(0).h_full().overflow_y_scrollbar().gap(10)
        .child(div().text_size(18).font_semibold().child(layout.name))
        .child(h_flex().gap(8).child(this.button("layout-switch", "Switch", () => run("layout.switch", { name: layout.name })))
          .child(this.input(this.cloneName))
          .child(this.button("layout-clone", "Clone", (_e, cx) => {
            const name = value(this.cloneName);
            run("layout.clone", name ? { name: layout.name, as: name } : { name: layout.name });
            this.notice = "Clone requested. Refresh the list to see it.";
            cx.notify();
          })))
        .child(div().text_size(11.5).text_color(t.dim).child(layout.shipped ? "Shipped layouts are read-only. Clone this layout before editing." : "Changes apply to this layout's root; nested nodes are preserved."));
      if (layout.error) detail.child(div().text_size(12).text_color(t.blocked).child(layout.error));
      if (layout.personal && !layout.error) {
        detail.child(this.heading("Root fields"))
          .child(h_flex().gap(5).flex_wrap().children(FIELDS.map(([key, title]) => this.chip(`layout-field-${key}`, title, key === this.field, (_e, cx) => this.pickField(key, cx)))))
          .child(div().text_size(16).font_semibold().child(field[1]))
          .child(div().text_size(11.5).text_color(t.muted).child(field[2]));
        if (this.field === "theme") {
          const pair = typeof current === "object" && current ? current : { light: typeof current === "string" ? current : "paper", dark: typeof current === "string" ? current : "graphite" };
          detail.child(this.label("Single theme"))
            .child(h_flex().gap(5).flex_wrap().children(this.data.themes.map(name => this.chip(`theme-single-${name}`, name, current === name, (_e, cx) => this.save(cx, name)))));
          for (const side of ["light", "dark"]) detail.child(this.label(`${side === "light" ? "Light" : "Dark"} theme`))
            .child(h_flex().gap(5).flex_wrap().children(this.data.themes.map(name => this.chip(`theme-${side}-${name}`, name, typeof current === "object" && current?.[side] === name, (_e, cx) => this.save(cx, JSON.stringify({ ...pair, [side]: name }))))));
        } else if (["terminal_menu", "toast", "link_handlers"].includes(this.field)) {
          const list = Array.isArray(current) ? current : [];
          detail.child(h_flex().gap(5).flex_wrap().children(this.data.mods.map(name => this.chip(`layout-mod-${name}`, name, this.field === "link_handlers" ? list.includes(name) : current === name, (_e, cx) => this.save(cx, this.field === "link_handlers" ? JSON.stringify(list.includes(name) ? list.filter(x => x !== name) : [...list, name]) : name)))));
          if (this.field === "link_handlers") detail.child(div().text_size(11).text_color(t.dim).child(`Order: ${list.join(", ") || "none"}`));
        } else if (["window_blur", "titlebar"].includes(this.field)) {
          const options = this.field === "window_blur" ? [["true", "On"], ["false", "Off"]] : [["system", "System"], ["none", "None"]];
          detail.child(h_flex().gap(5).children(this.segment("layout-choice", options, String(current), (v, cx) => this.save(cx, v))));
        } else {
          detail.child(this.input(this.edit)).child(this.button("layout-save", "Save", (_e, cx) => this.save(cx, value(this.edit)), { primary: true }));
        }
        if (current !== null) detail.child(this.button("layout-default", "Use the default", (_e, cx) => this.reset(cx)));
      }
      return this.split(300, [this.heading("Layouts"), left, this.button("layouts-refresh", "Refresh", (_e, cx) => this.refresh(cx))], detail);
    }
  }
  return page(app, new Page());
}
