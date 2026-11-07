import fs from "node:fs";
import path from "node:path";
import { expect, test } from "vitest";
import { OUT, PINK, SIZES, SOURCE, STATES, decode, encode, pinkIcons, tint } from "./tray-icons.mjs";

test("the committed Windows tray icons are the pink ones scripts/tray-icons.mjs makes from the handoff glyphs", () => {
  const icons = pinkIcons();
  expect(icons).toHaveLength(2 * STATES.length * SIZES.length);
  for (const [name, png] of icons) expect(fs.readFileSync(path.join(OUT, name)).equals(png), `${name}: run node scripts/tray-icons.mjs`).toBe(true);
  // No stale ICOs of the old white / black glyphs.
  for (const bar of Object.keys(PINK)) expect(fs.readdirSync(path.join(OUT, bar)).filter((f) => !f.endsWith(".png"))).toEqual([]);
});

test("every grey glyph pixel turns the taskbar's pink with its alpha; the amber badge stays", () => {
  for (const [bar, pink] of Object.entries(PINK))
    for (const state of STATES) {
      const before = decode(fs.readFileSync(path.join(SOURCE, `${bar}/tray-${state}-32.png`)));
      const after = decode(fs.readFileSync(path.join(OUT, `${bar}/tray-${state}-32.png`)));
      expect(after.width).toBe(32);
      let amber = 0;
      for (let i = 0; i < after.rgba.length; i += 4) {
        expect(after.rgba[i + 3]).toBe(before.rgba[i + 3]);
        if (after.rgba[i + 3] === 0) continue;
        const px = [...after.rgba.subarray(i, i + 3)];
        if (px.join() === "240,160,32") amber++;
        else if (px.join() !== pink.join()) expect(Math.max(...px) - Math.min(...px), `${bar} ${state} ${px}`).toBeGreaterThanOrEqual(40);
      }
      expect(amber > 0, `${bar} ${state}`).toBe(state === "attention");
    }
});

test("the PNG codec round-trips", () => {
  const img = { width: 2, height: 2, rgba: Buffer.from([1, 2, 3, 4, 255, 255, 255, 0, 9, 8, 7, 6, 200, 100, 50, 255]) };
  expect(decode(encode(img))).toEqual(img);
  expect([...tint(img, [7, 7, 7]).rgba]).toEqual([7, 7, 7, 4, 0, 0, 0, 0, 7, 7, 7, 6, 200, 100, 50, 255]);
});
