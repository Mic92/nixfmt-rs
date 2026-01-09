## Fixparser Workflow Checklist

1. Find a minimal reproducer for the mismatch.
2. Add a regression test capturing the reproducer.
3. Add temporary debug output to inspect parser/IR behavior.
4. Check the reference nixfmt implementation for expected behavior.
5. Implement the minimal fix in the parser/pretty pipeline.
6. Run `cargo test` to confirm the regression test now passes.
7. Clean up debug output, format, and prepare the commit.

