# Independent products, one dashboard

Owner requirement: TaskQ, AgentQ and ServerQ remain independently installable and useful. DashQ integrates whichever products the owner connects. This design proposal does not amend [D1–D7](../../dashq.md), enable host push or authorize writes.

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

## Proposed integration

1. Connect an explicit source: product kind, supported schema, scope and access reference. Do not scan hosts or require installation of the other products.
2. A DashQ adapter reads that product's existing interface and returns an internal view with source identity, observation time, freshness, completeness and available capabilities. Keep producer-specific payloads; do not force every product into a task model.
3. Compose views from available capabilities. TaskQ alone supplies boards and questions; ServerQ alone supplies checks; AgentQ alone supplies activity and resource history. Combined sources enrich the same entities only with a verified mapping.
4. Keep source observation time separate from collection time. A working connection does not make an old observation fresh. Disconnecting DashQ must not stop work, checks, updates or standalone telemetry.
5. Route an owner action to its owning product through a separately approved action path. The dashboard displays the source choice IDs, recommendation and free-text contract; the producer validates authority/revision and returns a receipt.

No new universal HTTP server, plugin loader or central scheduler is justified by the current cases. First implement adapters over supported interfaces. Extract a coreq primitive only when two consumers need the same behavior and standalone operation remains intact.

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

