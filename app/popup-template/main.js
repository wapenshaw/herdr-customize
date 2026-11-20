import { View, div } from "gpui-kit";
import { h_flex } from "gpui-base";
import * as herdr from "herdr";

const name = __POPUP_NAME__;
const label = `herdr.popup=${name}`;

async function hc(args) {
  const result = await herdr.hc(args);
  if (result.code !== 0) throw new Error(result.stderr.trim() || `hc exited with ${result.code}`);
  return result.stdout;
}

async function settings() {
  const raw = await herdr.readTextFile(`${herdr.appHome()}/mods/popup-${name}/popup.json`);
  if (raw === null) throw new Error("popup.json is missing");
  const config = JSON.parse(raw);
  if (!config || typeof config !== "object" || Array.isArray(config) || config.name !== name ||
      !Array.isArray(config.argv) || config.argv.length === 0 ||
      config.argv.some(arg => typeof arg !== "string") || !config.argv[0] ||
      !Number.isFinite(config.width) || config.width <= 0 ||
      !Number.isFinite(config.height) || config.height <= 0 ||
      typeof config.persistent !== "boolean") {
    throw new Error("popup.json has invalid settings");
  }
  return config;
}

function notify(error) {
  herdr.notify({ title: `Popup: ${name}`, body: String(error?.message ?? error), level: "error" });
}

async function launch(focus) {
  const config = await settings();
  let address;
  if (config.persistent) {
    address = herdr.terminals().find(terminal => terminal.labels?.["herdr.popup"] === name && terminal.state === "live")?.address;
    if (!address) {
      const list = JSON.parse(await hc(["ls", "--json", "--label", label]));
      address = list.terminals.find(terminal => terminal.state === "live")?.address;
    }
  }
  if (!address) {
    const args = ["run", "--json", "--bg", "--no-parent", "--label", label, "--cwd", focus?.cwd || herdr.home()];
    args.push("--", ...config.argv);
    const result = JSON.parse(await hc(args));
    address = result.address;
    if (typeof address !== "string" || !address) throw new Error("hc run did not return a terminal address");
  }
  herdr.openOverlay({
    kind: "window", key: `popup-${name}`, width: config.width, height: config.height,
    layout: { type: "column", children: [
      { type: "block", size: { px: 28 }, content: { mod: `popup-${name}`, props: { role: "bar", address, persistent: config.persistent } } },
      { type: "block", content: { terminal: address } },
    ] },
  });
}

export default class Popup extends View {
  init(_props, cx) {
    const given = herdr.props();
    this.bar = given?.role === "bar";
    // The bar owns a non-persistent terminal: close() ends it whenever its window goes.
    this.owned = this.bar && !given.persistent ? given.address : null;
    if (typeof herdr.checking === "function" ? herdr.checking() : !herdr.window()) return;
    if (this.bar) return;
    const overlay = herdr.props()?.overlay;
    const focus = herdr.focused();
    const space = herdr.spaces().find(item => item.id === focus?.space);
    cx.spawn(async () => {
      try {
        if (space?.machine) throw new Error("Popups can only start in a local space");
        await launch(focus);
      } catch (error) {
        notify(error);
      } finally {
        if (overlay) herdr.closeOverlay(overlay);
      }
    });
  }

  async close() {
    if (this.owned && herdr.terminals().some(t => t.address === this.owned && t.state === "live")) herdr.run("terminal.kill", { terminal: this.owned });
  }

  dismiss() {
    const overlay = herdr.props()?.overlay;
    if (overlay) herdr.closeOverlay(overlay);
  }

  render() {
    if (!this.bar) return div().p(12).child("Opening popup…");
    const t = herdr.theme();
    return h_flex().size_full().px(10).gap(10).items_center().bg(t.chrome).text_color(t.foreground)
      .child(div().flex_1().text_size(12).font_semibold().child(name))
      .child(div().id("close").px(9).py(4).rounded(5).text_size(12).text_color(t.muted)
        .cursor_pointer().hover(s => s.bg(t.hover).text_color(t.foreground))
        .on_mouse_down("left", (_event, cx) => cx.stop_propagation())
        .on_click(() => this.dismiss()).child("Close"));
  }
}
