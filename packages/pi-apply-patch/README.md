# @shinynito/pi-apply-patch

Pi extension that adds a Codex-style `apply_patch` tool backed exclusively by a Rust N-API binary.

```sh
pi install npm:@shinynito/pi-apply-patch
```

The matcher is exact and SIMD-accelerated. Whitespace and Unicode punctuation mismatches are errors; there is no JavaScript or tolerant-matching fallback.
