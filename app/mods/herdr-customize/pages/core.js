import { div } from "gpui-kit";
import { h_flex, v_flex, InputState } from "gpui-base";
import * as herdr from "herdr";
import { page, value, platform, reason, REPO } from "../lib.js";

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

// Whether version a is before version b, both dotted numbers like 0.1.18.
function older(a, b) {
  if (!a || !b) return false;
  const x = a.split(".").map(Number), y = b.split(".").map(Number);
  for (let i = 0; i < Math.max(x.length, y.length); i++) if ((x[i] ?? 0) !== (y[i] ?? 0)) return (x[i] ?? 0) < (y[i] ?? 0);
  return false;
}

export default function create(app) {
  class Page {
    id = "core";
    title = "Core";
    init(cx) {
    this.core = null;
    this.machine = null;
    this.coreSel = "shell";
    this.coreInput = InputState.new({ placeholder: "" });
    this.shellArgs = InputState.new({ placeholder: "" });
    // Enter does not save: only the Save button writes.
    for (const input of [this.coreInput, this.shellArgs]) input.on("change", (_e, cx) => cx.notify());
    }

  // Copied in by hand, the screen can't find the program it runs.
  coreSetting() {
    return this.core?.settings.find(s => s.key === this.coreSel);
  }

  // The inputs start from the saved value; a shell splits into its program
  // and the arguments after it.
  pickCore(key, cx) {
    this.coreSel = key;
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

  // The settings shown are this.machine's, or this computer's when it is null.
  on(args) {
    return this.machine ? [...args, "--machine", this.machine] : args;
  }

  pickMachine(name, cx) {
    if (name === this.machine) return;
    this.task(cx, `Reading ${name ?? "this computer"}'s settings`, name ? ["core", "--machine", name] : ["core"], (data, cx) => {
      this.machine = name;
      this.core = data;
      this.pickCore(this.coreSel, cx);
    });
  }

  // An empty draft means the core's default: Save then removes the setting.
  saveCore(cx) {
    const s = this.coreSetting();
    if (!s) return;
    const text = this.coreText();
    if (!text) { if (s.value) this.resetCore(cx); return; }
    this.task(cx, `Saving ${s.title.toLowerCase()}`, this.on(["core-set", "--key", s.key, "--value", text]), (data, cx) => {
      this.core = data;
      this.notice = s.key === "restore_screen" ? "Restore screen saved; it applies at the next recovery." : `${s.title} saved; the core uses it the next time it needs it.`;
      this.pickCore(s.key, cx);
    });
  }

  resetCore(cx) {
    const s = this.coreSetting();
    this.task(cx, `Resetting ${s.title.toLowerCase()}`, this.on(["core-set", "--key", s.key, "--default"]), (data, cx) => {
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
    // One column per program: its version, and a badge only when there is news.
    const column = (id, name, version, badge) => v_flex().id(id).gap(2).flex_none()
      .child(div().text_size(10.5).text_color(t.dim).child(name))
      .child(h_flex().gap(6).items_center().child(div().text_size(12.5).child(version ?? "unknown")).when(Boolean(badge), el => el.child(badge)));
    const ready = version => version ? this.badge(`${version} ready`, t.done) : null;
    // hc machine update copies this computer's hc there, so it only moves an older core forward.
    const machines = (this.core?.machines ?? []).filter(m => m.state === "connected");
    const checked = updates.checked == null ? "Never checked" : `Checked ${new Date(updates.checked * 1000).toLocaleString()}`;
    const row = h_flex().gap(20).items_center().flex_wrap()
      .child(div().text_size(13).font_semibold().child("Updates"))
      .child(column("updates-app", "App", updates.app.version, ready(updates.app.ready)))
      .child(column("updates-core", updates.core.managed ? `Core · ${updates.core.managed}` : "Core", updates.core.version, ready(updates.core.ready)))
      .when(Boolean(this.latest), el => el.child(column("updates-customize", "Customize", this.latest.version, this.latest.latest ? this.badge(`${this.latest.latest} available`, t.working) : null)))
      .children(machines.map(m => column(`updates-${m.name}`, m.name, m.version, older(m.version, updates.core.version) ? this.badge("older", t.working) : null)))
      .child(div().flex_1())
      .child(div().text_size(11).text_color(t.dim).child(checked))
      .when(has("app.check_updates"), el => el.child(this.button("check-updates", "Check", (_e, cx) => {
        herdr.run("app.check_updates");
        this.load(cx, ["latest", "--refresh"], data => { app.latest = data; });
      })))
      .when(Boolean(updates.core.ready && !updates.core.managed && has("app.update_core")), el =>
        el.child(this.button("update-core", "Update core", () => herdr.run("app.update_core"))))
      .when(Boolean(updates.app.ready && has("app.update")), el =>
        el.child(this.button("update-app", "Restart into update", () => herdr.run("app.update"))))
      .children(machines.filter(m => older(m.version, updates.core.version)).map(m =>
        this.button(`update-${m.name}`, `Update ${m.name}`, (_e, cx) =>
          this.task(cx, `Updating ${m.name}'s core`, ["core-update-machine", "--machine", m.name], (data, cx) => {
            this.machine = m.name;
            this.core = data;
            this.notice = `${m.name} runs core ${updates.core.version} now; its terminals kept running.`;
            this.pickCore(this.coreSel, cx);
          }))));
    const news = this.latest?.latest;
    const command = this.latest?.command;
    return v_flex().gap(6).px(12).py(10).rounded(8).bg(t.surface).flex_none().child(row)
      .when(Boolean(updates.core.error), el => el.child(div().text_size(11.5).text_color(t.blocked).child(updates.core.error)))
      .when(Boolean(news), el => el.child(h_flex().gap(8).items_center()
        .child(div().flex_1().min_w(0).text_size(11.5).text_color(t.muted).child(command
          ? `herdr-customize ${news} is out. To update, close Customize and run this in a terminal: ${command}`
          : `herdr-customize ${news} is out: ${REPO}/releases/latest`))
        .when(Boolean(command), row => row.child(this.button("copy-update", "Copy", (_e, cx) => { cx.write_to_clipboard(command); this.notice = "Copied the update command."; cx.notify(); })))));
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
    // A pick only fills the inputs; nothing is written until Save.
    if (s.key === "notes" || s.key === "restore_screen") {
      const on = value(this.coreInput) || (s.default === "on" ? "true" : "false");
      detail.child(h_flex().gap(6).children(this.segment(s.key, [["true", "On"], ["false", "Off"]], on, (v, cx) => { this.coreInput.set_value(v); cx.notify(); })));
    } else if (s.key === "shell") {
      // PowerShell is the core's default on Windows, with a hook that follows the
      // shell's folder, so picking it empties the inputs rather than naming it.
      const pickShell = argv => (_e, cx) => { this.coreInput.set_value(argv[0] ?? ""); this.shellArgs.set_value(formatArgs(argv.slice(1))); cx.notify(); };
      const powershell = platform() === "windows" && !this.machine ? [this.chip("shell-default", "PowerShell (default)", !this.coreText(), pickShell([]))] : [];
      const program = value(this.coreInput).toLowerCase();
      // Without the core's -Command hook, cd in the shell never reaches programs it starts.
      const hookLost = platform() === "windows" && !this.machine && /(^|[\\/])(pwsh|powershell)(\.exe)?$/.test(program) && !parseArgs(value(this.shellArgs)).some(a => /^-c(ommand)?$/i.test(a));
      detail.child(h_flex().gap(6).flex_wrap().children([...powershell, ...this.core.shells.map((sh, i) =>
        this.chip(`shell-${i}`, sh.name, this.coreText() === JSON.stringify(sh.argv), pickShell(sh.argv)))]))
        .child(v_flex().gap(5).child(this.label("Program"))
          .child(h_flex().gap(8).child(div().flex_1().min_w(0).child(this.input(this.coreInput)))
            .when(Boolean(herdr.pickFiles) && !this.machine, el => el.child(this.button("shell-choose", "Choose…", (_e, cx) => this.chooseProgram(cx)))))
          .child(hint(s.hint)))
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
      // A failed hc machine ls lists nothing; the picker stays while a machine is
      // picked, so the way back to this computer never disappears.
      .when(this.core.machines.length > 0 || Boolean(this.machine), el => el.child(h_flex().gap(6).flex_wrap().items_center()
        .child(div().text_size(11.5).text_color(t.muted).child("Settings of"))
        .child(this.chip("machine-here", "This computer", !this.machine, (_e, cx) => this.pickMachine(null, cx)))
        .children([...this.core.machines, ...this.machine && !this.core.machines.some(m => m.name === this.machine) ? [{ name: this.machine, state: "not listed" }] : []]
          .map(m => this.chip(`machine-${m.name}`, m.state === "connected" ? m.name : `${m.name} (${m.state.replaceAll("_", " ")})`, this.machine === m.name, (_e, cx) => this.pickMachine(m.name, cx))))))
      // hc machine login asks for a password or code, so it runs in a terminal of
      // its own; machine "" keeps it on this computer whatever space is shown.
      .children(this.core.machines.filter(m => m.login).map(m => h_flex().gap(8).items_center()
        .child(div().flex_1().text_size(11.5).text_color(t.working).child(`${m.name} needs a password or code before Herdr can reach it. Log in opens a space here that asks for it.`))
        .child(this.button(`login-${m.name}`, "Log in", () => this.closeAndRun("space.new", { name: `Log in to ${m.name}`, machine: "", argv: m.login })))))
      .child(this.split(420, [
      v_flex().id("core-settings").flex_1().min_h(0).overflow_y_scrollbar().gap(4).children(rows),
      div().text_size(11).text_color(t.dim).child(this.machine
        ? `Saved to ${this.core.path} through hc. That machine's core reads it each time it needs a setting.`
        : `Saved to ${this.core.path}. The core reads it each time it needs a setting; screen restoration applies at the next recovery.`),
    ], detail
      .when(Boolean(this.core.error), el => el.child(div().text_size(11.5).text_color(t.blocked).child(this.core.error)))
      .when(!this.coreText() && Boolean(s.value), el => el.child(hint(`Save puts ${s.title.toLowerCase()} back to the default (${s.default}).`)))
      .child(h_flex().gap(8).mt(4)
        .child(this.button("core-save", "Save", (_e, cx) => this.saveCore(cx), { primary: true, enabled: this.coreText() !== (s.value ?? "") }))
        .when(Boolean(s.value) && Boolean(this.coreText()), el => el.child(this.button("core-default", `Use the default (${s.default})`, (_e, cx) => {
          this.coreInput.set_value("");
          this.shellArgs.set_value("");
          cx.notify();
        }))))));
  }

  // The picker shows this computer's files only, so a remote machine gets no button.
  chooseProgram(cx) {
    cx.spawn(async cx => {
      try {
        const files = await herdr.pickFiles({ prompt: "Use" });
        if (files?.[0]) this.coreInput.set_value(files[0]);
      } catch (e) { this.error = reason(e); }
      cx.notify();
    });
  }
  focus() { if (this.core && this.coreSel !== "notes" && this.coreSel !== "restore_screen") this.coreInput.focus(); else this.handle.focus(); }
  show(cx) { if (!this.core) this.load(cx, ["core"], (data, cx) => { this.core = data; this.pickCore(this.coreSel, cx); }); }
  render() { return this.corePage(); }
  }
  return page(app, new Page());
}
