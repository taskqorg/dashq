# AgentQ client integrations

Accepted under D16. AgentQ's [configuration A1–A5](https://github.com/taskqorg/agentq/blob/main/docs/spec/CONFIGURATION.md) owns profile semantics, typed plans/apply, authority and host verification. DashQ owns placement and the scoped browser adapter. Enrollment observation remains independent.

## G1. Interface

List client/profile, installation/runtime, observed routing, MCP/hooks and optimization state. Use stable profile IDs and show shared-config aliases in review. A configured checkmark is not verified activity. Missing capability, unsupported fields, bypass, disabled preference and stale data remain distinct.

Selection opens routing, optimization and supported owned integration setup; engine versions/defaults are read-only release/host context. Do not add model/provider/login, foreign instruction, permission or TaskQ admission controls. Codex Direct bypasses provider optimization but retains its preference/MCP/telemetry; Claude Code can keep the RTK hook; Hermes has its plugin; Desktop Chat is MCP-only.

Review shows exact profile/host/aliases and old→new changes from the host plan. Preserve unselected profiles, draft and client trust. Host-wide settings do not masquerade as per-profile controls. No free-form config editor. Disable writes with the precise reason for read-only authorization, unsupported scope, stale/incomplete snapshot, conflict or another pending mutation.

Use one editor per target. Lock its confirmed diff while applying, keep the previous observed values until matching host readback, and preserve drafts/selection/focus during refresh. Switching profiles does not discard drafts. Received newer settings invalidate old confirmation rather than overwrite the draft silently.

## G2. Results

- Accepted: server stored the immutable request; host configuration not yet confirmed.
- Configured: fresh host readback matches the intended revision and effective adapter state. Show reload/real-turn requirements.
- Verified: a new real client observation for the same profile, adapter and revision proves the selected behavior; no universal savings claim.
- Rejected: show reason; preserve draft and require a new plan for changed configuration.
- Unknown/partial: reconcile the same operation read-only; never blind retry or claim all targets changed. Late outcomes remain visible.

Current work and incidents are supporting detail. Show only actual supported metadata; an event is not proof that an agent is currently running a task. TaskQ owns task titles/progress when explicitly linked. Keep incident reporting consent separate from configuration actions; no automatic public issue from this screen.

## G3. Browser and delivery adapter

Resolve the guarded browser actor, workspace, installation and profile scope server-side. Require AgentQ's explicit configuration capability, session/Origin/CSRF checks and canonical host plan/revision. Observation and `/answer` credentials cannot authorize these mutations. Reuse the existing DashQ store/authorized service connection for bounded requests and results; no local file edits passed off as remote changes.

