import { fileURLToPath } from "node:url";

const libc = process.platform === "linux"
  ? process.report.getReport().header.glibcVersionRuntime === undefined
    ? "musl"
    : "gnu"
  : undefined;
const platform = process.platform === "win32"
  ? `win32-${process.arch}-msvc`
  : process.platform === "linux"
    ? `linux-${process.arch}-${libc}`
    : `${process.platform}-${process.arch}`;

process.env.NAPI_RS_NATIVE_LIBRARY_PATH = fileURLToPath(
  new URL(`../native/apply-patch.${platform}.node`, import.meta.url),
);
