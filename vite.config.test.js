import { expect, test } from "vitest";
import exported from "./vite.config.js";

test("the dev server ignores Cargo output and Rust sources", async () => {
  const config =
    typeof exported === "function" ? await exported({ command: "serve", mode: "development" }) : exported;
  for (const glob of ["**/target/**", "**/src-tauri/**", "**/crates/**"]) {
    expect(config.server.watch.ignored).toContain(glob);
  }
});

test("the dev server and the app window use the same IPv4 address", async () => {
  const fs = await import("node:fs");
  const config =
    typeof exported === "function" ? await exported({ command: "serve", mode: "development" }) : exported;
  const tauri = JSON.parse(fs.readFileSync(new URL("./src-tauri/tauri.conf.json", import.meta.url), "utf8"));
  expect(config.server.host).toBe("127.0.0.1");
  expect(tauri.build.devUrl).toBe(`http://127.0.0.1:${config.server.port}`);
});
