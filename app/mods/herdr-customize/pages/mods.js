import { div, image } from "gpui-kit";
import { h_flex, v_flex, InputState, TextView } from "gpui-base";
import { theme, settings } from "herdr";
import { page, value, looksLikeRepo, plural, REPO } from "../lib.js";

export default function create(app) {
  class Page {
    id = "mods";
    title = "Mods";
    init(cx) {
    this.modView = "installed";
    this.packages = null;
    this.market = null;
    this.topic = "mods";
    this.sort = "stars";
    this.page = 1;
    this.marketSearch = InputState.new({ placeholder: "Search, or owner/repo to review" });
    this.marketSearch.on("submit", (_e, cx) => {
      const text = value(this.marketSearch);
      if (looksLikeRepo(text)) this.review(text, cx);
      else { this.page = 1; this.loadMarket(cx); }
    });
    this.bundledMods = null;
    this.reviewed = null;
    }

  closeReview(cx) {
    this.reviewed = null;
    this.focusPage();
    cx.notify();
  }

  refreshPackages(cx) {
    this.load(cx, ["packages"], data => { this.packages = data; });
  }

  loadMarket(cx) {
    const args = ["market", "--topic", this.topic, "--sort", this.sort, "--page", String(this.page), "--query", value(this.marketSearch)];
    this.task(cx, "Reading GitHub", args, data => { this.market = data; });
  }

  // from is the Browse row it came from, for the repository's stars.
  review(spec, cx, from = null) {
    this.task(cx, `Reading ${spec}`, ["review", spec], data => {
      if (!data.plans.length) throw new Error(`${spec} has no mods, layouts or themes`);
      this.reviewed = { plans: data.plans, at: 0, takeKeys: false, replaceEdits: false, from };
      this.focusPage();
    });
  }

  install(cx) {
    const r = this.reviewed;
    const p = r.plans[r.at];
    const args = ["install", "--repo", p.repo, "--subdir", p.subdir, "--commit", p.commit];
    if (r.takeKeys) args.push("--take-keys");
    this.task(cx, `Installing ${p.label}`, args, async data => {
      const failed = data.checks.filter(c => c.error);
      this.notice = failed.length ? "" : p.layouts.length ? `${p.label} is installed. Switch to it on the Layouts tab.` : `${p.label} is installed.`;
      if (failed.length) this.error = failed.map(c => `${c.name}: ${c.error}`).join("\n");
      this.reviewed = null;
      this.modView = "installed";
      this.focusPage();
      // Read before the busy label clears, so the list already shows the change.
      this.packages = await this.call(["packages"]);
    });
  }

  remove(pkg, cx) {
    const args = ["remove", "--repo", pkg.repo];
    if (pkg.subdir) args.push("--subdir", pkg.subdir);
    this.task(cx, `Removing ${pkg.label}`, args, async data => {
      this.notice = data.kept.length ? `${pkg.label} is removed; your edited files stay: ${data.kept.join(", ")}.` : `${pkg.label} is removed.`;
      this.packages = await this.call(["packages"]);
    });
  }

  openThing(kind, name) {
    this.closeAndRun(kind === "layout" ? "layout.switch" : "mod.open", { name });
  }

  modsPage() {
    const t = theme();
    const body = this.reviewed ? this.reviewPanel() : this.modView === "browse" ? this.browse() : this.modView === "bundled" ? this.bundled() : this.installed();
    return v_flex().flex_1().min_h(0).gap(10)
      .when(!this.reviewed, el => el.child(h_flex().gap(5)
        .children(this.segment("mods", [["installed", "Installed"], ["bundled", "Bundled with Customize"], ["browse", "Browse GitHub"]], this.modView, (view, cx) => {
          this.modView = view;
          this.focusPage();
          if (view === "browse" && !this.market) this.loadMarket(cx);
          cx.notify();
        }))
        .child(div().flex_1())
        .child(div().text_size(11).text_color(t.dim).child("Mods and layouts from the herdr-mod and herdr-layout topics."))))
      .child(body);
  }

  changeBundled(mod, action, cx) {
    this.task(cx, `${action === "extras-remove" ? "Removing" : "Installing"} ${mod.title}`, [action, "--name", mod.name], async data => {
      const messages = [];
      if (data.added.length) messages.push(`Added: ${data.added.join(", ")}`);
      if (data.skipped.length) messages.push(`Skipped: ${data.skipped.join(", ")}`);
      if (data.kept.length) messages.push(`Edited files kept: ${data.kept.join(", ")}`);
      this.notice = messages.length ? messages.join(" · ") : `${mod.title} ${action === "extras-remove" ? "removed" : "installed"}.`;
      this.bundledMods = (await this.call(["extras"])).extras;
    });
  }

  bundled() {
    const t = theme();
    if (!this.bundledMods) return this.empty("Loading…");
    return v_flex().id("bundled").flex_1().min_h(0).overflow_y_scrollbar().gap(8)
      .children(this.bundledMods.map(mod => h_flex().id(`bundled-${mod.name}`).p(12).gap(10).items_center().rounded(9).bg(t.surface)
        .child(v_flex().flex_1().min_w(0).gap(4)
          .child(div().text_size(13).font_semibold().child(mod.title))
          .child(div().text_size(11.5).text_color(t.muted).child(mod.about))
          .child(div().text_size(11).text_color(t.dim).child(mod.installed ? (mod.changed ? "Installed · edited or not managed here" : "Installed") : "Not installed"))
          .when(!mod.available, el => el.child(div().text_size(11).text_color(t.muted).child(`Needs Herdr 0.1.16 or newer; missing: ${mod.missing.join(", ")}`)))
          .children(mod.keys.map(key => div().text_size(11).text_color(t.dim).child(`${key.section}: ${key.key} · ${key.status}${key.conflict ? ` (${key.conflict})` : ""}`))))
        .child(this.button(`install-${mod.name}`, mod.installed ? "Update" : "Install", (_e, cx) => this.changeBundled(mod, "extras-install", cx), { enabled: mod.available && !mod.changed }))
        .when(mod.installed, el => el.child(this.button(`remove-${mod.name}`, "Remove", (_e, cx) => this.changeBundled(mod, "extras-remove", cx), { danger: true })))));
  }

  installed() {
    const t = theme();
    const p = this.packages;
    if (!p) return this.empty("Loading…");
    const counts = pkg => [plural(pkg.mods.length, "mod"), plural(pkg.layouts.length, "layout"), plural(pkg.themes.length, "theme"), plural(pkg.keys.length, "key")]
      .filter(s => !s.startsWith("0 ")).join(" · ");
    // The app's own mods are parts of its window, not screens to open.
    const open = (kind, name, i, opens = true) => h_flex().id(`open-${kind}-${i}`).px(9).py(5).gap(6).items_center().rounded(6).bg(t.surface)
      .when(opens, el => el.cursor_pointer().hover(s => s.bg(t.hover)).on_click(() => this.openThing(kind, name)))
      .child(div().text_size(12).text_color(opens ? t.foreground : t.muted).child(name)).child(div().text_size(10.5).text_color(t.dim).child(kind));
    const shipped = p.shipped.map(s => s.startsWith("layout:") ? ["layout", s.slice(7)] : ["mod", s]);
    return v_flex().id("installed").flex_1().min_h(0).overflow_y_scrollbar().gap(8)
      .child(this.heading("Installed here"))
      // This screen updates with the program, not as a package, so it has no buttons of its own.
      .child(h_flex().id("pkg-self").p(12).gap(10).items_center().rounded(9).bg(t.surface)
        .child(v_flex().flex_1().min_w(0).gap(4)
          .child(div().text_size(13).font_semibold().child("Customize"))
          .child(div().text_size(11.5).text_color(t.muted).child(`This screen · herdr-customize ${p.version ?? ""} · updates with the program`)))
        .child(this.button("self-releases", "What's new", (_e, cx) => cx.open_url(`${REPO}/releases`), { always: true })))
      .when(!p.installed.length, el => el.child(this.empty("No packages from GitHub yet. Browse GitHub to find mods and layouts.")))
      .children(p.installed.map((pkg, i) => h_flex().id(`pkg-${i}`).p(12).gap(10).items_center().rounded(9).bg(t.surface)
        .child(v_flex().flex_1().min_w(0).gap(4)
          .child(div().text_size(13).font_semibold().child(pkg.label))
          .child(div().text_size(11.5).text_color(t.muted).child(`${counts(pkg)} · ${pkg.commit.slice(0, 7)}`)))
        .child(this.button(`update-${i}`, "Check for update", (_e, cx) => this.review(pkg.subdir ? `${pkg.repo}/${pkg.subdir}` : pkg.repo, cx)))
        .child(this.button(`remove-${i}`, "Remove", (_e, cx) => this.remove(pkg, cx), { danger: true }))))
      .when(p.added.length > 0, el => el.child(this.heading("Added by hand"))
        .child(h_flex().flex_wrap().gap(6).children(p.added.map((a, i) => open(a.kind, a.name, `a${i}`)))))
      .child(this.heading("Shipped with the app"))
      .child(h_flex().flex_wrap().gap(6).children(shipped.map(([kind, name], i) => open(kind, name, `s${i}`, kind === "layout"))));
  }

  // layoutsOnly is the Layouts tab's view of the same list: no Mods/Layouts choice.
  browse(layoutsOnly = false) {
    const t = theme();
    const m = this.market;
    const pages = m ? Math.max(1, Math.ceil(Math.min(m.total, 1000) / m.per_page)) : 1;
    const go = (page, cx) => { this.page = page; this.loadMarket(cx); };
    return v_flex().flex_1().min_h(0).gap(8)
      .child(h_flex().gap(6).items_center()
        .child(div().flex_1().child(this.input(this.marketSearch)))
        .children(layoutsOnly ? [] : this.segment("topic", [["mods", "Mods"], ["layouts", "Layouts"]], this.topic, (topic, cx) => { this.topic = topic; go(1, cx); }))
        .child(div().w(8))
        .children(this.segment("sort", [["stars", "Stars"], ["updated", "Updated"]], this.sort, (sort, cx) => { this.sort = sort; go(1, cx); })))
      .child(v_flex().id("market").flex_1().min_h(0).overflow_y_scrollbar().gap(6)
        .when(!m, el => el.child(this.busy
          ? this.loading("Reading GitHub", "The first list on this computer reads every repository once and can take up to a minute. After that it opens at once.")
          : this.empty("Press Enter to search.")))
        .when(Boolean(m) && Boolean(this.busy), el => el.child(this.loading(this.busy, "Reading from GitHub.")))
        .when(Boolean(m) && !m.repos.length, el => el.child(this.empty("Nothing found.")))
        .children((m?.repos ?? []).map((r, i) => {
          return h_flex().id(`repo-${i}`).p(10).gap(12).items_center().rounded(9).bg(t.surface).cursor_pointer().hover(s => s.bg(t.hover))
            .on_click((_e, cx) => this.review(r.spec, cx, r))
            .child(div().w(128).h(80).flex_none().rounded(6).overflow_hidden().bg(t.raised)
              .when(Boolean(r.preview), el => el.child(image(r.preview).size_full())))
            .child(v_flex().flex_1().min_w(0).gap(3)
              .child(h_flex().gap(8).items_center()
                .child(div().text_size(13.5).font_semibold().child(r.name))
                .when(r.stars > 0, el => el.child(div().text_size(11).text_color(t.dim).child(`★ ${r.stars}`))))
              .child(div().text_size(11).text_color(t.dim).child(`${r.owner} · ${r.spec}`))
              .when(Boolean(r.blurb), el => el.child(div().text_size(11.5).text_color(t.muted).max_h(34).overflow_hidden().child(r.blurb))))
            .child(this.button(`review-${i}`, "Review", (_e, cx) => this.review(r.spec, cx, r)));
        })))
      .when(Boolean(m) && pages > 1, el => el.child(h_flex().gap(8).items_center().justify_center()
        .child(this.button("page-prev", "Previous", (_e, cx) => go(this.page - 1, cx), { enabled: this.page > 1 }))
        .child(div().text_size(11.5).text_color(t.muted).child(`Page ${this.page} of ${pages}`))
        .child(this.button("page-next", "Next", (_e, cx) => go(this.page + 1, cx), { enabled: this.page < pages }))));
  }

  reviewPanel() {
    const t = theme();
    const r = this.reviewed;
    const p = r.plans[r.at];
    const list = (title, items, color) => items.length ? v_flex().gap(3).child(this.heading(title))
      .children(items.map((item, i) => div().id(`${title}-${i}`).text_size(12).text_color(color ?? t.foreground).child(item))) : div();
    const check = (id, field, text) => h_flex().id(id).gap(8).items_center().cursor_pointer().on_click((_e, cx) => { r[field] = !r[field]; cx.notify(); })
      .child(div().size(14).rounded(3).border_1().border_color(t.border).bg(r[field] ? t.accent : t.input))
      .child(div().text_size(12).child(text));
    const blocked = p.taken.length > 0 || (p.edited.length > 0 && !r.replaceEdits);
    const folder = `https://github.com/${p.repo}${p.subdir ? `/tree/${p.branch}/${p.subdir}` : ""}`;
    const line = `Read ~/.config/herdr-app/AGENTS.md, then add this ${p.layouts.length ? "layout" : "mod"}: ${folder}`;
    // The title is in the header, and pictures beside the recipe can't load here.
    const recipe = p.recipe.replace(/^# .*\n+/, "").replace(/!\[[^\]]*\]\((?![a-z]+:)[^)]*\)\s*/gi, "");
    const blob = `https://github.com/${p.repo}/blob/${p.commit}/${p.subdir ? `${p.subdir}/` : ""}`;
    const stars = r.from?.stars ?? 0;
    return v_flex().flex_1().min_h(0).gap(12)
      .child(h_flex().gap(10).items_center()
        .child(this.button("review-back", "Back", (_e, cx) => this.closeReview(cx)))
        .child(div().text_size(20).font_semibold().child(p.title))
        .child(this.badge(p.update ? "update" : "new", p.update ? t.working : t.done))
        .child(div().text_size(11.5).text_color(t.dim).child(`${p.label} · ${p.commit.slice(0, 7)}`))
        .when(stars > 0, el => el.child(div().text_size(11.5).text_color(t.dim).child(`★ ${stars}`)))
        .child(div().flex_1())
        .child(this.button("open-github", "View on GitHub", (_e, cx) => cx.open_url(folder))))
      .when(Boolean(this.busy), el => el.child(this.loading(this.busy, "Cloning the repository from GitHub. A large one takes about a minute.")))
      .when(r.plans.length > 1, el => el.child(h_flex().flex_wrap().gap(5).children(r.plans.map((q, i) => this.chip(`plan-${i}`, q.title, i === r.at, (_e, cx) => { r.at = i; cx.notify(); })))))
      .child(h_flex().flex_1().min_h(0).gap(18).items_start()
        .child(v_flex().id("review-doc").flex_1().min_w(0).h_full().overflow_y_scrollbar().gap(14).pr(8)
          .when(Boolean(p.summary), el => el.child(div().text_size(13).text_color(t.muted).child(p.summary)))
          .when(Boolean(p.preview), el => el.child(div().w_full().h(340).flex_none().rounded(10).overflow_hidden().bg(t.surface)
            .child(image(p.preview).size_full())))
          .when(Boolean(recipe.trim()), el => el.child(this.heading("Recipe"))
            .child(div().text_size(13).text_color(t.foreground)
              .child(TextView.markdown(`recipe-${p.label}`, recipe).selectable()
                .on_link_click((url, cx) => cx.open_url(/^[a-z][a-z0-9+.-]*:/i.test(url) ? url : blob + url))))))
        .child(v_flex().id("review-side").w(340).flex_none().h_full().overflow_y_scrollbar().gap(12).pr(10)
          .child(list("Mods", p.mods)).child(list("Layouts", p.layouts)).child(list("Themes", p.themes))
          .child(list("Keys", p.keys.map(k => `${k.key} → ${k.action}`)))
          .child(list("Its code", p.uses, t.working))
          .when(p.key_clashes.length > 0, el => el
            .child(list("Keys you already use", p.key_clashes.map(c => `${c.key} runs ${c.held} now; it wants ${c.action}`), t.working))
            .child(check("take-keys", "takeKeys", "Give these keys to the package")))
          .child(list("Already taken", p.taken.map(n => `${n} exists and is not from this package; remove it first`), t.blocked))
          .when(p.edited.length > 0, el => el
            .child(list("Files you changed", p.edited, t.working))
            .child(check("replace-edits", "replaceEdits", "Replace my changes")))
          .child(this.button("install", p.update ? "Update" : "Install", (_e, cx) => this.install(cx), { primary: true, enabled: !blocked }))
          .child(div().text_size(11).text_color(t.dim).child("Read what its code reaches before you install it; mods run with your permissions."))
          .child(div().h(1).bg(t.border))
          .child(this.heading("Or tell your agent"))
          .child(div().text_size(11).text_color(t.dim).child("For a package whose recipe sets something up first, an agent in the app follows it."))
          .child(div().p(10).rounded(7).bg(t.surface).font_family(String(settings().font_family ?? "monospace")).text_size(11.5).child(line))
          .child(this.button("copy-line", "Copy", (_e, cx) => { cx.write_to_clipboard(line); this.notice = "Copied: paste it to an agent in the app."; cx.notify(); }))));
  }

  focus() { if (this.modView === "browse" && !this.reviewed) this.marketSearch.focus(); else this.handle.focus(); }
  show(cx) {
    this.load(cx, ["extras"], data => { this.bundledMods = data.extras; });
    if (this.modView === "browse" && !this.market) this.loadMarket(cx);
  }
  render() { return this.modsPage(); }
  }
  return page(app, new Page());
}
