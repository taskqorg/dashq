# ServerQ observation adapter (#40)

Requirements: D6, D15, adapters S1/S2/S7 and finding-tasks H1/H2.
The owner selected existing Hosts H4/H7 grouping, with findings separate by
workspace/installation/rule/first_seen and no inferred resource join.

Before: `cargo test --locked serverq_verified_episode_preserves_producer_time`
failed: the enrolled projection returned null instead of producer `verified_at`
1791907200. After: `cargo test --locked serverq_` passed the six affected checks.
The retained protection covers numeric evidence, exact catalog matching, missing
catalog, per-rule stale/future/unknown observations, incomplete projections,
retained and late samples, recurrence, and identical host labels.

The ingest/store/data/page chain uses synthetic observations on this Mac; it
performs no host probes or task creation. The exact ServerQ catalog comes from
[6b333b4](https://github.com/taskqorg/serverq/blob/6b333b435e6cfcdd81c0c793cf0071da8abbfb5f/rules.json).
Mismatches expose neither thresholds nor recommendations.

`tests/findings-browser.cjs` passed in local headless Chrome: three distinct
episodes, one verified and two open, no horizontal page overflow, no writes,
and disclosure/focus retained through two automatic 60 s refreshes. Inspected
screenshots: 1024 px/16 px light and 360 px/20 px dark, reduced motion. The
expanded records show producer times, installation/workspace, numeric evidence,
catalog hash and recommendation without clipping.

Retained evidence uses the existing seven-day enrollment history and at most
10,000 samples per installation/workspace. Capped/unavailable history is marked
incomplete and cannot establish recovery. This is not an unlimited episode archive.
Task creation, task-completion ordering and real producer transport on production
remain unqualified here; they belong to the separate handoff/live qualification.
No deployment or grant change.
