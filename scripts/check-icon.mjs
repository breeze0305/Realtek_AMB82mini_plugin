import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const iconPath = process.argv[2] ?? fileURLToPath(new URL("../src-tauri/icons/icon.ico", import.meta.url));
const requiredSizes = [16, 24, 32, 48, 256];
const pngSignature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);

function check(condition, message) {
  if (!condition) throw new Error(message);
}

function validateIcon(bytes) {
  check(bytes.length >= 6, "ICO header is truncated.");
  check(bytes.readUInt16LE(0) === 0 && bytes.readUInt16LE(2) === 1, "File is not a Windows ICO.");
  const count = bytes.readUInt16LE(4);
  check(count > 0, "ICO must contain icon images.");
  const directoryEnd = 6 + count * 16;
  check(directoryEnd <= bytes.length, "ICO directory entries are truncated.");
  const sizes = new Set();

  for (let index = 0; index < count; index += 1) {
    const entry = 6 + index * 16;
    const width = bytes[entry] || 256;
    const height = bytes[entry + 1] || 256;
    const label = `ICO frame ${index + 1} (${width}x${height})`;
    check(width === height && width >= 16 && width <= 256, `${label} must be square and between 16 and 256 pixels.`);
    check(!sizes.has(width), `${label} duplicates an existing size.`);
    check(bytes[entry + 3] === 0, `${label} has an invalid reserved field.`);
    check(bytes.readUInt16LE(entry + 4) <= 1, `${label} has an invalid color plane count.`);
    check(bytes.readUInt16LE(entry + 6) === 32, `${label} must use 32-bit color.`);
    const length = bytes.readUInt32LE(entry + 8);
    const offset = bytes.readUInt32LE(entry + 12);
    check(
      offset >= directoryEnd && length > 0 && offset + length <= bytes.length,
      `${label} image data is truncated or out of bounds.`,
    );
    const data = bytes.subarray(offset, offset + length);

    if (data.subarray(0, 8).equals(pngSignature)) {
      check(data.length >= 33, `${label} PNG header is truncated.`);
      check(
        data.readUInt32BE(8) === 13 && data.toString("ascii", 12, 16) === "IHDR",
        `${label} has an invalid PNG header.`,
      );
      const actualWidth = data.readUInt32BE(16);
      const actualHeight = data.readUInt32BE(20);
      check(
        actualWidth === width && actualHeight === height,
        `${label} directory dimensions differ from its PNG dimensions (${actualWidth}x${actualHeight}).`,
      );
      check(data[24] === 8 && data[25] === 6, `${label} PNG must use 8-bit RGBA channels.`);
    } else {
      check(data.length >= 40, `${label} bitmap header is truncated.`);
      const headerSize = data.readUInt32LE(0);
      check(headerSize >= 40 && headerSize <= data.length, `${label} has an invalid bitmap header size.`);
      const actualWidth = data.readInt32LE(4);
      const combinedHeight = data.readInt32LE(8);
      check(
        actualWidth === width && combinedHeight === height * 2,
        `${label} directory dimensions differ from its bitmap dimensions (${actualWidth}x${combinedHeight / 2}).`,
      );
      check(
        data.readUInt16LE(12) === 1 && data.readUInt16LE(14) === 32,
        `${label} bitmap must use one plane and 32-bit color.`,
      );
      check(data.readUInt32LE(16) === 0, `${label} bitmap must be uncompressed.`);
      const colorStride = width * 4;
      const maskStride = Math.ceil(width / 32) * 4;
      check(
        data.length >= headerSize + (colorStride + maskStride) * height,
        `${label} bitmap pixels or transparency mask are truncated.`,
      );
    }
    sizes.add(width);
  }

  check(sizes.size > 1, "ICO must contain multiple icon sizes.");
  const missing = requiredSizes.filter((size) => !sizes.has(size));
  check(missing.length === 0, `ICO is missing required sizes: ${missing.join(", ")}.`);
  return [...sizes].sort((left, right) => left - right);
}

try {
  const sizes = validateIcon(readFileSync(iconPath));
  console.log(`Windows icon is valid: ${sizes.map((size) => `${size}x${size}`).join(", ")}.`);
} catch (error) {
  console.error(`Windows icon check failed: ${error instanceof Error ? error.message : String(error)}`);
  process.exitCode = 1;
}
