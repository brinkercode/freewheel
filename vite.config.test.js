import { expect, test } from "vitest";
import exported from "./vite.config.js";

test("the dev server ignores Cargo output and Rust sources", async () => {
  const config =
    typeof exported === "function" ? await exported({ command: "serve", mode: "development" }) : exported;
  for (const glob of ["**/target/**", "**/src-tauri/**", "**/crates/**"]) {
    expect(config.server.watch.ignored).toContain(glob);
  }
});
