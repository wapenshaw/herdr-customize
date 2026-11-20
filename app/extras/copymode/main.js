import { View, div } from "gpui-kit";
import { h_flex } from "gpui-base";
import * as herdr from "herdr";

const required = ["terminal.select", "terminal.adjust_selection", "terminal.clear_selection", "terminal.copy"];

export default class CopyMode extends View {
  init(_props, cx) {
    this.handle = cx.focus_handle();
    this.mode = "navigate";
    this.terminal = herdr.props()?.terminal ?? null;
    this.error = "";
    this.queue = [];
    this.waiting = false;
    this.hasSelection = false;
    this.line = null;
    if (herdr.checking?.() || !herdr.window()) return;

    if (!herdr.props()?.copyModeWindow) {
      const terminal = herdr.focused()?.terminal ?? null;
      herdr.closeOverlay(herdr.props().overlay);
      herdr.openOverlay({ mod: "copymode", kind: "window", frame: "none", width: 580, height: 36,
        props: { copyModeWindow: true, terminal } });
      return;
    }

    const commands = herdr.commands();
    const names = new Set(commands.map(command => command.name));
    this.copyTakesTerminal = commands.find(command => command.name === "terminal.copy")?.args.some(arg => arg.name === "terminal") ?? false;
    if (!required.every(name => names.has(name))) this.error = "Needs Herdr 0.1.16 or newer. Press any key to close.";
    else if (!this.terminal) this.error = "Focus a terminal first. Press any key to close.";
    cx.spawn(async cx => {
      for (;;) {
        const command = await herdr.nextCommand();
        if (command.startsWith("key:")) {
          const name = command.slice(4);
          const shift = name === "V" || name === "G" || name === "$";
          this.handleKey(name, { shift, control: false, alt: false, platform: false }, cx);
        } else if (command === "focus") this.handle.focus();
      }
    });
    this.handle.focus();
  }

  selection() {
    return herdr.terminals().find(terminal => terminal.address === this.terminal)?.selection ?? null;
  }

  command(name, args = {}) {
    herdr.run(name, { ...args, terminal: this.terminal });
  }

  dismiss(clear) {
    if (clear && this.terminal) this.command("terminal.clear_selection");
    herdr.closeOverlay(herdr.props().overlay);
  }

  // Terminal commands are queued, so snapshots immediately after a command are stale.
  // Serialize keys until the selection is published; line ranges then use local rows
  // and an ordered sequence of commands without reading back between them.
  waitFor(cx, predicate, done, remaining = 80) {
    this.waiting = true;
    cx.timer.after(30, cx => {
      const selection = this.selection();
      if (predicate(selection)) {
        this.waiting = false;
        done(selection, cx);
        this.drain(cx);
      } else if (remaining <= 0) {
        this.waiting = false;
        this.queue = [];
        this.error = "Selection did not update. Press any key to close.";
        cx.notify();
      } else this.waitFor(cx, predicate, done, remaining - 1);
    });
  }

  handleKey(key, modifiers, cx) {
    this.queue.push({ key, modifiers });
    this.drain(cx);
  }

  drain(cx) {
    while (!this.waiting && this.queue.length) {
      const { key, modifiers } = this.queue.shift();
      this.applyKey(key, modifiers, cx);
    }
    cx.notify();
  }

  applyKey(input, modifiers, cx) {
    if (this.error) { this.dismiss(false); this.queue = []; return; }
    const key = (input ?? "").toLowerCase();
    if (key === "escape" || (!modifiers.control && !modifiers.alt && !modifiers.platform && key === "q")) {
      this.dismiss(true);
      this.queue = [];
      return;
    }
    if (modifiers.alt || modifiers.platform) return;
    if (modifiers.control) {
      if (key === "u" || key === "d") this.command("terminal.scroll", { pages: key === "u" ? -0.5 : 0.5 });
      return;
    }
    if (key === "v") {
      const previous = this.selection();
      const at = this.hasSelection ? previous?.end ?? "cursor" : "cursor";
      this.mode = modifiers.shift ? "line" : "cell";
      const select = cx => {
        this.command("terminal.select", { at, mode: this.mode });
        this.waitFor(cx, selection => selection !== null, selection => {
          this.hasSelection = !!selection;
          if (selection && this.mode === "line") {
            const row = selection.start.row;
            this.line = { row, min: row, max: row };
          }
        });
      };
      if (previous) {
        this.command("terminal.clear_selection");
        this.waitFor(cx, selection => selection === null, (_selection, nextCx) => select(nextCx));
      } else select(cx);
    } else if (key === "y") {
      if (this.copyTakesTerminal) this.command("terminal.copy");
      else {
        herdr.run("pane.focus", { terminal: this.terminal });
        herdr.run("terminal.copy");
      }
      this.dismiss(false);
      this.queue = [];
    } else if (key === "g") {
      this.command("terminal.scroll", { to: modifiers.shift ? "bottom" : "top" });
    } else {
      const direction = { h: "left", left: "left", j: "down", down: "down", k: "up", up: "up",
        l: "right", right: "right", "0": "line_start", "$": "line_end" }[key]
        ?? (key === "4" && modifiers.shift ? "line_end" : null);
      if (direction) this.move(direction, cx);
    }
  }

  move(direction, cx) {
    if (this.mode === "line" && this.line) {
      if (direction !== "up" && direction !== "down") return;
      this.line.row = Math.max(0, this.line.row + (direction === "up" ? -1 : 1));
      this.line.min = Math.min(this.line.min, this.line.row);
      this.line.max = Math.max(this.line.max, this.line.row);
      this.command("terminal.select", { at: { row: this.line.min, col: 0 }, mode: "cell" });
      for (let row = this.line.min; row < this.line.max; row++) {
        this.command("terminal.adjust_selection", { direction: "down" });
      }
      this.command("terminal.adjust_selection", { direction: "line_end" });
      return;
    }
    const before = JSON.stringify(this.selection());
    if (!this.hasSelection) {
      this.command("terminal.select", { at: "cursor", mode: "cell" });
      this.hasSelection = true;
    }
    this.command("terminal.adjust_selection", { direction });
    this.waitFor(cx, selection => JSON.stringify(selection) !== before, () => {});
  }

  render() {
    const t = herdr.theme();
    return h_flex().size_full().px(12).gap(12).items_center().bg(t.popover).text_color(t.foreground)
      .track_focus(this.handle).on_key_down((e, cx) => { cx.stop_propagation(); this.handleKey(e.key, e.modifiers, cx); })
      .child(div().text_size(12).font_semibold().text_color(t.accent).child("COPY MODE"))
      .child(div().text_size(12).whitespace_nowrap().text_color(t.muted)
        .child(this.error || `${this.mode}  |  h j k l move  v/V select  y copy  esc quit`));
  }
}
