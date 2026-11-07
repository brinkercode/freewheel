import { execFileSync } from "node:child_process";
import { expect, test } from "vitest";

// The owner's design handoff (prototype, screens, tokens, icons) is private and must never be in this public repo.
test("no design handoff files are tracked", () => {
  const tracked = execFileSync("git", ["ls-files"], { encoding: "utf8" }).split("\n");
  expect(tracked.filter((f) => /^design\//.test(f) || /freewheel\.dc\.html|screens-and-components\.md/.test(f))).toEqual([]);
});
