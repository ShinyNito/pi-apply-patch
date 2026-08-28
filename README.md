# pi-apply-patch

`@shinynito/pi-apply-patch` adds a Codex-style `apply_patch` tool to Pi. Parsing, exact hunk matching, diff preparation, and filesystem mutation run in Rust through N-API; TypeScript only integrates the native engine with Pi.

There is no TypeScript implementation fallback. Hunk matching is exact: trailing whitespace, surrounding whitespace, and Unicode punctuation are not normalized. A mismatch fails before any file is changed.

## Install

```sh
pi install npm:@shinynito/pi-apply-patch
```

## Packages

- `crates/apply-patch-core`: parser, SIMD-accelerated exact matcher, and filesystem operations
- `crates/apply-patch-node`: N-API binding
- `packages/apply-patch-node`: public Node SDK and platform binary loader
- `packages/pi-apply-patch`: Pi extension published as `@shinynito/pi-apply-patch`

The Node package publishes separate optional packages for macOS, Linux glibc, Linux musl, and Windows on ARM64 and x64. If the exact native package for the current platform is absent, loading fails.

## Patch format

```text
*** Begin Patch
*** Update File: src/example.ts
@@
-old line
+new line
*** End Patch
```

Supported operations:

- `*** Add File: path`
- `*** Update File: path`
- `*** Move to: path`
- `*** Delete File: path`
- multiple files and hunks
- `*** End of File`

The complete patch is prepared before writes begin. Preparation failures leave every file unchanged. An I/O failure during the write phase does not roll back operations already applied.

## Develop

Requires Node.js 22.19 or newer, pnpm 11, and stable Rust.

```sh
pnpm install
pnpm --filter @shinynito/apply-patch-node test
pnpm --filter @shinynito/pi-apply-patch typecheck
pnpm bench
```

Local native commands build only the host target. Cross-platform binaries are built and tested by GitHub Actions.

## License

MIT
