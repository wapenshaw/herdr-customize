import { View, div } from "gpui-kit";
import { h_flex, v_flex, Input, ScrollHandle } from "gpui-base";
import * as herdr from "herdr";
import { REPO, reason, platform, chord } from "./lib.js";
import * as pages from "./pages.js";

const { exec, home, appHome, theme, run, props, closeOverlay } = herdr;
const BIN = "@BIN@";
const PAGE_REGISTRY = Object.values(pages);
let keysBound = false;

export default class Customize extends View {
  init(_props, cx) {
    this.tab = "themes";
    this.handle = cx.focus_handle();
    this.busy = "";
    this.error = "";
    this.notice = "";
    this.problems = [];
    this.healthVersion = 0;
    this.dismissedHealth = "";
    this.tabScroll = ScrollHandle.new();
    this.pageList = PAGE_REGISTRY.map(create => create(this));
    this.pages = Object.fromEntries(this.pageList.map(page => [page.id, page]));
    for (const page of this.pageList) page.init(cx);

    if (!keysBound) {
      try {
        cx.bind_keys([
          { keystroke: "tab", action: "customize.next_screen", context: "Customize" },
          { keystroke: "shift-tab", action: "customize.prev_screen", context: "Customize" },
        ]);
      } catch {}
      keysBound = true;
    }

    const checking = herdr.checking?.() ?? !herdr.window();
    this.binFound = checking ? Promise.resolve(null) : this.findBin();
    if (!checking) this.loadAll(cx);
    const themes = this.pages.themes;
    themes.themeSearch.focus();
    cx.timer.after(300, cx => {
      themes.themeSearch.focus();
      const at = themes.themeRows().findIndex(r => r.name === themes.themeSel);
      if (at >= 0) themes.themeScroll.scroll_to_item(at, "center");
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
          try { const data = JSON.parse(r.stdout); if (!data.error) this.pages.mods.packages = data; } catch {}
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
        this.refreshHealth(cx);
        this.load(cx, ["palettes"], data => { this.pages.themes.palettes = data.palettes; });
        if (!this.pages.mods.packages) this.pages.mods.refreshPackages(cx);
      }
      cx.notify();
    });
  }

  async call(args) {
    const bin = await this.binFound;
    if (!bin) throw new Error("herdr-customize is not installed");
    // exec's default 30 s is shorter than a download or a first Browse may take.
    const r = await exec(bin, ["app", "--home", appHome(), "api", ...args], { timeout: 600000 });
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

  refreshHealth(cx) {
    const version = ++this.healthVersion;
    cx.spawn(async cx => {
      try {
        const data = await this.call(["health"]);
        if (version === this.healthVersion) {
          if (JSON.stringify(this.problems) !== JSON.stringify(data.problems)) this.dismissedHealth = "";
          this.problems = data.problems;
        }
      } catch (e) {
        if (version === this.healthVersion) this.error = reason(e);
      }
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
      this.refreshHealth(cx);
      this.busy = "";
      cx.notify();
    });
  }

  close() {
    if (props()?.overlay) closeOverlay(props().overlay);
  }

  stepTab(by, cx) {
    const at = this.pageList.findIndex(page => page.id === this.tab);
    this.showTab(this.pageList[(at + by + this.pageList.length) % this.pageList.length].id, cx);
  }

  showTab(tab, cx) {
    this.tab = tab;
    this.tabScroll.scroll_to_item(this.pageList.findIndex(page => page.id === tab));
    this.error = "";
    this.notice = "";
    this.pages.keys.recording = false;
    this.focusPage();
    this.pages[tab].show?.(cx);
    cx.notify();
  }

  focusPage() { this.pages[this.tab].focus(); }

  onKey(e, cx) {
    if (this.pages.keys.recording) { this.pages.keys.record(e, cx); return; }
    if (e.key === "escape") {
      if (this.pages.mods.reviewed) this.pages.mods.closeReview(cx); else this.close();
      return;
    }
    if (chord(e.modifiers)) return;
    if (e.key === "tab") {
      this.stepTab(e.modifiers.shift ? -1 : 1, cx);
      cx.stop_propagation();
      return;
    }
    this.pages[this.tab].key?.(e, cx);
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

  closeAndRun(command, args) {
    this.close();
    run(command, args);
  }

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
      : this.pages[this.tab].render();
    return v_flex().size_full().min_h(0).bg(t.popover).text_color(t.foreground)
      .key_context("Customize")
      .on_action("customize.next_screen", (_e, cx) => this.stepTab(1, cx))
      .on_action("customize.prev_screen", (_e, cx) => this.stepTab(-1, cx))
      .track_focus(this.handle).on_key_down((e, cx) => this.onKey(e, cx))
      .child(h_flex().px(20).h(52).gap(14).items_center().border_b(1).border_color(t.border).flex_none()
        .child(div().text_size(14).font_semibold().child("Customize"))
        .child(h_flex().id("tabs").gap(2).p(3).rounded(8).bg(t.surface).flex_1().min_w(0).overflow_x_scroll().track_scroll(this.tabScroll)
          .children(this.pageList.map(({ id, title }) => div().id(`tab-${id}`).px(8).py(5).rounded(6).flex_none()
          .text_size(12).font_medium().cursor_pointer().bg(this.tab === id ? t.raised : t.surface).text_color(this.tab === id ? t.foreground : t.muted)
          .hover(s => s.text_color(t.foreground)).on_click((_e, cx) => this.showTab(id, cx)).child(title))))
        .child(div().flex_1())
        .when(Boolean(this.busy), el => el.child(div().text_size(11.5).text_color(t.muted).child(`${this.busy}…`)))
        .child(this.button("close", "Close", () => this.close(), { always: true })))
      .when(this.problems.length > 0 && this.dismissedHealth !== JSON.stringify(this.problems), el => el.child(
        h_flex().mx(20).mt(10).p(10).gap(10).rounded(8).bg(`${t.blocked}18`).text_color(t.blocked).items_start().flex_none()
          .child(v_flex().gap(3).flex_1().min_w(0).children(this.problems.map((p, i) =>
            div().id(`health-${i}`).text_size(11.5).child(`${p.source}: ${p.message}`))))
          .child(div().id("dismiss-health").cursor_pointer().text_size(11.5).child("Dismiss")
            .on_click((_e, cx) => { this.dismissedHealth = JSON.stringify(this.problems); cx.notify(); }))))
      .child(v_flex().px(20).pt(14).pb(12).flex_1().min_h(0).child(page))
      .when(Boolean(this.error || this.notice), el => el.child(div().mx(20).mb(14).p(10).rounded(8).flex_none()
        .bg(this.error ? `${t.blocked}18` : `${t.done}18`).text_size(11.5).text_color(this.error ? t.blocked : t.done).child(this.error || this.notice)));
  }
}
