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

Implement against AgentQ's published typed envelope and shared conformance fixtures. Preserve immutable operation identity, expiry, source revision, scope and results through restarts. Reject arbitrary fields/paths/commands; no generic remote execution channel or consumer copy of AgentQ's config logic. Store acceptance is not host success; loss, revocation and full-storage behavior follow A3/A4.

## G4. Reference and acceptance

[Accepted reference](agentq-settings-reference.html), [preview](agentq-settings-preview.html), and [manifest](agentq-settings-reference.sources.json) contain synthetic profiles, versions and actions. They show the intended review/readback sequence, not released controls. The mutable fixture is not a production state machine; actual conflicts/races use A3/A4.

Conform at 1024 px/16 px light and 360 px/20 px dark, keyboard/touch, long labels and automatic refresh. Preserve charts and existing TaskQ/ServerQ blocks. Exercise all G2 outcomes, immutable review, profile draft retention, shared aliases, partial/missing data and all client capability differences. Live enablement additionally requires A5 evidence on real hosts with exact versions, authorized grants and restored baseline.
