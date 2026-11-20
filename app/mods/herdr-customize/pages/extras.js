import { div } from "gpui-kit";
import { h_flex, v_flex } from "gpui-base";
import { theme } from "herdr";
import { page } from "../lib.js";

export default function create(app) {
  class Page {
    id = "extras";
    title = "Extras";
    init() { this.extras = null; }
    focus() { this.handle.focus(); }
    show(cx) { this.load(cx, ["extras"], data => { this.extras = data.extras; }); }
    change(extra, action, cx) {
      this.task(cx, `${action === "extras-remove" ? "Removing" : "Installing"} ${extra.title}`, [action, "--name", extra.name], async data => {
        const messages = [];
        if (data.added.length) messages.push(`Added: ${data.added.join(", ")}`);
        if (data.skipped.length) messages.push(`Skipped: ${data.skipped.join(", ")}`);
        if (data.kept.length) messages.push(`Edited files kept: ${data.kept.join(", ")}`);
        this.notice = messages.length ? messages.join(" · ") : `${extra.title} ${action === "extras-remove" ? "removed" : "installed"}.`;
        this.extras = (await this.call(["extras"])).extras;
      });
    }
    render() {
      if (!this.extras) return this.empty("Loading…");
      const t = theme();
      return v_flex().id("extras-list").flex_1().min_h(0).overflow_y_scrollbar().gap(8)
        .children(this.extras.map(extra => h_flex().id(`extra-${extra.name}`).p(12).gap(10).items_center().rounded(9).bg(t.surface)
          .child(v_flex().flex_1().min_w(0).gap(4)
            .child(div().text_size(13).font_semibold().child(extra.title))
            .child(div().text_size(11.5).text_color(t.muted).child(extra.about))
            .child(div().text_size(11).text_color(t.dim).child(extra.installed ? (extra.changed ? "Installed · edited or not managed here" : "Installed") : "Not installed"))
            .when(!extra.available, el => el.child(div().text_size(11).text_color(t.muted).child(`Needs Herdr 0.1.16 or newer; missing: ${extra.missing.join(", ")}`)))
            .children(extra.keys.map(key => div().text_size(11).text_color(t.dim).child(`${key.section}: ${key.key} · ${key.status}${key.conflict ? ` (${key.conflict})` : ""}`))))
          .child(this.button(`install-${extra.name}`, extra.installed ? "Update" : "Install", (_e, cx) => this.change(extra, "extras-install", cx), { enabled: extra.available && !extra.changed }))
          .when(extra.installed, el => el.child(this.button(`remove-${extra.name}`, "Remove", (_e, cx) => this.change(extra, "extras-remove", cx), { danger: true })))));
    }
  }
  return page(app, new Page());
}
