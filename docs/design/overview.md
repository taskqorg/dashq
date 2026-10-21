# Host resources

Candidate #169 under [D8–D10](../../dashq.md#d8-page), awaiting owner visual approval. [First screen](first-screen.md) owns placement; [specq W1–W15](https://github.com/taskqorg/specq/blob/fea6e540efa6971c5739453b198183cbb3fb010d/design/dashboards.md) owns shared behavior. No Rust or producer contract changes.

## Overview and detail

Initial grid: current CPU percent, RAM used/total, each filesystem used/total. A zero-based capacity bar needs a known total. Keep units, per-field source age, warnings and accessible missing-data reasons, with one visible legend. History plots move to detail.

Host detail: CPU/RAM 30 min trends on a shared 0–100% scale, disk history, filesystem roles and evidence. Label endpoints/window; three real samples establish history under U1. Warmup, gaps, resets, stale tails, capped reads and unsupported metrics retain D4.4 semantics. Unknown total has no bar; missing history no continuous line. Identity follows H1–H7. Only producer/approved policy thresholds establish warnings; no new threshold here.

## Updates and reference

Keep automatic 60 s refresh, immediate new values, about 1.3 s local neutral tint; no transitions under reduced motion. Delta uses percentage points. Numeric rise alone proves neither failure nor recovery. Preserve rows, focus, disclosures, selection, drafts, scroll; failed refresh retains values, ages and reason.

Older [resource fragment](overview-reference.html), SHA-256 fae9772c872d5dfc3128886b598328b298f7d0f8b5873509f3a2465c1a0d41ae, remains historical chart/update evidence. Placement, sample button, capacities and 85/90% limits are illustrative. [Host-first prototype](host-first-preview.html) is the candidate composition; no synthetic data enters a live view.

Acceptance: HF1/HF4, long names, several disks, unknown total/history, 16/20 px, 360/1280, both themes/reduced motion. Multiple actual refreshes, threshold transitions and live producer samples still need runtime evidence; a static drawing cannot qualify them.
