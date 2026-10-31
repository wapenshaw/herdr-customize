// Opens herdr-customize in a floating terminal. Written by
// `herdr-customize app install`, which fills in BIN; reinstalling replaces it.
import { View, div } from "gpui-kit";
import { hc, openOverlay, closeOverlay, props, notify, appHome } from "herdr";

const BIN = "@BIN@";

export default class Customize extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      const own = props().overlay;
      try {
        const r = await hc(["run", "--bg", "--json", "--rm", "--no-parent", "--name", "herdr-customize", "--", BIN, "app", "--home", appHome(), "ui"]);
        if (r.code !== 0) throw new Error(r.stderr || r.stdout);
        const t = JSON.parse(r.stdout);
        openOverlay({ kind: "dialog", width: 0.9, height: 0.85, layout: { type: "column", children: [{ type: "block", content: { terminal: t.address } }] } });
      } catch (e) {
        notify({ title: "herdr-customize", body: String(e), level: "error" });
      }
      if (own) closeOverlay(own);
    });
  }

  render() {
    return div();
  }
}
