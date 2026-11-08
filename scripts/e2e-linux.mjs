#!/usr/bin/env node
// The Linux click-through (.github/workflows/e2e-linux.yml): the shared flow
// in scripts/e2e.mjs with the Linux platform (tauri-driver in front of
// WebKitGTK's WebKitWebDriver, under Xvfb).
//   node scripts/e2e-linux.mjs --app target/release/freewheel --out e2e-shots --tauri-driver ~/.cargo/bin/tauri-driver --native-driver /usr/bin/WebKitWebDriver
import { fileURLToPath } from "node:url";
import { main } from "./e2e.mjs";

if (process.argv[1] === fileURLToPath(import.meta.url)) await main("linux");
