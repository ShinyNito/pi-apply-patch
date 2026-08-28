import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { performance } from "node:perf_hooks";

import { applyPatch } from "../dist/index.js";

const cwd = await mkdtemp(join(tmpdir(), "pi-apply-patch-bench-"));
try {
  const lineCount = 1_000_000;
  const contents = Array.from({ length: lineCount }, (_, index) => `line ${index}`).join("\n") + "\n";
  await writeFile(join(cwd, "benchmark.txt"), contents);
  const started = performance.now();
  await applyPatch(
    `*** Begin Patch
*** Update File: benchmark.txt
@@
-line 999998
+changed
*** End Patch`,
    cwd,
  );
  console.log(`Updated an exact match in a ${lineCount.toLocaleString()}-line file in ${(performance.now() - started).toFixed(1)} ms`);
} finally {
  await rm(cwd, { recursive: true, force: true });
}
