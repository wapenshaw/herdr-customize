import { div } from "gpui-kit";
import { h_flex, v_flex, InputState } from "gpui-base";
import { theme } from "herdr";
import { page, value, platform } from "../lib.js";

export default function create(app) {
  class Page {
    id = "app";
    title = "App";
    init(_cx) {
      this.settings = null;
      this.selected = "zoom";
      this.edit = InputState.new({ placeholder: "" });
      this.bindInput();
    }
    current() { return this.settings?.settings.find(s => s.key === this.selected); }
    numericDefault(s) { return s?.kind === "input" && /^\d+(\.\d+)?$/.test(s.default) ? s.default : ""; }
    // Enter does not save: only the Save button writes.
    bindInput() {
      this.edit.on("change", (_e, cx) => cx.notify());
    }
    pick(key, cx) {
      this.selected = key;
      const s = this.current();
      this.edit.release();
      this.edit = InputState.new({ placeholder: this.numericDefault(s), value: s?.value ?? "" });
      this.bindInput();
      this.focusPage();
      cx.notify();
    }
    // An empty draft means the default: Save then removes the setting.
    save(cx) {
      const s = this.current();
      if (!s) return;
      const text = value(this.edit);
      if (!text) { if (s.value !== null) this.reset(cx); return; }
      this.task(cx, `Saving ${s.title.toLowerCase()}`, ["settings-set", "--key", s.key, "--value", text], (data, cx) => {
        this.settings = data;
        this.notice = s.restart ? `${s.title} saved; restart the app to apply it.` : `${s.title} saved.`;
        this.pick(s.key, cx);
      });
    }
    reset(cx) {
      const s = this.current();
      this.task(cx, `Resetting ${s.title.toLowerCase()}`, ["settings-set", "--key", s.key, "--default"], (data, cx) => {
        this.settings = data;
        this.notice = s.restart ? `${s.title} reset; restart the app to apply it.` : `${s.title} is back to the default.`;
        this.pick(s.key, cx);
      });
    }
    focus() { if (this.settings && this.current()?.kind === "input") this.edit.focus(); else this.handle.focus(); }
    show(cx) { if (!this.settings) this.load(cx, ["settings"], (data, cx) => { this.settings = data; this.pick(this.selected, cx); }); }
    render() {
      const t = theme();
      if (!this.settings) return this.empty("Reading the app's settings…");
      const s = this.current();
      const visible = this.settings.settings.filter(x => !x.macos_only || platform() === "macos");
      const groups = ["Look", "Terminal", "Input and clipboard", "Behavior"];
      const rows = groups.map(group => v_flex().gap(4).child(this.heading(group)).children(visible.filter(x => x.group === group).map(x =>
        h_flex().id(`app-${x.key}`).h(36).px(10).gap(10).items_center().rounded(6).cursor_pointer()
          .bg(x.key === this.selected ? t.selected : t.popover).hover(st => st.bg(t.hover))
          .on_click((_e, cx) => this.pick(x.key, cx))
          .child(div().w(150).flex_none().text_size(12.5).text_color(t.foreground).child(x.title))
          .child(div().flex_1().min_w(0).text_size(12).whitespace_nowrap().text_ellipsis().text_color(x.value !== null ? t.muted : t.dim).child(x.value ?? `default: ${x.default}`)))));
      const detail = v_flex().flex_1().min_w(0).gap(10)
        .child(div().text_size(18).font_semibold().child(s.title))
        .child(div().text_size(12).text_color(t.muted).child(s.about))
        .child(div().text_size(11).text_color(t.dim).child(s.restart ? "Needs app restart." : "Applies live."));
      // A pick only fills the draft; nothing is written until Save.
      const draft = value(this.edit);
      if (s.kind === "toggle") {
        detail.child(h_flex().gap(6).children(this.segment("app-toggle", [["true", "On"], ["false", "Off"]], draft || s.default, (v, cx) => { this.edit.set_value(v); cx.notify(); })));
      } else if (s.kind === "choice") {
        detail.child(h_flex().gap(6).flex_wrap().children(s.choices.map(v => this.chip(`app-choice-${v}`, v, (draft || s.default) === v, (_e, cx) => { this.edit.set_value(v); cx.notify(); }))));
      } else {
        detail.child(v_flex().gap(5).child(this.input(this.edit)).child(div().text_size(11).text_color(t.dim).child(s.hint)));
      }
      detail.child(div().text_size(11).text_color(t.dim).child(`Default: ${s.default}`))
        .when(Boolean(this.settings.error), el => el.child(div().text_size(11.5).text_color(t.blocked).child(this.settings.error)))
        .when(!draft && s.value !== null, el => el.child(div().text_size(11).text_color(t.dim).child(`Save puts ${s.title.toLowerCase()} back to the default.`)))
        .child(h_flex().gap(8).mt(4)
          .child(this.button("app-save", "Save", (_e, cx) => this.save(cx), { primary: true, enabled: draft !== (s.value ?? "") }))
          .when(s.value !== null && Boolean(draft), el => el.child(this.button("app-default", `Use the default (${s.default})`, (_e, cx) => { this.edit.set_value(""); cx.notify(); }))));
      return this.split(420, [
        v_flex().id("app-settings").flex_1().min_h(0).overflow_y_scrollbar().gap(4).children(rows),
        div().text_size(11).text_color(t.dim).child(`Saved to ${this.settings.path}.`),
      ], detail);
    }
  }
  return page(app, new Page());
}
