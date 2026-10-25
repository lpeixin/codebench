// Generates a placeholder app icon (icon-src.png) with zero dependencies.
// Then run `npx tauri icon icon-src.png` to produce the full icon set in src-tauri/icons.
import { deflateSync } from "node:zlib";
import { writeFileSync } from "node:fs";

const W = 1024;
const H = 1024;

function pixel(x, y) {
  const inOuter = x >= 232 && x < 792 && y >= 232 && y < 792;
  const inInner = x >= 352 && x < 672 && y >= 352 && y < 672;
  const inBar = inOuter && !inInner && x >= 462 && x < 562 && y >= 432 && y < 592;
  if (inBar) return [56, 189, 248]; // accent
  if (inInner) return [15, 23, 42]; // hole
  if (inOuter) return [30, 41, 59]; // frame
  return [15, 23, 42]; // background
}

const stride = 1 + W * 3;
const raw = Buffer.alloc(H * stride);
for (let y = 0; y < H; y++) {
  const row = y * stride;
  raw[row] = 0; // PNG filter: none
  for (let x = 0; x < W; x++) {
    const [r, g, b] = pixel(x, y);
    const o = row + 1 + x * 3;
    raw[o] = r;
    raw[o + 1] = g;
    raw[o + 2] = b;
  }
}

const CRC_TABLE = (() => {
  const t = new Int32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c;
  }
  return t;
})();

function crc32(buf) {
  let c = 0xffffffff;
  for (const byte of buf) c = CRC_TABLE[(c ^ byte) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const out = Buffer.alloc(12 + data.length);
  out.writeUInt32BE(data.length, 0);
  out.write(type, 4, "ascii");
  data.copy(out, 8);
  out.writeUInt32BE(crc32(out.subarray(4, 8 + data.length)), 8 + data.length);
  return out;
}

const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(W, 0);
ihdr.writeUInt32BE(H, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 2; // color type: truecolor RGB
ihdr[10] = 0; // deflate
ihdr[11] = 0; // adaptive filtering
ihdr[12] = 0; // no interlace

const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", ihdr),
  chunk("IDAT", deflateSync(raw, { level: 9 })),
  chunk("IEND", Buffer.alloc(0)),
]);

writeFileSync("icon-src.png", png);
console.log("wrote icon-src.png");
