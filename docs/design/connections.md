# Connection lifecycle interface

Accepted visual scenario under [D11](../../dashq.md#d11-independent-product-connections). [Enrollment C1–C7](enrollment.md) remains behavioral authority; this page adds presentation and conformance, not another enrollment protocol or permission class.

## L1. One list, independent states

Approved #169 placement ([owner acceptance](first-screen.md)): More → Connections (/integrations) and host detail, not above hosts. Default: current active/pending records. Explicit stale/superseded/revoked filters show counts/history. A direct installation/approval anchor reveals its row even when filtered. Expected hosts, resource warnings and non-ok ages stay visible in the overview. No deletion, credential revival or identity change.

Each product installation has a stable row with product, owner label, native/WSL runtime, connection state and observation age. Keep transport, producer freshness and downstream export separate. A recent receipt never refreshes an old observation. Disconnected/revoked rows keep history in detail, not active-failure counts. Unknown host activity must not be called stopped.

Select a row for identity, version, authorized scope, current evidence and lifecycle actions. Equal host labels never merge installations; product enrollment never inherits another product's authority. The same composition supports AgentQ, ServerQ-only and TaskQ-only installations. Show no dependency on Langfuse for ServerQ or TaskQ.

## L2. First connection

Add service leads to the existing product's one enrollment command, with the real HTTPS destination visible. Reuse installation and service; headless uses the same approval link/code in another browser. Approval shows verified account/workspace, discovered product/host/runtime, requested data and the matching short code; omit a workspace picker when there is only one choice. Deny/expiry before approval creates no active grant.

After approval display the actual pending stage: credential activation, durable first snapshot, then dashboard projection readback. Do not say Connected before matching projected receipt. Timeout retains stage and bounded recovery; no fresh grant on a blind retry. Open the new installation overview when verified. CPU warmup, unavailable probes and fewer than three samples remain explicit; no fabricated history.

## L3. Loss and recovery

- No contact: retain last observations with age, retry automatically with the existing grant; recovery produces no duplicate installation or new browser approval.
- Fresh connection, stale data: show both times and producer reason; reconnect is not a freshness repair.
- Export delayed: AgentQ snapshots/resources remain available while bounded forwarding retries. Other products remain independent.
- Revoke: review the named installation and effect; on acknowledged revocation retain history and local work. New browser approval is required to reconnect; preserve identity/history with a new generation, never revive the old credential.
- Local offline disconnect: stop sending, display revocation pending until server acknowledgement; retained revoke-only retry does not resume telemetry.
- Identity lost after reinstall: display a new installation and preserve the old one separately. No hostname merge or automatic transfer of history or authorization.

Offline revocation evidence boundary (#60 owner answer): pending revocation is verified on the host. Until acknowledgement, DashQ retains the last server evidence and reports host activity unknown; it does not infer local disconnect from silence. No protocol extension.

Connection refresh preserves selection, focus, scroll and approval/review state. Scope changes and destination changes retain C1/C4 review and destination-bound backlog rules. Observation scope grants no task or host-configuration actions.

## L4. Reference and gates

[Fragment](connections-reference.html) and [preview](connections-preview.html) are the accepted presentation reference. Every host, short code, identity, URL and observation is synthetic. Buttons prefixed Simulate and the Demo state selector belong only to the fixture. The fixture models one selected installation/scenario and a newly added row; it is not a live registry or a production enrollment client.

Browser checks at 736 px/16 px light and 360 px/20 px dark cover approval/code match, pending readback/timeout, network recovery, revoke/new approval, stale data, export delay, offline revoke and separate new identity. Qualify the same scenarios against real services under E1–E10 before release, including expiry, cancellation, restart, duplicate requests, cross-workspace denial and actual source freshness. UI evidence never substitutes for those gates.

The existing #36 implementation is a baseline, not proof of this reference's full conformance; follow-up work retains its capabilities. #4 owns end-to-end release qualification; AgentQ #95 retains platform/migration evidence.

Whole-dashboard first launch, standalone TaskQ and partial/all-source failure composition follows [U1–U4](startup-outages.md); its fixture shortcuts do not replace L2's enrollment flow.
