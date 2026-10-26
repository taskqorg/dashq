# Approved DashQ operator redesign

Owner accepted the consolidated design and authorized a PM planning/decomposition task on 2026-10-22. [PM planning task #177](https://github.com/taskqorg/dashq/issues/177) consumes this package; implementation, rollout, enrollment and live operations are not started by it.

- [Requirements and PM brief](implementation-brief.md): full target, decisions, preservation ledger, cross-service deliverables and qualification gates.
- [Scenarios/data](scenarios-and-data.md): DQ-01–DQ-10 and T1–T9 coverage.
- [Visual foundations](design-system.md): hierarchy and operator grammar.
- [Public reference](overview-candidate.html): synthetic derivative of the accepted mock; no real operational counts or financial amounts. Build with `python3 docs/design/operator-redesign/build-overview.py`.
- [Review](review/README.md): independent/mutual review and final correction dispositions.
- [Approval fingerprints](approval.json): exact owner-accepted mock and private packet manifest hashes. Hashes distinguish accepted input from the public derivative.
- [Minimal Overview runtime evidence](../evidence/minimal-overview/README.md): #219 fixture comparison and retained detail/navigation coverage; other redesign surfaces remain separately gated.

## Private input and authority

The source repository is public. Exact accepted HTML and full live-data/reviewer evidence remain in the owner-only input packet on the registered DashQ primary checkout on host mac, outside committed files. The native planning worker may read `docs/design/approved/r1/manifest.json` there and verify its hashes. Find the primary checkout through the existing registered project/Git worktree identity; do not search home or the disk. Missing/mismatched input is a blocker, never permission to publish or replace it. Never commit, upload or quote private inputs in a task, plan or report.

The packet manifest SHA-256 is `d209d6ac2729649f088440eaaebef63124bcafbae30090d61ab6d8cc38419a63`. The exact accepted mock SHA-256 is `8a39efd555cc9317f8ca6454a4e7e6b0241370ccc8fb2c03cd8c893840188dfb`.

Audit source baseline: DashQ `0db7eca8a1df8165e625ed6acbfdc8fa4847d403`; TaskQ `51527d1d217c2c51621b74832be8957775d1bfa6`; AgentQ `b560e58966d1bebe65ee7980a2d49c1843ea3167`; CoreQ `75984a1b4fc8df4a3d96a6153768c8dc020df368`; ServerQ `472f16542b3016ca598885c22fad4528e2c75e09`. Reconcile fresh main/releases and board state; these pins are evidence, not claims about current deployments.

TaskQ components are unchanged published v4.6.30 assets with provenance hashes. Shared task/menu/workspace styles and the PM composer still need complete upstream extraction/adoption. System detail descriptions preserve destinations and are not a completed implementation.

## PM outcome

Return a reviewed cross-service plan and coherent task links. Map every scenario, telemetry requirement, review item and preservation row to its owner, dependency and observable check. Link existing work before creating anything. Create implementation children held; ask the owner to accept the reviewed plan before starting them. Escalate only genuinely new product/authority/cost choices. Review technical choices from source, volume and qualification evidence.
