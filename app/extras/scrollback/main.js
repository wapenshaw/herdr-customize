import { View, div } from "gpui-kit";
import * as herdr from "herdr";

async function editorFor(config, machine) {
  if (config.editor !== undefined) {
    if (!Array.isArray(config.editor) || config.editor.length === 0 ||
        config.editor.some(arg => typeof arg !== "string" || !arg)) {
      throw new Error("config.json editor must be a nonempty array of nonempty strings");
    }
    return config.editor;
  }

  const options = machine ? { on: machine } : undefined;
  try {
    const result = await herdr.exec("sh", ["-c", "printf '%s' \"${VISUAL:-${EDITOR:-}}\""], options);
    if (result.code !== 0) throw new Error(result.stderr || "Could not read the editor environment");
    const editor = result.stdout.trim();
    return editor ? editor.split(/\s+/) : ["vi"];
  } catch (error) {
    if (!machine) throw error;
    const result = await herdr.exec("cmd", ["/c", "ver"], options);
    if (result.code !== 0) throw error;
    return ["notepad"];
  }
}

async function openScrollback(focus) {
  const terminal = focus?.terminal;
  if (!terminal) throw new Error("Focus a terminal before opening scrollback");

  const command = herdr.commands().find(item => item.name === "terminal.open");
  if (!command || !["argv", "place"].every(name => command.args.some(arg => arg.name === name))) {
    throw new Error("This version of Herdr cannot open an editor tab");
  }

  const folder = `${herdr.appHome()}/data/scrollback`;
  const raw = await herdr.readTextFile(`${folder}/config.json`);
  const config = raw === null ? {} : JSON.parse(raw);
  if (!config || typeof config !== "object" || Array.isArray(config)) throw new Error("config.json must be an object");
  const lines = config.lines === undefined ? 100000 : config.lines;
  if (!Number.isSafeInteger(lines) || lines < 0) throw new Error("config.json lines must be a nonnegative integer");

  const result = await herdr.hc(["read", terminal, "--lines", String(lines)]);
  if (result.code !== 0) throw new Error(result.stderr.trim() || `hc read exited with ${result.code}`);
  const file = `${folder}/${encodeURIComponent(terminal)}.txt`;
  const previous = await herdr.readTextFile(file);
  await herdr.writeTextFile(file, result.stdout, previous);

  const space = herdr.spaces().find(item => item.id === focus.space);
  if (!space) throw new Error("The original space is no longer available");
  const machine = space.machine;
  if (!machine && !config.editor && herdr.window()?.platform === "windows") {
    // The app registered for .txt, as Explorer would; no terminal tab for a GUI editor.
    await herdr.exec("rundll32.exe", ["url.dll,FileProtocolHandler", file.replaceAll("/", "\\")]);
    return;
  }
  const editor = await editorFor(config, machine);
  let editorFile = file;
  if (machine) {
    const terminalMachine = terminal.includes("/") ? terminal.slice(0, terminal.indexOf("/")) : null;
    if (machine !== terminalMachine) {
      throw new Error("The terminal and the current space are on different machines");
    }
    const copied = await herdr.hc(["put", terminal, file, "--json"]);
    if (copied.code !== 0) throw new Error(copied.stderr.trim() || `hc put exited with ${copied.code}`);
    editorFile = JSON.parse(copied.stdout).path;
    if (typeof editorFile !== "string" || !editorFile) throw new Error("hc put did not return a file path");
  }
  if (herdr.focused()?.space !== focus.space) throw new Error("The focused space changed; try again in the original space");
  const args = { argv: [...editor, editorFile], place: "tab" };
  if (command.args.some(arg => arg.name === "name")) args.name = "scrollback";
  herdr.run("terminal.open", args);
}

export default class Scrollback extends View {
  init(_props, cx) {
    if (typeof herdr.checking === "function" ? herdr.checking() : !herdr.window()) return;
    const overlay = herdr.props()?.overlay;
    const focus = herdr.focused();
    cx.spawn(async () => {
      try {
        await openScrollback(focus);
      } catch (error) {
        herdr.notify({ title: "Scrollback", body: String(error?.message ?? error), level: "error" });
      } finally {
        if (overlay) herdr.closeOverlay(overlay);
      }
    });
  }

  render() {
    return div().p(12).child("Opening scrollback…");
  }
}
