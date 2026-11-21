import { div } from "gpui-kit";
import { h_flex, v_flex, InputState } from "gpui-base";
import * as herdr from "herdr";
import { page, value, platform } from "../lib.js";

const { theme } = herdr;

// An argument with a space (a -Command script) can't survive splitting, so such a list shows as JSON.
const formatArgs = args => args.some(a => /\s/.test(a)) ? JSON.stringify(args) : args.join(" ");

function parseArgs(text) {
  const trimmed = text.trim();
  if (trimmed.startsWith("[")) {
    try {
      const list = JSON.parse(trimmed);
      if (Array.isArray(list) && list.every(a => typeof a === "string")) return list.filter(Boolean);
    } catch {}
  }
  return trimmed.split(/\s+/).filter(Boolean);
}

const FRESH_COMMAND = "hc server restart --fresh";

export default function create(app) {
  class Page {
    id = "core";
    title = "Core";
    init(cx) {
    this.core = null;
    this.coreSel = "shell";
    this.coreProblem = "";
    this.coreInput = InputState.new({ placeholder: "" });
    this.shellArgs = InputState.new({ placeholder: "" });
    for (const input of [this.coreInput, this.shellArgs]) {
      input.on("change", (_e, cx) => { this.coreProblem = ""; cx.notify(); });
      input.on("submit", (_e, cx) => this.saveCore(cx));
    }
    }

  // Copied in by hand, the screen can't find the program it runs.
  coreSetting() {
    return this.core?.settings.find(s => s.key === this.coreSel);
  }

  // The inputs start from the saved value; a shell splits into its program
  // and the arguments after it.
  pickCore(key, cx) {
    this.coreSel = key;
    this.coreProblem = "";
    const s = this.coreSetting();
    if (s?.key === "shell") {
      const argv = s.value ? JSON.parse(s.value) : [];
      this.coreInput.set_value(argv[0] ?? "");
      this.shellArgs.set_value(formatArgs(argv.slice(1)));
    } else this.coreInput.set_value(s?.value ?? "");
    this.focusPage();
    cx.notify();
  }

  // Arguments are split at spaces, or read as a JSON list when they start with [.
  coreText() {
    if (this.coreSel !== "shell") return value(this.coreInput);
    const program = value(this.coreInput);
    return program ? JSON.stringify([program, ...parseArgs(value(this.shellArgs))]) : "";
  }

  saveCore(cx, text = this.coreText()) {
    const s = this.coreSetting();
    if (!s) return;
    if (!text) {
      this.coreProblem = "Type a value, or use the default.";
      cx.notify();
      return;
    }
    this.task(cx, `Saving ${s.title.toLowerCase()}`, ["core-set", "--key", s.key, "--value", text], (data, cx) => {
      this.core = data;
      this.notice = s.key === "restore_screen" ? "Restore screen saved; it applies at the next recovery." : `${s.title} saved; the core uses it the next time it needs it.`;
      this.pickCore(s.key, cx);
    });
  }

  resetCore(cx) {
    const s = this.coreSetting();
    this.task(cx, `Resetting ${s.title.toLowerCase()}`, ["core-set", "--key", s.key, "--default"], (data, cx) => {
      this.core = data;
      this.notice = `${s.title} is back to the default.`;
      this.pickCore(s.key, cx);
    });
  }

  updatesCard() {
    if (typeof herdr.updates !== "function") return null;
    const t = theme();
    const updates = herdr.updates();
    const commands = typeof herdr.commands === "function" ? herdr.commands() : [];
    const has = name => commands.some(command => command.name === name);
    const line = text => div().text_size(11.5).text_color(t.muted).child(text);
    const card = v_flex().gap(6).p(12).rounded(8).bg(t.surface)
      .child(div().text_size(13).font_semibold().child("Updates"))
      .child(line(`App ${updates.app.version}${updates.app.ready ? ` · update ${updates.app.ready} ready` : " · no update ready"}`))
      .child(line(`Core ${updates.core.version ?? "unknown"}${updates.core.managed ? ` · managed by ${updates.core.managed}` : " · not package-managed"}${updates.core.ready ? ` · update ${updates.core.ready} ready` : " · no update ready"}`))
      .child(line(updates.checked == null ? "No successful check recorded" : `Checked ${new Date(updates.checked * 1000).toLocaleString()}`))
      .when(Boolean(updates.core.error), el => el.child(div().text_size(11.5).text_color(t.blocked).child(updates.core.error)));
    const actions = h_flex().gap(8).flex_wrap()
      .when(has("app.check_updates"), el => el.child(this.button("check-updates", "Check for updates", () => herdr.run("app.check_updates"))))
      .when(Boolean(updates.core.ready && !updates.core.managed && has("app.update_core")), el =>
        el.child(this.button("update-core", "Update core", () => herdr.run("app.update_core"))))
      .when(Boolean(updates.app.ready && has("app.update")), el =>
        el.child(this.button("update-app", "Restart app into update", () => herdr.run("app.update"))));
    return card.child(actions).child(line("Updating the core keeps terminals running."));
  }

  corePage() {
    const t = theme();
    if (!this.core) return this.empty("Reading the core's settings…");
    const s = this.coreSetting();
    const hint = text => div().text_size(11).text_color(t.dim).child(text);
    const shown = x => x.key === "shell" ? JSON.parse(x.value).join(" ") : ["notes", "restore_screen"].includes(x.key) ? (x.value === "false" ? "off" : "on") : x.value;
    const rows = this.core.settings.map(x => h_flex().id(`core-${x.key}`).h(36).px(10).gap(10).items_center().rounded(6).cursor_pointer()
      .bg(x.key === this.coreSel ? t.selected : t.popover).hover(st => st.bg(t.hover))
      .on_click((_e, cx) => this.pickCore(x.key, cx))
      .child(div().w(130).flex_none().text_size(12.5).text_color(t.foreground).child(x.title))
      .child(div().flex_1().min_w(0).text_size(12).whitespace_nowrap().text_ellipsis().text_color(x.value ? t.muted : t.dim).child(x.value ? shown(x) : `default: ${x.default}`)));
    const detail = v_flex().flex_1().min_w(0).gap(10)
      .child(div().text_size(18).font_semibold().child(s.title))
      .child(div().text_size(12).text_color(t.muted).child(s.about));
    if (s.key === "notes" || s.key === "restore_screen") {
      const on = s.value ?? (s.default === "on" ? "true" : "false");
      detail.child(h_flex().gap(6).children(this.segment(s.key, [["true", "On"], ["false", "Off"]], on, (v, cx) => this.saveCore(cx, v))));
    } else if (s.key === "shell") {
      // PowerShell is the core's default on Windows, with a hook that follows the
      // shell's folder, so picking it clears the setting rather than naming it.
      const powershell = platform() === "windows" ? [this.chip("shell-default", "PowerShell (default)", !s.value, (_e, cx) => { if (s.value) this.resetCore(cx); })] : [];
      const program = value(this.coreInput).toLowerCase();
      // Without the core's -Command hook, cd in the shell never reaches programs it starts.
      const hookLost = platform() === "windows" && /(^|[\\/])(pwsh|powershell)(\.exe)?$/.test(program) && !parseArgs(value(this.shellArgs)).some(a => /^-c(ommand)?$/i.test(a));
      detail.child(h_flex().gap(6).flex_wrap().children([...powershell, ...this.core.shells.map((sh, i) =>
        this.chip(`shell-${i}`, sh.name, s.value === JSON.stringify(sh.argv), (_e, cx) => this.saveCore(cx, JSON.stringify(sh.argv))))]))
        .child(v_flex().gap(5).child(this.label("Program")).child(this.input(this.coreInput)).child(hint(s.hint)))
        .child(v_flex().gap(5).child(this.label("Arguments")).child(this.input(this.shellArgs))
          .child(hint("Split at spaces, like --login -i for Git Bash. An argument that holds spaces shows the list as JSON, like [\"-NoLogo\", \"-Command\", \"...\"]; edit it in that form. Leave empty for none.")))
        .when(hookLost, el => el.child(div().text_size(11.5).text_color(t.working).child("Naming PowerShell here replaces the core's default start-up: new terminals no longer follow cd for the programs they start. Choose PowerShell (default) unless you need other arguments.")));
    } else {
      detail.child(v_flex().gap(5).child(this.input(this.coreInput)).child(hint(s.hint)).child(hint(`Default: ${s.default}`)));
    }
    const updates = this.updatesCard();
    return v_flex().flex_1().min_h(0).gap(10)
      .when(Boolean(updates), el => el.child(updates))
      .child(h_flex().gap(8).items_center()
        .child(div().flex_1().text_size(11.5).text_color(t.muted).child("To make new terminals take your shell's environment without ending programs, run hc server restart --fresh from a terminal outside Herdr."))
        .child(this.button("copy-fresh", "Copy", (_e, cx) => { cx.write_to_clipboard(FRESH_COMMAND); this.notice = "Copied the restart command."; cx.notify(); })))
      .child(this.split(420, [
      v_flex().id("core-settings").flex_1().min_h(0).overflow_y_scrollbar().gap(4).children(rows),
      div().text_size(11).text_color(t.dim).child(`Saved to ${this.core.path}. The core reads it each time it needs a setting; screen restoration applies at the next recovery.`),
    ], detail
      .when(Boolean(this.core.error), el => el.child(div().text_size(11.5).text_color(t.blocked).child(this.core.error)))
      .when(Boolean(this.coreProblem), el => el.child(div().text_size(11.5).text_color(t.blocked).child(this.coreProblem)))
      .child(h_flex().gap(8).mt(4)
        .when(s.key !== "notes" && s.key !== "restore_screen", el => el.child(this.button("core-save", "Save", (_e, cx) => this.saveCore(cx), { primary: true })))
        .when(Boolean(s.value), el => el.child(this.button("core-default", `Use the default (${s.default})`, (_e, cx) => this.resetCore(cx)))))));
  }

  focus() { if (this.core && this.coreSel !== "notes" && this.coreSel !== "restore_screen") this.coreInput.focus(); else this.handle.focus(); }
  show(cx) { if (!this.core) this.load(cx, ["core"], (data, cx) => { this.core = data; this.pickCore(this.coreSel, cx); }); }
  render() { return this.corePage(); }
  }
  return page(app, new Page());
}
