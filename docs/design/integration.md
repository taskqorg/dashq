# Independent products, one dashboard

Owner requirement: TaskQ, AgentQ and ServerQ remain independently installable and useful. DashQ integrates whichever products the owner connects. The accepted [enrollment contract](enrollment.md) and [D11](../../dashq.md#d11-independent-product-connections) amend D1–D8. This inventory supplies context; enrollment owns the connection lifecycle.

## Existing common ground

| Layer | Existing responsibility | Integration boundary |
|---|---|---|
| specq | Shared engineering, interaction and visual rules | Semantics and review; no runtime dependency |
| coreq | Linked Rust crate: install, update, service, common status, incidents | Reuse inside each binary; no central daemon |
| taskq | Board-owned tasks, snapshots, questions and operation receipts | Producer owns queue state and answer validation |
| agentq | Agent integration, bounded telemetry queue, resource samples | Existing OTLP to Langfuse; no mandatory collector for other products |
| serverq | Rule catalog, checks, reports and incident verification | Read-only checks; remediation remains separate |
| dashq | Source reads, aggregation, freshness and rendering | Adapters preserve each source's meaning and authority |

coreq already removes repeated lifecycle code. Its common status keys do not describe every product capability, establish cross-product host identity or transport remote reports. A matching `host` name is insufficient identity proof.

## Integration boundaries

1. Connect an explicit source: product kind, supported schema, scope and access reference. Do not scan hosts or require installation of the other products.
2. A DashQ adapter reads that product's existing interface and returns an internal view with source identity, observation time, freshness, completeness and available capabilities. Keep producer-specific payloads; do not force every product into a task model.
3. Compose views from available capabilities. TaskQ alone supplies boards and questions; ServerQ alone supplies checks; AgentQ alone supplies activity and resource history. Combined sources enrich the same entities only with a verified mapping.
4. Keep source observation time separate from collection time. A working connection does not make an old observation fresh. Disconnecting DashQ must not stop work, checks, updates or standalone telemetry.
5. Route an owner action to its owning product through a separately approved action path. The dashboard displays the source choice IDs, recommendation and free-text contract; the producer validates authority/revision and returns a receipt.

The connection proposal adds narrow handlers to the existing DashQ process and a shared client inside existing product services. No separate universal HTTP server, plugin loader or central scheduler. Two-consumer conformance cases justify common coreq behavior; standalone operation remains intact.

### Current paths and gaps

| Source | Available now in contracts/source | Missing for the proposed integration |
|---|---|---|
| TaskQ | `status --json`, service snapshot, fenced/idempotent commands and receipts | DashQ currently reads GitHub labels; canonical remote snapshot/answer access needs a defined adapter and authority |
| AgentQ | OTLP events, installation identity, Linux/macOS resource samples | Windows/WSL probes and live history qualification; independent install identity is not a shared host ID |
| ServerQ | Local schema-1 report, rule IDs/catalog, timestamps, manual/unknown results | Safe multi-host delivery and resource mapping; no remote action protocol |
| coreq | Shared local status and incident helpers | No generic remote reporting/enrollment contract; do not copy raw local status to a public sink |

Product-specific freshness remains authoritative. ServerQ's report includes filesystem/detail fields and local status can include paths; export needs a reviewed projection, not raw-file forwarding. AgentQ hashes filesystem identity with an installation key; DashQ cannot infer its mapping to a ServerQ path.

Connection states distinguish not configured, disconnected, unsupported capability, stale observation and failed read. A source that never offered a capability is not a broken installation. An expected source that disappears remains visible with its last observation.

## Scenarios and acceptance

| ID | Scenario | Required result |
|---|---|---|
| I1 | Install/use any producer without DashQ or sibling products | Its primary workflow works without shared enrollment |
| I2 | Connect only one producer | Useful available view; no fabricated empty panels or unrelated warnings |
| I3 | Connect all producers on one machine | Join only verified identities; no double-counted resources; Windows/WSL scopes remain distinct |
| I4 | Disconnect DashQ or one source | Producers continue; other sources remain useful; retained data keeps age and reason |
| I5 | Old/new producer schemas overlap | Supported fields render; unknown capabilities are explicit; no forced fleet-wide update |
| I6 | Two clients answer one question, or delivery is uncertain | Source IDs/revision/op ID preserved; stale/retry behavior follows TaskQ; accepted does not imply resumed |
| I7 | ServerQ warns without TaskQ installed | Finding, evidence and recommendation remain useful; no automatic repair or mandatory task creation |

## Ownership and remaining decisions

The [complete connection proposal](enrollment.md) starts from AgentQ PRODUCT R5/R9/R10 and covers independent enrollment, first data, recovery, revocation and reuse.

- [coreq #22](https://github.com/taskqorg/coreq/issues/22): reusable contract proposal, two-consumer proof, independent operation and compatibility. Shared semantics go to specq; Rust helpers stay in coreq.
- [serverq #21](https://github.com/taskqorg/serverq/issues/21): report projection, recommendation references and resource identity without requiring AgentQ or TaskQ.
- [dashq #24](https://github.com/taskqorg/dashq/issues/24): connection/adapters, capability-based views and proposed action authority; consumer composition belongs here. Proposal: [adapters](adapters.md).
- Existing delivery work: [dashq #4](https://github.com/taskqorg/dashq/issues/4), [agentq #60](https://github.com/taskqorg/agentq/issues/60). Neither is an approved generic transport merely because an issue exists.
- Resource qualification stays in [agentq #85](https://github.com/taskqorg/agentq/issues/85) and [#86](https://github.com/taskqorg/agentq/issues/86). Reuse TaskQ #886/#887; do not rebuild its state machine in DashQ.

Resolve transport for remote snapshots, source enrollment/revocation, verified host mapping and answer authorization before implementation. Compare existing paths first. The recommended architecture above is a proposal; deployment and runtime permissions remain unchanged.

## AgentQ standalone connection

Current proposal: [complete enrollment C1–C7](enrollment.md). One browser approval, per-installation access, automatic snapshot/readback, durable recovery and revocation; reusable by independently installed products. The previous manual Langfuse-source setup is withdrawn; its investigation remains in git history.

## Reviewed revisions

Contracts inspected at these source revisions; this is not deployment evidence.

- [specq](https://github.com/taskqorg/specq/tree/4053eeda29feb35c383321ab2026c2baf8bade0b): `4053eeda29feb35c383321ab2026c2baf8bade0b`.
- [coreq](https://github.com/taskqorg/coreq/tree/2a74456395548d967109f7d4c118f25faef633d2): `2a74456395548d967109f7d4c118f25faef633d2`.
- [taskq](https://github.com/taskqorg/taskq/tree/90fa95b1e3281cb3a07cabd92fa7f2f310c6523b): `90fa95b1e3281cb3a07cabd92fa7f2f310c6523b`.
- [agentq](https://github.com/taskqorg/agentq/tree/b96179d5806795cda10e5cb295e08df7fa41f113): `b96179d5806795cda10e5cb295e08df7fa41f113`.
- [serverq](https://github.com/taskqorg/serverq/tree/444e4944590f4bce96bfeba5893af6828a39a3c1): `444e4944590f4bce96bfeba5893af6828a39a3c1`.
- [dashq](https://github.com/taskqorg/dashq/tree/a12cb42df1facf9bec4aaa56e57dbb8a3b96a484): `a12cb42df1facf9bec4aaa56e57dbb8a3b96a484`.
