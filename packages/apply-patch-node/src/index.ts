import { resolve } from "node:path";

import {
  applyPreparedOperationNative,
  parsePatchNative,
  prepareOperationNative,
  type NativePatchOperation,
  type NativePreparedOperation,
  type NativeUpdateChunk,
} from "../native/index.js";

export type UpdateChunk = NativeUpdateChunk;
export type PatchOperation = NativePatchOperation & {
  kind: "add" | "delete" | "update";
};

export class PatchError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "PatchError";
  }
}

export type MutationQueue = <T>(path: string, callback: () => Promise<T>) => Promise<T>;

const runWithoutQueue: MutationQueue = async (_path, callback) => callback();

export type AppliedChange = {
  operation: "add" | "update" | "delete";
  path: string;
  moveTo?: string;
  diff?: string;
};

export type ApplyPatchProgress = {
  phase: "preparing" | "applying";
  completed: number;
  total: number;
  change?: AppliedChange;
};

export interface ApplyPatchResult {
  changes: AppliedChange[];
}

export interface ApplyPatchOptions {
  queue?: MutationQueue;
  signal?: AbortSignal;
  includeDiff?: boolean;
  progress?: (progress: ApplyPatchProgress) => void;
}

function asPatchError(error: unknown): PatchError {
  return error instanceof PatchError
    ? error
    : new PatchError(error instanceof Error ? error.message : String(error));
}

export function parsePatch(patch: string): PatchOperation[] {
  try {
    return parsePatchNative(patch) as PatchOperation[];
  } catch (error) {
    throw asPatchError(error);
  }
}

function changeForOperation(operation: PatchOperation, diff?: string): AppliedChange {
  const change: AppliedChange = {
    operation: operation.kind,
    path: operation.path,
  };
  if (operation.moveTo !== undefined) {
    change.moveTo = operation.moveTo;
  }
  if (diff !== undefined) {
    change.diff = diff;
  }
  return change;
}

function absolutePathsForOperations(operations: PatchOperation[], cwd: string): string[] {
  return operations.flatMap((operation) => {
    const sourcePath = resolve(cwd, operation.path);
    if (operation.kind === "update" && operation.moveTo) {
      return [sourcePath, resolve(cwd, operation.moveTo)];
    }
    return [sourcePath];
  });
}

async function withMutationQueues<T>(
  paths: string[],
  queue: MutationQueue,
  callback: () => Promise<T>,
): Promise<T> {
  const uniquePaths = [...new Set(paths)].sort();
  const run = (index: number): Promise<T> =>
    index === uniquePaths.length
      ? callback()
      : queue(uniquePaths[index], () => run(index + 1));
  return run(0);
}

export async function applyPatch(
  patch: string,
  cwd: string,
  options: ApplyPatchOptions = {},
): Promise<ApplyPatchResult> {
  const operations = parsePatch(patch);
  if (operations.length === 0) {
    throw new PatchError("No files were modified.");
  }

  const queue = options.queue ?? runWithoutQueue;
  const paths = absolutePathsForOperations(operations, cwd);
  return withMutationQueues(paths, queue, async () => {
    const prepared: NativePreparedOperation[] = [];
    options.progress?.({ phase: "preparing", completed: 0, total: operations.length });

    for (const operation of operations) {
      options.signal?.throwIfAborted();
      let next: NativePreparedOperation;
      try {
        next = (await prepareOperationNative(
          operation,
          cwd,
          options.includeDiff ?? false,
        )) as NativePreparedOperation;
      } catch (error) {
        throw asPatchError(error);
      }
      prepared.push(next);
      options.progress?.({
        phase: "preparing",
        completed: prepared.length,
        total: operations.length,
        change: changeForOperation(operation),
      });
    }

    for (let index = 0; index < prepared.length; index += 1) {
      options.signal?.throwIfAborted();
      try {
        await applyPreparedOperationNative(prepared[index]);
      } catch (error) {
        throw asPatchError(error);
      }
      options.progress?.({
        phase: "applying",
        completed: index + 1,
        total: prepared.length,
        change: changeForOperation(operations[index]),
      });
    }

    return {
      changes: prepared.map((operation, index) =>
        changeForOperation(operations[index], operation.diff ?? undefined),
      ),
    };
  });
}