Authority follows coreq v0.11.0 operations and D16: the `agentq.configure` capability needs its own tick at approval; DashQ sets `authority` `{actor, workspace, profiles}` server-side (actor: the verified subject; workspace: the grant's; profiles: the installation's current snapshot profiles, for that workspace's administrator or the approving member only) and never reads it from the payload.

Implement against AgentQ's published typed envelope and shared conformance fixtures. Preserve immutable operation identity, expiry, source revision, scope and results through restarts. Reject arbitrary fields/paths/commands; no generic remote execution channel or consumer copy of AgentQ's config logic. Store acceptance is not host success; loss, revocation and full-storage behavior follow A3/A4.

## Delivery (#96)

DashQ's half of coreq wire v1 operations (v0.11.0) for `agentq.configure`; browser review/apply in [Browser adapter (#63)](#browser-adapter-63), no production grant. Contract sources, copied unchanged: [fixture/enrollment/agentq-operations.json](../../fixture/enrollment/agentq-operations.json) (coreq v0.11.0) and [fixture/agentq/configuration-fixtures.json](../../fixture/agentq/configuration-fixtures.json) (agentq `8385b9e`), replayed by `cargo test --locked`.

- Typed enqueue (`Enroll::configure`, internal only): the caller names the subject, installation, a 32-hex operation ID, 1–64 changes (`routing` `routed|direct`, `optimization` `enabled|disabled`, `mcp`/`hooks` `registered`, `plugin` `enabled`, one per profile and field) and, for an apply, the confirmed plan `{revision, digest}`. The server builds AgentQ's request envelope and `authority`: actor from the subject (`actor_id`: the subject when it is a coreq ID, else `actor-` plus 24 hex of its SHA-256), workspace and generation from the installation's live acknowledged grant, profiles from its current snapshot. The subject must administer that workspace or be the grant's approving member, the grant must hold `agentq.configure`, and every change must name a snapshot profile. Lifetime 300 s; payload at most 16 KiB; digest per coreq (`digest`). Refusals are safe codes (`invalid`, `no_grant`, `authority`, `no_profiles`, `profile_out_of_scope`, `scope`, `busy`, `replay_mismatch`, `store_full`, `unavailable`); nothing is queued.
- Store: one `operations` row per ID holds the immutable wire object; the same ID and content returns it, any change is `replay_mismatch`. Per installation: one unresolved apply, at most 16 pending per generation and 64 unresolved. Unresolved means no result and either delivered or still deliverable; an operation never delivered before expiry or revoke never ran and resolves. A full store refuses new operations; results use the control reserve. Prune keeps every unresolved operation and the newest 64 resolved per installation.
- Machine routes (device credential, active grant, per-device rate limit; the proxy passes them like `/machine/ingest`): `POST /machine/operations` `{capability}` returns the credential's own pending operations (no result, not expired, at most 16) and marks them delivered in the same commit; a capability outside the grant is 403 `scope`. `POST /machine/operations/result` `{operation, digest, result}` commits the first result before its `accepted` receipt; the same result repeats it, another is `replay_mismatch`, another digest `digest_mismatch`, another installation's or generation's operation `unknown_operation`. A result is a metadata object of at most 16 KiB (one level deeper than a snapshot), else 400.
- Readback (`Enroll::readback`): the actor or the workspace administrator reads `state` `queued`, `delivered`, `resulted` (with the host's result), `expired` or `revoked` (never delivered, never ran) or `unknown` (delivered, then revoked before a result). Store acceptance is not host success; AgentQ's result decides it, and a later revoke never undoes a committed result.

## Browser adapter (#63)

Guarded JSON routes on the #96 store; config `agentq_configure: true` turns them on (default off, no production writes before A5). Every call needs the signed-in subject (`X-Dashq-Subject`); a `POST` also this dashboard's `Origin`, JSON, at most 16 KiB and the CSRF token from the list reply (bound to subject, purpose and day). Off or store unavailable: 503 `unavailable`.

- `GET /agentq/configure?installation=I`: `{csrf, installation, operations}`, the newest 16 operations of `I` the subject may read (its own, or all as workspace administrator), so late outcomes stay visible.
- `POST /agentq/configure` `{installation, operation, changes}` asks the host for a plan; `{installation, operation, plan}` applies the plan of that earlier operation. The apply takes changes from the stored plan request and `{revision, digest}` from the host's plan reply, never from the browser, and needs the same actor and installation, a writable unexpired plan and a current snapshot at the plan's revision. Strict JSON, exactly these keys. Refusals (`{reason}`, nothing queued): `invalid`, `unauthorized`, `writes_disabled` (AgentQ's per-profile `writes.enabled` is not true, e.g. `not_released`), `not_planned`, `blocked`, `expired`, `stale_revision`, `unknown_operation` and every `Enroll::configure` code. The same operation and body repeats its reply after a lost receipt; a changed body is `replay_mismatch`.
- `GET /agentq/configure/<operation>`: transport `state` from readback plus G2 `status` and `reason`. Plans: `planning`, `planned` (with the host plan), `rejected`. Applies: `accepted` (stored or host-accepted; `awaiting_readback` once the host reports applied), `configured` (a fresh, complete snapshot observed at or after the result shows every intended value), `verified` (configured and AgentQ verified every changed profile), `rejected` (host reason, or `expired`/`revoked` when never delivered), `partial` and `unknown` (host reason, `revoked` when delivered then revoked, `readback_mismatch` when an applied value is no longer read back). Unknown is never success and is re-read, never retried.
- Page: with the route on, an installation whose connection holds `agentq.configure` and whose snapshot is fresh and complete gets draft selects for each profile field AgentQ lists as writable (only values other than the current one) and one Review/Apply editor; otherwise the precise reason. Refresh keeps drafts; an edited draft or a newer snapshot revision invalidates the reviewed plan; an unresolved apply locks the drafts.
- Open: a delivered apply revoked before its result (`unknown`) still blocks new applies on that installation (#96 rule); clearing it needs an owner decision. Browser rendering at the G4 sizes is not yet captured.


[Accepted reference](agentq-settings-reference.html), [preview](agentq-settings-preview.html), and [manifest](agentq-settings-reference.sources.json) contain synthetic profiles, versions and actions. They show the intended review/readback sequence, not released controls. The mutable fixture is not a production state machine; actual conflicts/races use A3/A4.

Conform at 1024 px/16 px light and 360 px/20 px dark, keyboard/touch, long labels and automatic refresh. Preserve charts and existing TaskQ/ServerQ blocks. Exercise all G2 outcomes, immutable review, profile draft retention, shared aliases, partial/missing data and all client capability differences. Live enablement additionally requires A5 evidence on real hosts with exact versions, authorized grants and restored baseline.
