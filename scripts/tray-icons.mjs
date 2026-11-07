#!/usr/bin/env node
// The Windows tray icons in the app's pink, made from the design handoff's
// glyphs (the private design handoff, icons/tray/windows: white for a dark taskbar,
// near-black for a light one) into src-tauri/icons/tray/windows:
//
//   node scripts/tray-icons.mjs          # write them
//   node scripts/tray-icons.mjs --check  # exit 1 if a committed one differs
//
// Every grey pixel of the glyph becomes the pink of its taskbar, keeping its
// alpha, so each state keeps its look: On solid, Off dimmed, Not connected
// dashed; the Attention badge's amber stays. The pinks are the accent's
// (src/styles/tokens.css): the light theme's on a light taskbar, the dark
// theme's brighter one on a dark taskbar. macOS keeps the handoff's template
// images (the menu bar tints those itself).
//
// Plain Node (zlib), no image library: the handoff PNGs are 8-bit RGBA,
// non-interlaced; the output is the same, deflated at level 9 (stable bytes).
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import zlib from "node:zlib";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
// The owner's design handoff is never committed: point FREEWHEEL_HANDOFF at a local copy to regenerate.
export const SOURCE = process.env.FREEWHEEL_HANDOFF
  ? path.join(process.env.FREEWHEEL_HANDOFF, "icons/tray/windows")
  : null;
export const OUT = path.join(root, "src-tauri/icons/tray/windows");
/** The glyph's pink per taskbar. */
export const PINK = { "light-taskbar": [0xc4, 0x46, 0x7f], "dark-taskbar": [0xf0, 0x7f, 0xae] };
export const STATES = ["on", "off", "not-connected", "attention"];
export const SIZES = [16, 24, 32];

const SIGNATURE = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);

/** Decodes an 8-bit RGBA non-interlaced PNG to { width, height, rgba }. */
export function decode(buf) {
  if (!buf.subarray(0, 8).equals(SIGNATURE)) throw new Error("not a PNG");
  let at = 8;
  let header = null;
  const idat = [];
  while (at < buf.length) {
    const len = buf.readUInt32BE(at);
    const type = buf.toString("latin1", at + 4, at + 8);
    const data = buf.subarray(at + 8, at + 8 + len);
    if (type === "IHDR") header = { width: data.readUInt32BE(0), height: data.readUInt32BE(4), depth: data[8], color: data[9], interlace: data[12] };
    else if (type === "IDAT") idat.push(data);
    else if (type === "IEND") break;
    at += 12 + len;
  }
  if (!header || header.depth !== 8 || header.color !== 6 || header.interlace !== 0) throw new Error(`unsupported PNG ${JSON.stringify(header)}`);
  const { width, height } = header;
  const raw = zlib.inflateSync(Buffer.concat(idat));
  const stride = width * 4;
  const rgba = Buffer.alloc(stride * height);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    const line = raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1));
    for (let x = 0; x < stride; x++) {
      const a = x >= 4 ? rgba[y * stride + x - 4] : 0;
      const b = y > 0 ? rgba[(y - 1) * stride + x] : 0;
      const c = x >= 4 && y > 0 ? rgba[(y - 1) * stride + x - 4] : 0;
      let v = line[x];
      if (filter === 1) v += a;
      else if (filter === 2) v += b;
      else if (filter === 3) v += (a + b) >> 1;
      else if (filter === 4) {
        const p = a + b - c;
        const pa = Math.abs(p - a);
        const pb = Math.abs(p - b);
        const pc = Math.abs(p - c);
        v += pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      } else if (filter !== 0) throw new Error(`bad filter ${filter}`);
      rgba[y * stride + x] = v & 0xff;
    }
  }
  return { width, height, rgba };
}

const CRC_TABLE = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
const crc32 = (buf) => {
  let c = 0xffffffff;
  for (const byte of buf) c = CRC_TABLE[(c ^ byte) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};
const chunk = (type, data) => {
  const out = Buffer.alloc(12 + data.length);
  out.writeUInt32BE(data.length, 0);
  out.write(type, 4, "latin1");
  data.copy(out, 8);
  out.writeUInt32BE(crc32(out.subarray(4, 8 + data.length)), 8 + data.length);
  return out;
};

/** Encodes RGBA pixels as an 8-bit RGBA PNG (filter 0, zlib level 9). */
export function encode({ width, height, rgba }) {
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr.set([8, 6, 0, 0, 0], 8);
  const stride = width * 4;
  const raw = Buffer.alloc((stride + 1) * height);
  for (let y = 0; y < height; y++) rgba.copy(raw, y * (stride + 1) + 1, y * stride, (y + 1) * stride);
  return Buffer.concat([SIGNATURE, chunk("IHDR", ihdr), chunk("IDAT", zlib.deflateSync(raw, { level: 9 })), chunk("IEND", Buffer.alloc(0))]);
}

/** The glyph's grey pixels in `pink`, alpha kept; coloured ones (the amber badge) as they are. */
export function tint({ width, height, rgba }, pink) {
  const out = Buffer.from(rgba);
  for (let i = 0; i < out.length; i += 4) {
    if (out[i + 3] === 0) {
      out.fill(0, i, i + 3);
      continue;
    }
    const [r, g, b] = out.subarray(i, i + 3);
    if (Math.max(r, g, b) - Math.min(r, g, b) < 40) out.set(pink, i);
  }
  return { width, height, rgba: out };
}

/** Every pink icon: [relative path, PNG bytes]. */
export function pinkIcons() {
  const all = [];
  for (const [taskbar, pink] of Object.entries(PINK))
    for (const state of STATES)
      for (const size of SIZES) {
        const name = `${taskbar}/tray-${state}-${size}.png`;
        if (!SOURCE) throw new Error("set FREEWHEEL_HANDOFF to a local copy of the design handoff");
        all.push([name, encode(tint(decode(fs.readFileSync(path.join(SOURCE, name))), pink))]);
      }
  return all;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const check = process.argv.includes("--check");
  let stale = 0;
  for (const [name, png] of pinkIcons()) {
    const file = path.join(OUT, name);
    if (check) {
      if (!fs.existsSync(file) || !fs.readFileSync(file).equals(png)) {
        console.error(`stale: ${name} (run node scripts/tray-icons.mjs)`);
        stale++;
      }
    } else {
      fs.mkdirSync(path.dirname(file), { recursive: true });
      fs.writeFileSync(file, png);
    }
  }
  if (stale) process.exit(1);
  console.log(check ? "tray icons are current" : `wrote ${SIZES.length * STATES.length * 2} tray icons to ${path.relative(root, OUT)}`);
}
