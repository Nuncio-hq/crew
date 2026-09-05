# Final virtualization parity

Status: DONE. No source or test edits.

- Frozen uninstrumented bundle: `/tmp/crew-planner-virtua-ack-final-dist`.
- Full `virtualization.spec.ts`, smoke project, `CI=1`, `--retries=0`.
- macOS: 11/11 passed, 40.2s; Linux Ubuntu Noble arm64 / Playwright1.60: 11/11 passed, 53.7s.
- Both JSON reports: expected11, unexpected0, flaky0, skipped0. No initial failures or hidden retries.
- Includes cascading older-page case08 (22.0s mac / 27.5s Linux) and continued-wheel rest case09 (5.0s / 7.0s). Existing drift, rollback, rest and virtualization thresholds unchanged.
- Host and container index SHA256: `0f31b1dfcac329d38eef548ed091d9406ebb6f5db0654f1d635ef8b2097c0263`.
- Host and container test SHA256: `79954c5867ab55fa3c5a77aac78f77acd49411a25003baca9944631bd1c3124c`.

Evidence: `/tmp/crew-pool-final-virtualization-mac.log`, `/tmp/crew-pool-final-virtualization-linux.log`; corresponding `-report.json` files retain structured results.

Concern: Linux image arm64 differs from hosted CI amd64. No unresolved correctness findings.