First concrete connection: [AgentQ standalone](#agentq-standalone-connection). It reuses the existing telemetry path and identifies the minimum DashQ changes before any shared transport is built.

- [coreq #22](https://github.com/taskqorg/coreq/issues/22): reusable contract proposal, two-consumer proof, independent operation and compatibility. Shared semantics go to specq; Rust helpers stay in coreq.
- [serverq #21](https://github.com/taskqorg/serverq/issues/21): report projection, recommendation references and resource identity without requiring AgentQ or TaskQ.
- [dashq #24](https://github.com/taskqorg/dashq/issues/24): connection/adapters, capability-based views and proposed action authority; consumer composition belongs here.
- Existing delivery work: [dashq #4](https://github.com/taskqorg/dashq/issues/4), [agentq #60](https://github.com/taskqorg/agentq/issues/60). Neither is an approved generic transport merely because an issue exists.
- Resource qualification stays in [agentq #85](https://github.com/taskqorg/agentq/issues/85) and [#86](https://github.com/taskqorg/agentq/issues/86). Reuse TaskQ #886/#887; do not rebuild its state machine in DashQ.

Resolve transport for remote snapshots, source enrollment/revocation, verified host mapping and answer authorization before implementation. Compare existing paths first. The recommended architecture above is a proposal; deployment and runtime permissions remain unchanged.

## AgentQ standalone connection

Proposal AG1–AG6. Inspected AgentQ `938b6288cbb0446b28e9caf7898e336047082369` and DashQ `cc2b41f6d56d9c79401a3d1438062f4c8b10377a`. Source inspection, not live qualification. The owner requirement is independent products; this particular connection mechanism awaits design acceptance.

### AG1. Reuse the existing data path

`AgentQ service → existing bounded OTLP queue → Langfuse ← DashQ adapter → overview`

This first connection means observing an installation through its existing telemetry source. AgentQ needs no DashQ URL, listener or second background process. It continues to run when DashQ is absent. Langfuse is the existing store in this deployment, not a new dependency imposed on ServerQ or TaskQ.

| Option | User effect | Cost / boundary |
|---|---|---|
| Existing Langfuse path (recommended first) | Connect a telemetry source once, then select AgentQ installations | Reuses today's sender/reader; trusted AgentQ hosts retain Langfuse project credentials |
| `agentq enroll <dashq-url>` | Each installation receives a revocable device credential; no Langfuse secret on that host | Requires DashQ enrollment, authenticated ingestion, forwarding and durable credential state; changes D1–D3 and agentq #60 |

The current AgentQ dispatcher has no `enroll` branch. Its appearance in agentq.md is a target, not a runnable connection procedure. Do not print it as a working copy/paste command. The first option does not implement or cancel team enrollment.

### AG2. Operator journey

1. **Existing installation.** Read `agentq doctor` and `agentq telemetry status`. Do not rerun onboarding or change agent routing merely to add a dashboard. Missing telemetry configuration uses the existing `agentq langfuse` secure-input command, only for the intended project. Reuse a working destination; do not silently repoint it.
2. **Source configuration.** An administrator configures the same Langfuse project on DashQ through its protected config. Reuse its current credential handling and UI guard. A proposed `Sources → AgentQ` view displays connection state and installation selection; initially its setup instructions may point to administrator configuration rather than adding a write API.
3. **Select scope.** Read candidate installations only from that explicitly configured project. Show installation identity, host label, runtime, version and last observation. The administrator records selected installation IDs as expected sources in the existing config; no network-wide discovery or automatic hostname merging.
4. **Verify.** With consent for one synthetic event, run `agentq telemetry verify`. Its successful readback proves AgentQ → Langfuse only. Independently read a matching installation heartbeat through DashQ `/data`, then inspect the rendered row. Never promote a `verify` span into a heartbeat or a resource sample.
5. **Use.** Show installation/version/freshness immediately when present; add resource bars from valid samples and trends only after at least three samples. Under the current 300 s cadence, a fresh start may need about 15 minutes plus delivery/cache delay for three samples. Do not animate invented history while waiting.
6. **Disconnect.** Remove the selected installation from DashQ configuration. DashQ stops displaying/querying it as selected; AgentQ and its Langfuse delivery continue. Disabling telemetry or revoking Langfuse credentials is a separate action with its scope explained. No history is deleted.

For one shared project, a selection filter is presentation scope, not a credential security boundary. This option provides neither isolated per-device revocation nor exclusive access to one installation. Use the team design if those are required; never label project credentials as device tokens or read-only without proof.

### AG3. Small connection contract

One private connection record in existing DashQ config: local source ID, product kind, source reference, selected installation IDs and optional display aliases. Credentials remain in the existing protected source configuration. Add no database, generic plugin registry or duplicated event queue.

- Entity key: `(source ID, product, installation ID)`. Runtime and host name are attributes, not cross-product identity proof. A reinstall with a new ID is a new installation; an explicit operator mapping may replace the old expected installation.
- Observations carry their producer timestamp; the adapter separately records read time, source state and completeness. Resource freshness follows AgentQ's 900 s limit; installation heartbeat freshness follows DashQ D4.1's 7200 s limit. Local AgentQ status freshness is not remote heartbeat freshness.
- Capabilities are adapter/schema knowledge plus qualified evidence, never inferred from an empty query. Missing resource events mean unknown history/availability until producer version/platform is qualified. No activity in a successfully read, uncapped window can be zero; failed or capped reads cannot.
- Keep an expected row when no event remains in the 24 h query window or after DashQ restarts. The configured ID survives; its last-seen time is unknown if no retained observation supplies it. Do not introduce persistent last-value storage for this slice.
- Keep errors isolated: a failing optional resource query cannot erase a valid fleet read. Honor per-source timeout, cap and incomplete semantics; do not query unlimited history to find a missing installation.
- The overview uses compact semantic status beside the installation. Details own source diagnostics and connection instructions. No repeated `connected` badges on every metric; connection health and resource warnings remain distinct.

### AG4. Concrete implementation gaps

| Gap found in source | Minimum change | Owner |
|---|---|---|
| `load_config` requires Langfuse, GitHub token, nonempty project list and ServerQ path together | Make source blocks independently optional; validate present blocks fully; no configured source means setup, not all-sources-failed | DashQ |
| Sources and Attention assume missing providers are observation gaps | Distinguish intentionally absent from failing configured sources; render only available sections; preserve existing configuration compatibility | DashQ |
| Fleet is rebuilt only from recent events; host rows join by host name/runtime | Keep selected expected installations in config; key AgentQ rows by installation; defer cross-product joins without verified mappings | DashQ |
| Source reader has no `agentq.resource` query | Implement the already planned resource adapter under #23 after AgentQ #86 qualification; do not duplicate collection | DashQ |
| Latest AgentQ release currently comes from the configured GitHub project reads | Decouple optional version lookup from board access; a missing latest version must not block AgentQ data or demand a GitHub token | DashQ |
| `telemetry status` does not expose installation identity | If required by the connection flow, add a safe identity field through the existing command, specified first; no new service/API | AgentQ |
| Installation payload currently exports selected heartbeat fields, not every local status field | Display only observed fields; routing/hooks/engine health stay unavailable until a producer task exports and qualifies them | AgentQ / DashQ |

Implementation order: optional source configuration and failure isolation; selected installation identity/lifecycle; end-to-end connection qualification. Resource charts reuse #23/#85/#86. Team enrollment #60 and host-push #4 remain separate. No coreq extraction is required to complete this first path.

### AG5. Reuse for the next products

| Product | Reuse | Keep product-owned |
|---|---|---|
| ServerQ | Source selection, schema check, observation/read time, expected-source lifecycle, unknown states | Local report adapter first; remote delivery unresolved; rule catalog and sanitized findings; no mandatory AgentQ or Langfuse |
| TaskQ | Same connection lifecycle and rendering state; board/project scope instead of installation where appropriate | Canonical snapshot and action receipts, board authority, choice/revision/op IDs; no mandatory AgentQ or Langfuse |

Share these semantics through specq after approval and the proven Rust helpers through coreq when a second consumer actually uses them. Shared does not mean identical transports or a new daemon. Answer/write capabilities remain outside this read-only AgentQ connection.

### AG6. Acceptance before shipping

- AgentQ-only configuration starts DashQ without GitHub credentials, board projects or ServerQ; existing all-source config still works. No sources shows setup; a broken configured source shows failure.
- One selected installation appears with matching ID/version/timestamp; another installation in the same project is outside the selected display scope. Two identical host labels do not merge; native Windows and WSL stay distinct.
- Verify event readback, actual heartbeat `/data` readback and rendered row are separately evidenced. Resource graphs require qualified producer samples; no fabricated health, usage, cost or engine state.
- Offline source, expired credentials, unsupported schema and query cap retain truthful partial results. An expected installation absent for more than 24 h survives a DashQ restart as unknown/stale evidence permits.
- Disconnect/reconnect leaves AgentQ routing, primary work and telemetry destination unchanged. Replacing an installation is explicit; removing the view does not claim credential revocation.
- Browser and report output expose no credentials or raw local status. Review compact normal, waiting, failed and disconnected states in both themes and readable large text.

Required specification amendments after acceptance: DashQ D2 (optional sources/config scope), D4 (selection and AgentQ resource adapter), D6–D8 (absent versus unavailable, partial failures and composition). D1/D3 remain read-only with the existing guard. AgentQ needs a contract amendment only for any added diagnostic identity output; its existing telemetry route remains unchanged.

## Reviewed revisions

Contracts inspected at these source revisions; this is not deployment evidence.

- [specq](https://github.com/taskqorg/specq/tree/4053eeda29feb35c383321ab2026c2baf8bade0b): `4053eeda29feb35c383321ab2026c2baf8bade0b`.
- [coreq](https://github.com/taskqorg/coreq/tree/2a74456395548d967109f7d4c118f25faef633d2): `2a74456395548d967109f7d4c118f25faef633d2`.
- [taskq](https://github.com/taskqorg/taskq/tree/90fa95b1e3281cb3a07cabd92fa7f2f310c6523b): `90fa95b1e3281cb3a07cabd92fa7f2f310c6523b`.
- [agentq](https://github.com/taskqorg/agentq/tree/b96179d5806795cda10e5cb295e08df7fa41f113): `b96179d5806795cda10e5cb295e08df7fa41f113`.
- [serverq](https://github.com/taskqorg/serverq/tree/444e4944590f4bce96bfeba5893af6828a39a3c1): `444e4944590f4bce96bfeba5893af6828a39a3c1`.
- [dashq](https://github.com/taskqorg/dashq/tree/a12cb42df1facf9bec4aaa56e57dbb8a3b96a484): `a12cb42df1facf9bec4aaa56e57dbb8a3b96a484`.
