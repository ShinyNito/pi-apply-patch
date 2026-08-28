import type { ExtensionAPI, Theme } from "@earendil-works/pi-coding-agent";
import {
  DEFAULT_MAX_BYTES,
  DEFAULT_MAX_LINES,
  formatSize,
  keyHint,
  truncateHead,
  withFileMutationQueue,
} from "@earendil-works/pi-coding-agent";
import { Text } from "@earendil-works/pi-tui";
import { Type } from "typebox";

import type {
  AppliedChange,
  ApplyPatchProgress,
  ApplyPatchResult,
} from "@shinynito/apply-patch-node";
import { applyPatch, parsePatch } from "@shinynito/apply-patch-node";

const parameters = Type.Object({
  patch: Type.String({
    description:
      "A complete file-oriented patch wrapped in *** Begin Patch and *** End Patch.",
  }),
});

function formatChange(change: AppliedChange): string {
  const path = change.moveTo === undefined ? change.path : `${change.path} -> ${change.moveTo}`;
  const prefix = change.operation === "add" ? "A" : change.operation === "delete" ? "D" : "M";
  return `${prefix} ${path}`;
}

type OperationCounts = { add: number; update: number; delete: number };

function countChanges(changes: AppliedChange[]): OperationCounts {
  return changes.reduce<OperationCounts>(
    (counts, change) => {
      counts[change.operation] += 1;
      return counts;
    },
    { add: 0, update: 0, delete: 0 },
  );
}

function countPatchOperations(patch: string): OperationCounts {
  return parsePatch(patch).reduce<OperationCounts>(
    (counts, operation) => {
      counts[operation.kind] += 1;
      return counts;
    },
    { add: 0, update: 0, delete: 0 },
  );
}

function operationBadge(operation: "add" | "update" | "delete", theme: Theme): string {
  const label = operation === "add" ? "A" : operation === "update" ? "M" : "D";
  const color = operation === "add" ? "success" : operation === "update" ? "accent" : "warning";
  return theme.fg(color, theme.bold(label));
}

function formatCounts(counts: OperationCounts, theme: Theme): string {
  const parts: string[] = [];
  if (counts.add > 0) parts.push(`${operationBadge("add", theme)} ${counts.add}`);
  if (counts.update > 0) parts.push(`${operationBadge("update", theme)} ${counts.update}`);
  if (counts.delete > 0) parts.push(`${operationBadge("delete", theme)} ${counts.delete}`);
  return parts.join(theme.fg("dim", " · "));
}

function formatPlainCounts(counts: OperationCounts): string {
  const parts: string[] = [];
  if (counts.add > 0) parts.push(`A ${counts.add}`);
  if (counts.update > 0) parts.push(`M ${counts.update}`);
  if (counts.delete > 0) parts.push(`D ${counts.delete}`);
  return parts.join(", ");
}

function formatUiChange(change: AppliedChange, theme: Theme): string {
  const path = change.moveTo === undefined ? change.path : `${change.path} → ${change.moveTo}`;
  return `${operationBadge(change.operation, theme)} ${theme.fg("toolOutput", path)}`;
}

function formatUiDiffLine(line: string, theme: Theme): string {
  const color = line.startsWith("+")
    ? "toolDiffAdded"
    : line.startsWith("-")
      ? "toolDiffRemoved"
      : "toolDiffContext";
  return theme.fg(color, line);
}

function renderText(text: string, lastComponent: unknown): Text {
  const component = lastComponent instanceof Text ? lastComponent : new Text("", 0, 0);
  component.setText(text);
  return component;
}

function formatPlainResult(changes: AppliedChange[]): string {
  const plainText = `Applied patch to ${changes.length} ${changes.length === 1 ? "file" : "files"} (${formatPlainCounts(
    countChanges(changes),
  )}):\n${changes.flatMap((change) => [formatChange(change), ...(change.diff?.split("\n") ?? [])]).join("\n")}`;
  const truncated = truncateHead(plainText, {
    maxBytes: DEFAULT_MAX_BYTES,
    maxLines: DEFAULT_MAX_LINES,
  });
  if (!truncated.truncated) return truncated.content;
  return `${truncated.content}\n\n[Diff truncated: ${truncated.outputLines} of ${truncated.totalLines} lines, ${formatSize(
    truncated.outputBytes,
  )} of ${formatSize(truncated.totalBytes)}. Expand the tool row in the TUI for the full diff.]`;
}

type ApplyPatchToolDetails = ApplyPatchResult | ApplyPatchProgress;

