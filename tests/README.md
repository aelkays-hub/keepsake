# Integration Tests

Cross-crate integration tests live here. They are run with `cargo test --workspace`.

Tests that require real media (corpus tests) are `#[ignore]` by default and only run
when `KEEPSAKE_CORPUS` is set:

```bash
KEEPSAKE_CORPUS=/path/to/corpus cargo test --workspace -- --include-ignored corpus
```
