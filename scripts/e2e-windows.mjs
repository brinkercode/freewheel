#!/usr/bin/env node
// The Windows click-through (.github/workflows/e2e-windows.yml): the shared
// flow in scripts/e2e.mjs with the Windows platform (WebView2 attached on
// its DevTools port by msedgedriver).
//   node scripts/e2e-windows.mjs --port 9222 --app target/release/freewheel.exe --out e2e-shots
import { fileURLToPath } from "node:url";
import { main } from "./e2e.mjs";

export * from "./e2e.mjs";

if (process.argv[1] === fileURLToPath(import.meta.url)) await main("windows");