export default function applyPatchExtension(pi: ExtensionAPI): void {
  pi.registerTool<typeof parameters, ApplyPatchToolDetails>({
    name: "apply_patch",
    label: "apply_patch",
    description:
      "Edit files with a complete Codex-style, file-oriented patch. The patch is fully validated before any write, then its file operations are applied in order.",
    executionMode: "sequential",
    parameters,
    promptSnippet: "Use apply_patch for structured file edits with a complete Codex-style patch.",
    promptGuidelines: [
      "Provide one complete patch wrapped in *** Begin Patch and *** End Patch.",
      "Each file operation must start with *** Add File:, *** Delete File:, or *** Update File:.",
      "For renames, put *** Move to: immediately after the Update File header.",
      "Start each update hunk with @@. Use about 3 lines of context before and after each change; use @@ <class or function> when context is not unique, and do not duplicate nearby context.",
      "Prefix update lines with a space for context, - for removed lines, and + for added lines. Prefix every Add File line with +.",
      "Use *** End of File when the final update hunk targets the end of a file.",
      "Use relative file paths only. Pass the patch body directly in patch; do not wrap it in a shell command or heredoc.",
    ],
    async execute(_toolCallId, params, signal, _onUpdate, ctx) {
      const result = await applyPatch(params.patch, ctx.cwd, {
        signal,
        includeDiff: true,
        queue: (path, callback) => withFileMutationQueue(path, callback),
        progress: (progress) => {
          _onUpdate?.({
            content: [
              {
                type: "text",
                text:
                  progress.phase === "preparing"
                    ? `Preparing patch (${progress.completed}/${progress.total} files)`
                    : `Applying patch (${progress.completed}/${progress.total} files)${progress.change ? `: ${formatChange(progress.change)}` : ""}`,
              },
            ],
            details: progress,
          });
        },
      });
      return {
        content: [
          {
            type: "text",
            text: formatPlainResult(result.changes),
          },
        ],
        details: result,
      };
    },
    renderCall(args, theme, context) {
      let text = theme.fg("toolTitle", theme.bold("apply_patch"));
      if (!context.argsComplete) return renderText(text, context.lastComponent);

      const counts = countPatchOperations(args.patch);
      const total = counts.add + counts.update + counts.delete;
      const fileLabel = total === 1 ? "file" : "files";
      text += theme.fg("muted", ` · ${total} ${fileLabel}`);
      const formattedCounts = formatCounts(counts, theme);
      if (formattedCounts) text += theme.fg("dim", "  ") + formattedCounts;
      return renderText(text, context.lastComponent);
    },
    renderResult(result, { expanded, isPartial }, theme, context) {
      if (isPartial) {
        const progress = result.details as ApplyPatchProgress | undefined;
        if (progress?.phase === "preparing") {
          return renderText(
            `${theme.fg("warning", "…")} ${theme.fg("muted", `preparing ${progress.completed}/${progress.total}`)}`,
            context.lastComponent,
          );
        }
        if (progress?.phase === "applying") {
          const change = progress.change ? `  ${formatUiChange(progress.change, theme)}` : "";
          return renderText(
            `${theme.fg("warning", "…")} ${theme.fg("muted", `applying ${progress.completed}/${progress.total}`)}${change}`,
            context.lastComponent,
          );
        }
        return renderText(`${theme.fg("warning", "…")} ${theme.fg("muted", "applying patch")}`, context.lastComponent);
      }

      if (context.isError) {
        const message = result.content.find((item) => item.type === "text")?.text ?? "Patch failed";
        return renderText(`${theme.fg("error", "×")} ${theme.fg("error", message)}`, context.lastComponent);
      }

      const details = result.details as ApplyPatchResult;
      const counts = countChanges(details.changes);
      const fileLabel = details.changes.length === 1 ? "file" : "files";
      let text = `${theme.fg("success", "✓")} ${theme.fg("success", "applied")} ${theme.fg("muted", `· ${details.changes.length} ${fileLabel}`)}`;
      const formattedCounts = formatCounts(counts, theme);
      if (formattedCounts) text += `  ${formattedCounts}`;

      const visibleChanges = expanded ? details.changes : details.changes.slice(0, 3);
      const maxPreviewDiffLines = expanded ? Number.MAX_SAFE_INTEGER : 10;
      let previewDiffLines = 0;
      for (const change of visibleChanges) {
        text += `\n  ${formatUiChange(change, theme)}`;

        const diff = change.diff?.split("\n") ?? [];
        const remainingPreviewLines = maxPreviewDiffLines - previewDiffLines;
        const visibleDiff = expanded ? diff : diff.slice(0, Math.max(0, remainingPreviewLines));
        for (const line of visibleDiff) {
          text += `\n    ${formatUiDiffLine(line, theme)}`;
        }
        previewDiffLines += visibleDiff.length;
        if (!expanded && visibleDiff.length < diff.length) {
          text += `\n    ${theme.fg("dim", `… ${diff.length - visibleDiff.length} more diff lines  · `)}${keyHint("app.tools.expand", "expand")}`;
        }
      }
      const remaining = details.changes.length - visibleChanges.length;
      if (remaining > 0) {
        text += `\n  ${theme.fg("dim", `… ${remaining} more  · `)}${keyHint("app.tools.expand", expanded ? "collapse" : "expand")}`;
      }
      return renderText(text, context.lastComponent);
    },
  });
}
