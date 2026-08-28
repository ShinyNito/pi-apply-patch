import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const libc = process.platform === "linux"
  ? process.report.getReport().header.glibcVersionRuntime === undefined
    ? "musl"
    : "gnu"
  : undefined;
const target = process.platform === "win32"
  ? `win32-${process.arch}-msvc`
  : process.platform === "linux"
    ? `linux-${process.arch}-${libc}`
    : `${process.platform}-${process.arch}`;
const supportedTargets = new Set([
  "darwin-arm64",
  "darwin-x64",
  "linux-arm64-gnu",
  "linux-arm64-musl",
  "linux-x64-gnu",
  "linux-x64-musl",
  "win32-arm64-msvc",
  "win32-x64-msvc",
]);

if (!supportedTargets.has(target)) {
  throw new Error(`Unsupported platform for @shinynito/apply-patch-node: ${target}`);
}

const binding = require(`./apply-patch.${target}.node`);

export const {
  applyPreparedOperationNative,
  parsePatchNative,
  prepareOperationNative,
} = binding;
