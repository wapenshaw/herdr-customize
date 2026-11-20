import { div } from "gpui-kit";
import { h_flex, v_flex, InputState, VirtualListScrollHandle, v_virtual_list } from "gpui-base";
import { theme, themes, settings, run } from "herdr";
import { page, FILTERS, plural, value } from "../lib.js";

export default function create(app) {
  class Page {
    id = "themes";
    title = "Themes";
    init(cx) {
    this.palettes = [];
    this.filter = "all";
    const cur = this.current();
    this.themeSel = cur[cur.showing] ?? "";
    this.themeScroll = VirtualListScrollHandle.new();
    this.themeSearch = InputState.new({ placeholder: "Search themes" });
    this.themeSearch.on("change", (_e, cx) => { this.themeSel = ""; cx.notify(); });
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

  focus() { this.themeSearch.focus(); }
  key(e, cx) {
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
  render() { return this.themesPage(); }
  }
  return page(app, new Page());
}
