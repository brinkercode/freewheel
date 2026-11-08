// The build tools CI downloads from GitHub releases (the AppImage packaging
// tools, gitleaks, cargo-deny) come from the actions cache, pinned and
// sha256-checked, with retried downloads on a miss: a release download outage
// cannot fail a build.
import { readdirSync, readFileSync } from "node:fs";
import { expect, test } from "vitest";

const read = (p) => readFileSync(new URL(`../${p}`, import.meta.url), "utf8");
const action = read(".github/actions/appimage-tools/action.yml");
const oss = read(".github/workflows/oss-readiness.yml");
const workflows = readdirSync(new URL("../.github/workflows", import.meta.url))
  .filter((f) => /\.ya?ml$/.test(f))
  .map((f) => [f, read(`.github/workflows/${f}`)]);

const RETRIES = "--retry 8 --retry-all-errors --retry-delay 15";

/** The `name sha256 url` lines of the heredoc a download loop reads. */
const manifest = (text) =>
  [...text.matchAll(/^\s*(\S+) (\S+) (https:\/\/github\.com\/\S+)$/gm)].map(([, name, sha, url]) => ({ name, sha, url }));

/** The cache key of the restore step with this id. */
const restoreKey = (text, id) => {
  const m = text.match(new RegExp(`id: ${id}\\r?\\n\\s+uses: actions/cache/restore@[0-9a-f]{40} # v[\\d.]+\\r?\\n\\s+with:\\r?\\n\\s+path: (\\S+)\\r?\\n\\s+key: (.+)`));
  return m ? { path: m[1], key: m[2] } : null;
};

test("the AppImage tools: four pinned files, each with a sha256", () => {
  const files = manifest(action);
  expect(files.map((f) => f.name)).toEqual([
    "AppRun-x86_64",
    "linuxdeploy-07333c6-x86_64.AppImage",
    "linuxdeploy-plugin-appimage.AppImage",
    "runtime-x86_64",
  ]);
  for (const f of files) {
    expect(f.sha).toMatch(/^[0-9a-f]{64}$/);
    // A release tag, never the moving 'continuous' one.
    expect(f.url).toMatch(/\/releases\/download\/[^/]+\//);
    expect(f.url).not.toMatch(/\/continuous\//);
  }
});

test("the AppImage tools' cache key names every tool version", () => {
  const r = restoreKey(action, "restore");
  expect(r.path).toBe("~/.cache/freewheel-appimage-tools");
  for (const tag of ["07333c6", "apprun-old", "1-alpha-20250213-1", "20251108"]) expect(r.key).toContain(tag);
  for (const f of manifest(action)) {
    const tag = f.url.match(/\/releases\/download\/(?:linuxdeploy-)?([^/]+)\//)[1];
    expect(r.key).toContain(tag);
  }
});

test("the AppImage tools are checked, retried, saved on a miss and put where the bundler looks", () => {
  expect(action).toContain(`curl --fail --silent --show-error --location ${RETRIES}`);
  expect(action).toContain('echo "$sha  $store/$name.part" | sha256sum -c -');
  // A restored copy is checked too; one that fails is downloaded again.
  expect(action).toContain('echo "$sha  $store/$name" | sha256sum -c --status -');
  expect(action).toMatch(/if: steps\.restore\.outputs\.cache-hit != 'true'\r?\n\s+uses: actions\/cache\/save@[0-9a-f]{40} # v[\d.]+\r?\n\s+with:\r?\n\s+path: ~\/\.cache\/freewheel-appimage-tools\r?\n\s+key: \$\{\{ steps\.restore\.outputs\.cache-primary-key \}\}/);
  // tauri-bundler's prepare_tools: dirs::cache_dir()/tauri, these names.
  expect(action).toContain('tools="$HOME/.cache/tauri"');
  expect(action).toContain("for name in AppRun-x86_64 linuxdeploy-07333c6-x86_64.AppImage linuxdeploy-plugin-appimage.AppImage; do");
  // appimagetool takes the runtime from here instead of downloading it.
  expect(action).toContain('echo "LDAI_RUNTIME_FILE=$store/runtime-x86_64" >> "$GITHUB_ENV"');
});

test("every workflow that builds an AppImage sets the tools up first", () => {
  const building = workflows.filter(([, t]) => /--bundles appimage/.test(t));
  expect(building.map(([f]) => f).sort()).toEqual(["e2e-linux.yml", "release.yml"]);
  for (const [, t] of building) {
    const setup = t.indexOf("uses: ./.github/actions/appimage-tools");
    expect(setup).toBeGreaterThan(0);
    expect(setup).toBeLessThan(t.indexOf("--bundles appimage"));
  }
});

test("the update e2e's AppImage build downloads nothing: GitHub release downloads go nowhere", () => {
  const e2e = read(".github/workflows/e2e-linux.yml");
  expect(e2e).toMatch(/- name: Build and pack version B, then A\r?\n\s+env:\r?\n\s+TAURI_BUNDLER_TOOLS_GITHUB_MIRROR: http:\/\/127\.0\.0\.1:9\r?\n/);
});

test("oss-readiness: gitleaks and cargo-deny from the cache, keyed on their versions and sha256s", () => {
  const r = restoreKey(oss, "tools-cache");
  expect(r.path).toBe("~/.cache/freewheel-oss-tools");
  for (const v of ["GITLEAKS_VERSION", "GITLEAKS_SHA256", "CARGO_DENY_VERSION", "CARGO_DENY_SHA256"]) {
    expect(r.key).toContain(`\${{ env.${v} }}`);
    expect(oss).toMatch(new RegExp(`^  ${v}: "[^"]+"$`, "m"));
  }
  expect(oss).toMatch(/^  GITLEAKS_SHA256: "[0-9a-f]{64}"$/m);
  expect(oss).toMatch(/^  CARGO_DENY_SHA256: "[0-9a-f]{64}"$/m);
  expect(manifest(oss).map((f) => [f.name, f.sha])).toEqual([
    ["gitleaks.tgz", "${GITLEAKS_SHA256}"],
    ["deny.tgz", "${CARGO_DENY_SHA256}"],
  ]);
  expect(oss).toContain(`curl --fail --silent --show-error --location ${RETRIES}`);
  expect(oss).toContain('echo "$sha  $store/$name.part" | sha256sum -c -');
  expect(oss).toContain('echo "$sha  $store/$name" | sha256sum -c --status -');
  expect(oss).toMatch(/if: steps\.tools-cache\.outputs\.cache-hit != 'true'\r?\n\s+uses: actions\/cache\/save@[0-9a-f]{40}/);
  expect(oss.indexOf("id: tools-cache")).toBeLessThan(oss.indexOf("- name: Tools (pinned versions, checked sha256)"));
});

test("no download in CI goes without retries", () => {
  for (const [f, t] of [...workflows, ["appimage-tools/action.yml", action]]) {
    for (const line of t.split(/\r?\n/).filter((l) => /^\s*(?:curl|wget)\s/.test(l))) {
      expect(`${f}: ${line}`).toContain(RETRIES);
    }
  }
});
