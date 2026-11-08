import { describe, expect, test } from "vitest";
import { parseNotes, withoutComments } from "./notesMarkdown.js";

describe("withoutComments", () => {
  test("cuts comments and keeps the text around them", () => {
    expect(withoutComments("a<!-- x -->b")).toBe("ab");
    expect(withoutComments("a<!-- never closed")).toBe("a");
    expect(withoutComments("no comments")).toBe("no comments");
  });

  test("no '<!--' survives, even one that cutting would join back together", () => {
    for (const t of ["<!<!---->--", "<!--<!-- -->", "<<!---->!--", "x<!-->y-->z", "<!---->", "<!<!<!------>-->--"])
      expect(withoutComments(t)).not.toContain("<!--");
  });

  test("release notes with a release-please comment parse without it", () => {
    const blocks = parseNotes("<!-- release-please -->\n### Fixed\n\n* a thing <!<!---->-- x -->works");
    expect(JSON.stringify(blocks)).not.toContain("<!--");
  });
});
