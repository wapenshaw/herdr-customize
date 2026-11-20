import * as herdr from "herdr";

export const REPO = "https://github.com/wapenshaw/herdr-customize";
export const FILTERS = [["all", "All", () => true], ["dark", "Dark", r => !r.light], ["light", "Light", r => r.light], ["added", "In Settings", r => r.added]];
export const MODIFIER_KEYS = ["shift", "control", "ctrl", "alt", "platform", "cmd", "super", "fn", "function", "meta"];
export const plural = (n, word) => `${n} ${word}${n === 1 ? "" : "s"}`;
export const reason = e => String(e?.message ?? e).replace(/^Error: /, "");
export const value = input => input.value().trim();
// keys() gives an object binding as its JSON text.
export const commandOf = action => {
  if (action.startsWith("{")) try { return JSON.parse(action).command ?? ""; } catch { return ""; }
  return action.split(":")[0];
};
export const keyRowId = (section, key) => `k:${section}:${key}`;
export const platform = () => herdr.window()?.platform;
// Ctrl, Alt or Cmd held: the key is a shortcut, not Tab or list navigation.
export const chord = m => m.control || m.alt || m.platform;
// The forms the program reviews: owner/repo[/folder], or a GitHub link to either.
export const looksLikeRepo = text => /^(https?:\/\/)?(github\.com\/)?[\w.-]+\/[\w.-]+(\/.*)?$/.test(text.trim());

export function page(app, instance) {
  Object.setPrototypeOf(Object.getPrototypeOf(instance), app);
  for (const name of ["busy", "error", "notice"]) {
    Object.defineProperty(instance, name, { get: () => app[name], set: value => { app[name] = value; } });
  }
  return instance;
}
