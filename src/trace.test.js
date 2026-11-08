import { expect, test } from "vitest";
import { TRACE_MAX, trace } from "./trace.js";

test("the trace keeps the newest entries, timestamped", () => {
  const target = {};
  trace("update", { status: "ready" }, target);
  expect(target.__freewheelTrace).toEqual([{ at: expect.any(String), what: "update", status: "ready" }]);
  for (let i = 0; i < TRACE_MAX + 5; i++) trace("n", { i }, target);
  expect(target.__freewheelTrace).toHaveLength(TRACE_MAX);
  expect(target.__freewheelTrace.at(-1).i).toBe(TRACE_MAX + 4);
  trace("nothing", {}, undefined);
});
