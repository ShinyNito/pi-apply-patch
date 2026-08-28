import { strict as assert } from "node:assert";
import { execFile } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { promisify } from "node:util";

import { applyPatch, parsePatch, PatchError } from "../dist/index.js";

const temporaryDirectory = () => mkdtemp(join(tmpdir(), "pi-apply-patch-"));
const execFileAsync = promisify(execFile);

test("fails when the exact platform package is missing", async () => {
  const loaderUrl = new URL("../native/index.js", import.meta.url).href;
  await assert.rejects(
    execFileAsync(process.execPath, [
      "--input-type=module",
      "--eval",
      `delete process.env.NAPI_RS_NATIVE_LIBRARY_PATH; await import(${JSON.stringify(loaderUrl)})`,
    ]),
    (error) => error.stderr.includes("@shinynito/apply-patch-node-"),
  );
});

test("applies add, update, move, and delete operations", async () => {
  const cwd = await temporaryDirectory();
  try {
    await mkdir(join(cwd, "src"), { recursive: true });
    await mkdir(join(cwd, "move"), { recursive: true });
    await writeFile(join(cwd, "src", "update.txt"), "old\n");
    await writeFile(join(cwd, "src", "delete.txt"), "remove\n");
    await writeFile(join(cwd, "move", "source.txt"), "before\n");

    const result = await applyPatch(
      `*** Begin Patch
*** Add File: nested/new.txt
+one
+
+three
*** Update File: src/update.txt
@@
-old
+new
*** Update File: move/source.txt
*** Move to: moved/target.txt
@@
-before
+after
*** Delete File: src/delete.txt
*** End Patch`,
      cwd,
    );

    assert.deepEqual(result.changes, [
      { operation: "add", path: "nested/new.txt" },
      { operation: "update", path: "src/update.txt" },
      { operation: "update", path: "move/source.txt", moveTo: "moved/target.txt" },
      { operation: "delete", path: "src/delete.txt" },
    ]);
    assert.equal(await readFile(join(cwd, "nested", "new.txt"), "utf8"), "one\n\nthree\n");
    assert.equal(await readFile(join(cwd, "src", "update.txt"), "utf8"), "new\n");
    assert.equal(await readFile(join(cwd, "moved", "target.txt"), "utf8"), "after\n");
    await assert.rejects(readFile(join(cwd, "move", "source.txt")));
    await assert.rejects(readFile(join(cwd, "src", "delete.txt")));
  } finally {
    await rm(cwd, { recursive: true, force: true });
  }
});

test("matches exact context and preserves CRLF", async () => {
  const cwd = await temporaryDirectory();
  try {
    await writeFile(join(cwd, "context.txt"), "before\r\nneedle\r\nold\r\nafter\r\n");
    await applyPatch(
      `*** Begin Patch
*** Update File: context.txt
@@ needle
-old
+new
*** End Patch`,
      cwd,
    );
    assert.equal(
      await readFile(join(cwd, "context.txt"), "utf8"),
      "before\r\nneedle\r\nnew\r\nafter\r\n",
    );
  } finally {
    await rm(cwd, { recursive: true, force: true });
  }
});

test("rejects whitespace and Unicode punctuation mismatches without fallback", async () => {
  const cwd = await temporaryDirectory();
  try {
    await writeFile(join(cwd, "space.txt"), "value  \n");
    await writeFile(join(cwd, "unicode.txt"), "“hello”\n");
    await assert.rejects(
      applyPatch(
        `*** Begin Patch
*** Update File: space.txt
@@
-value
+changed
*** End Patch`,
        cwd,
      ),
      /Failed to find expected lines/,
    );
    await assert.rejects(
      applyPatch(
        `*** Begin Patch
*** Update File: unicode.txt
@@
-"hello"
+"world"
*** End Patch`,
        cwd,
      ),
      /Failed to find expected lines/,
    );
    assert.equal(await readFile(join(cwd, "space.txt"), "utf8"), "value  \n");
    assert.equal(await readFile(join(cwd, "unicode.txt"), "utf8"), "“hello”\n");
  } finally {
    await rm(cwd, { recursive: true, force: true });
  }
});

test("preflights the complete patch before changing files", async () => {
  const cwd = await temporaryDirectory();
  try {
    await writeFile(join(cwd, "first.txt"), "first old\n");
    await writeFile(join(cwd, "second.txt"), "second unchanged\n");
    await assert.rejects(
      applyPatch(
        `*** Begin Patch
*** Update File: first.txt
@@
-first old
+first new
*** Update File: second.txt
@@
-missing
+second new
*** End Patch`,
        cwd,
      ),
      /Failed to find expected lines/,
    );
    assert.equal(await readFile(join(cwd, "first.txt"), "utf8"), "first old\n");
    assert.equal(await readFile(join(cwd, "second.txt"), "utf8"), "second unchanged\n");
  } finally {
    await rm(cwd, { recursive: true, force: true });
  }
});

test("rejects malformed patches", () => {
  assert.throws(
    () => parsePatch("*** Begin Patch\n*** Update File: file.txt\n*** End Patch"),
    PatchError,
  );
});
