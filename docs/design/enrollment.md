# Connect independent products

Accepted implementation contract. Implements AgentQ PRODUCT R5/R9/R10 and amends dashq D1–D8 through D11. Follows [shared connection rules](https://github.com/taskqorg/specq/blob/main/engineering/product-connections.md). Supersedes the withdrawn source-selection proposal. Acceptance is not evidence that the feature is shipped; deployment changes require the scoped release task.

## C1. User outcome

An installed product connects to DashQ with one command and one browser approval. No configuration-file editing, copying API keys, selecting installation IDs, opening host ports or installing another service. Each product remains useful without DashQ and during its outages.

Accepted entry points: `agentq enroll <https-url>`, later `serverq enroll <https-url>` and `taskq enroll <https-url>`. These are design targets, not commands available in current releases. Agent-led setup invokes the same path and never approves the browser grant on the person's behalf.

### First connection

1. The command inspects the existing product installation, service, connection and protocol support. Reuse known settings. If an update is necessary, use the product's verified updater without interrupting active work. No rerun of client onboarding, hook edits or provider changes.
2. AgentQ requests a short-lived pairing session from the named DashQ HTTPS origin. It opens the approval page in the system browser; a headless host prints the same link and short code for a browser elsewhere. Never require a callback port on the host. Show the real destination before authentication; reject credentials in URLs, TLS errors and cross-origin credential redirects.
3. The person signs in through DashQ's existing identity provider if needed. Show the account/workspace, product, host label, native/WSL runtime, matching code and requested data scope. Pick a workspace only when there is a choice. One approval binds this installation to that account and workspace; cancellation creates no active grant.
4. The command receives a per-connection credential into protected storage, activates the connection atomically and wakes the existing service. No secrets pass through command arguments, browser URLs, chat or logs. Repeating enrollment to the same destination returns the current connection and verifies it, without duplicate rows or grants.
5. The service immediately sends a real installation snapshot and a verification receipt request. DashQ durably stores them and reads the snapshot through the same projection used by the browser. The command reports `Connected` only after this readback matches its connection, schema and snapshot ID. The browser opens that installation's overview automatically; polling handles an already-open page.
6. The overview shows version, runtime, last observation, component status and available resources. Empty activity is not an error; unavailable probes retain reasons. CPU warmup and history collection are explicit. The connection does not wait for an artificial workload or invent samples to fill a graph.

### Disconnect, revoke, reconnect

- `disconnect` on the host revokes the grant and removes only this connection's credentials/state after acknowledgement. Offline: stop local sending immediately, retain a bounded revoke-only record and retry when reachable; report `Disconnected locally; revocation pending`, never claim server revocation.
- DashQ's `Revoke` action names the installation and effect, then revokes its grant transactionally. Every request checks revocation; no auth cache may extend a revoked grant. Offline hosts observe this on their next request and stop retrying with that credential. Existing agent work and local checks continue.
- Reconnecting a revoked installation requires a new browser approval. Preserve installation identity and history; issue a new connection generation and credential. Old credentials cannot revive it. An uninstall/reinstall with lost identity creates a new installation, never an automatic hostname merge.
- Workspace/destination changes show old → new and the data scope before switching. A host can have one active DashQ destination per product installation in this design. Already queued records stay bound to their original destination and grant; never drain old private data into a newly selected workspace.
- Revoke/disconnect retains previously accepted history under its retention policy. Deletion of stored data is a separate, explicit action. The overview keeps revoked/disconnected installations available in detail; they are not active failures.

## C2. One shared mechanism, product-owned data

`product service → coreq connection client → DashQ ingestion + store → dashboard`

AgentQ's eligible telemetry is also forwarded by DashQ to its configured Langfuse destination. Langfuse credentials remain on the server. Snapshot visibility and resource charts do not wait for Langfuse ingestion or depend on Langfuse being available.

| Owner | Responsibility |
|---|---|
| specq | Reusable connection semantics, evidence, consent, independence and UI rules |
| coreq library | Pairing client, protected credential handling, connection state, bounded authenticated delivery, acknowledgement/recovery; no daemon or UI framework |
| DashQ process | Existing web UI plus pairing/ingestion handlers, membership checks, connection registry, durable receipts/snapshots/history, forwarding and rendering |
| AgentQ | Installation identity, engine/routing health, resource probes, metadata projection; existing service and sender queue |
| ServerQ | Same enrollment/delivery path; sanitized rule findings and reports; no dependency on AgentQ or Langfuse |
| TaskQ | Same enrollment/delivery path; canonical board snapshot and declared scope; task state remains on its board |

Use ordinary versioned JSON handlers and the existing service loops. No broker, plugin loader, generic remote shell, extra host supervisor or independent database server. Reuse the AgentQ queue; do not insert a second local queue in front of it. ServerQ/TaskQ status is coalesced to the latest unsent snapshot, not an event log of their authoritative state.

Task answers are a distinct capability: enrollment for observation cannot authorize answering, restarting, changing permissions or repairing a host. A later TaskQ action adapter must use its canonical choice/revision/op-ID/receipt contract and actor authorization. Do not add a command channel to complete AgentQ observation.

## C3. Identity and access

- Use the device-authorization interaction and pending/slow-down/expiry behavior from [RFC 8628](https://www.rfc-editor.org/rfc/rfc8628). The command is a public client; do not embed a shared client secret. Browser approval uses the existing authenticated identity, not a second password system or a copied GitHub token.
- Routes: browser `/connect` and connection management remain behind the current guard. Every machine route lives under one prefix `/machine/` (owner decision, #110; no unprefixed aliases): `/machine/connect/device`, `/machine/connect/token`, `/machine/connect/ack`, `/machine/ingest`, `/machine/connection/receipt`, `/machine/connection/revoke`, `/machine/operations` and `/machine/operations/result`, plus the resource exchange `/machine/resources/v1/exchange`. They use their own narrow pairing/device authorization. They must not redirect machines to browser login or expose `/data` publicly. The proxy and Cloudflare Access each exempt only `POST /machine/*` (one rule each); a coreq client reaches them with destination `https://<public_host>/machine`. Proxy changes are part of the reviewed deployment, not a blanket guard bypass.
- Bind a grant to server-side account, workspace, product, installation, runtime and allowed schemas/data scopes. Ignore/reject conflicting authority fields in payloads. A device can submit its own data and read its own receipts, never another device's records or dashboard data.
- Operation capabilities (coreq v0.11.0, D16) are separate grants: a device may request one, the approval page shows each as its own unticked effect, and the grant keeps only what the approver ticked. An observation grant carries no operation authority; queued operations take `authority` from the server, never from the payload.
- Administrator: workspace installations and membership/revocation. Member: own installations and authorized views. Apply checks to HTML, JSON, history, evidence links and exports; client-side filters are insufficient. Unmapped/expired browser identity denies access. Normal browser mutations require CSRF/origin protection.
- A verified account is not yet a workspace administrator. Deployment binds the authorized installer's verified provider subject as the initial admin through the setup flow; it never trusts a client-supplied email or an arbitrary forwarded header. Proxy identity is accepted only from the trusted boundary, with spoofed headers stripped. This mapping must be qualified before release; additional users accept an admin invitation through the UI. No unauthenticated first-visitor admin bootstrap.
- Initial DashQ setup reuses the existing guarded deployment and presents only missing server-side inputs through its setup flow; connection users never edit config. Langfuse export may remain unconfigured without blocking dashboard enrollment or resource history.
- Generate an opaque random 256-bit device credential, store only its verifier server-side, and keep it in the host's protected credential store. It is explicitly revocable, without automatic expiry. Rotation through re-enrollment replaces the credential without replacing installation identity. Owner-accepted exception to the shared bounded-token-lifetime baseline: unattended observation must continue without periodic human enrollment. This exception applies only to this installation ingestion/receipt/revoke credential; never to browser, administrator, forge, agent execution or task-action access. Revocation and scope isolation are release gates.
- A grant identifies a product installation, not a physical machine. Host names/IPs are labels. Do not merge unrelated installations, native Windows and WSL. Future grouping is an explicit owner action; adding ServerQ on the same named host does not inherit AgentQ access.
- Pairing timeout: 10 min; minimum poll interval 5 s with RFC slow-down behavior. Codes are single-use and random: 8 characters from a 32-symbol alphabet; at most 5 failed approvals per code, then invalidate it. Limit new sessions to 10/min per source IP and 1,000 pending globally, with a clear retry response; shared-NAT handling is part of qualification. Repeated token delivery after a lost response recovers the same grant until client acknowledgement/expiry; keep any required token-response secret encrypted and erase it at acknowledgement or 10 min expiry. Never issue another grant on a blind retry.

## C4. Data, acknowledgement and recovery

One DashQ SQLite file in its existing process: WAL, `synchronous=FULL` for grants, revocations and acknowledged data; one bounded writer. Store connection registry, credential verifiers, membership, latest snapshots, resource samples and forwarding outbox. Acknowledgements follow the transaction commit. Back up the store and encryption key as one protected deployment unit; qualify restoration.

Envelope: protocol major, schema version, stable event ID, producer observation time and payload. The authenticated grant supplies ownership. Replays of the same `(connection generation, event ID)` return the same outcome; reusing an ID with different content is rejected. Late observations may enter retained history but never replace a newer current value. A sample with implausible future time is marked clock-uncertain, not healthy.

Receipts distinguish rejected, durably accepted and projected. AgentQ deletes a queued record only after durable acceptance; connection qualification additionally requires projection readback. Langfuse export status is separate: an upstream 2xx is not dashboard proof, and a stored snapshot is not proof that every local component is healthy.

If the response is lost, retry the same event ID. If DashQ restarts, grants, revocations and accepted receipts survive. If the store is full/corrupt/unavailable, refuse new acceptance explicitly and preserve evidence; no fake 2xx or automatic reset. If Langfuse is unavailable, its bounded outbox retains pending exports while snapshots still render. Replayed exports retain event IDs; downstream idempotency remains an explicit qualification check.

AgentQ enrollment preserves a previously configured standalone destination until the new connection is verified. Show the proposed switch once; new events switch atomically, and old queued events keep their original destination. Do not export pre-enrollment backlog without explicit scope approval. Disconnect does not silently reactivate old credentials or destinations. Native telemetry capture/provider configuration is unchanged.

Only allowlisted metadata and producer schemas cross this connection. Do not upload raw `status.json`, prompts, tool output, paths, credentials or arbitrary OTLP attributes. Publish the AgentQ projection for routing/engine/queue fields that current installation events omit; unknown is explicit. Preserve supported native usage/IDs only where the approved metadata contract supplies them.

## C5. Operational defaults to qualify

These are accepted release targets, not measured performance or claims about existing releases.

| Concern | Required bound / behavior |
|---|---|
| Heartbeat | Immediate real snapshot on connect; then 60 s, coalesced; stale after 180 s |
| Resources | Keep AgentQ's 300 s samples and 900 s freshness; first CPU baseline is unknown; at least 3 samples for a trend |
| Connection completion | Within 60 s of browser approval on a healthy deployment; timeout names the unverified stage and continues bounded recovery |
| Delivery | HTTPS request timeout 10 s; jittered backoff from 1 s to 60 s; honor 429 Retry-After; denial/revocation requires new authorization |
| Client storage | Reuse AgentQ's 64 MiB/7-day queue; overflow count and gap visible; lifecycle/credentials never evicted with telemetry |
| Server input | 1 MiB request, at most 100 records; per-device 60 requests/min burst 10; separate pairing rate/attempt limits |
| Retention | Resource history 7 days; accepted receipt/dedup records 8 days; reject older-than-window retries explicitly, never create duplicates |
| Forwarding | Global outbox cap 1 GiB/7 days; backpressure eligible event ingestion at cap; separate snapshot acceptance stays available; no acknowledged silent loss |
| Server ceiling | Initial qualification: 100 installations on 2 vCPU/2 GiB; service ceiling 512 MiB memory, 256 descriptors, 4 GiB store including WAL/outbox |
| Saturation | Prune expired history in service; refuse new telemetry before reserve exhaustion; retain 64 MiB reserve for revocations/control records; full failure is visible |
| Performance | One-hour 100-installation hold, including recovery backlog; ingest p95 ≤500 ms, p99 ≤2 s, CPU ≤70%; separate expected backpressure from unexpected errors |

Expired forwarding data must be reported as an export gap before removal. Event timestamps never reset to retry time. The UI refreshes under D5; first-connection progress may poll its receipt at 2 s for at most 60 s. A connection can be verified while history is still collecting; their statuses must not be conflated.

## C6. Scenario checks

| ID | Scenario | Required evidence |
|---|---|---|
| E1 | AgentQ installed alone; connect from desktop or headless host | One browser approval; no key/config copying; matching snapshot in CLI receipt, `/data` and actual page |
| E2 | Cancel, deny, expire, wrong account/code, repeated click | No unintended grant; clear next action; no duplicate installation |
| E3 | Crash between approval, credential save, send and receipt | Restart resumes/reconciles; no second grant, lost acknowledged data or unbounded retry |
| E4 | DashQ/network/Langfuse outage; queue/store ceiling | Agent work continues; bounded recovery; distinct stale/unknown/export gaps and honest acknowledgements |
| E5 | Forged identity, wrong workspace, revoked credential, CSRF | No cross-installation/workspace data access or mutation; revoke survives restart |
| E6 | Old/new schemas, unavailable probes, clock jump, late samples | Compatible fields survive; unsupported data is explicit; no fresh false health or fabricated graph |
| E7 | Disconnect offline; reconnect; reinstall; same-name hosts | Pending revocation is honest; old grants stay dead; preserved versus new identities are explicit |
| E8 | ServerQ-only and TaskQ-only clients use the same connection client | Independent primary functions work; schemas differ; no AgentQ/Langfuse dependency or inherited write permission |
| E9 | Upgrade/rollback and deployment guard | Existing work continues; no public read data; backward-incompatible rollback refuses rather than losing grants/revocations |
| E10 | Normal/empty/stale/revoked UI, large text, both themes | Short local state cues, no repeated explanatory badges; accessible evidence and actions; drafts/focus survive refresh |

Chain tests cover enrollment → commit → projection → receipt and revoke → refusal → restart. Platform qualification includes credential storage, service wake/recovery and browser/headless enrollment on macOS, Linux, native Windows and WSL. A server fixture alone cannot qualify installed AgentQ or the reverse proxy. Missing evidence blocks the corresponding release claim.

## C7. Contract changes and delivery

Canonical adoption boundaries:

- DashQ D1–D3: narrow enrollment/ingestion writes, SQLite, actor authorization and machine routes; retain loopback listener and guarded browser data. D4/D6/D7: push source projection, durable expected installations and independent optional sources. D8: connection UI and scoped views.
- AgentQ §§1/5/6/8 and INSTALL: real enroll/status/disconnect workflow, protected grant, immediate snapshot, destination ownership and recovery. Replace the ambiguous GitHub-device-flow wording with approval through the actual DashQ identity provider; preserve standalone mode.
- coreq: shared client/state/storage API and two-consumer conformance fixtures; no product-specific state machine, telemetry catalog or additional process.
- ServerQ/TaskQ: subsequent adoption tasks add the same enrollment client to existing services and their own allowlisted snapshot adapters. Queue actions remain a separately authorized contract.
- specq: extract only accepted reusable semantics; do not duplicate this product's routes, limits or credential policy.

Delivery batches are implementation boundaries, not partial product releases: (1) shared connection contract/client and conformance traces; (2) DashQ authenticated enrollment/store/ingestion/projection; (3) AgentQ integration and migration; (4) full UI/platform/outage/guard qualification and release. Changes can merge behind an inactive capability; user-facing connection ships only when E1–E10 applicable to AgentQ pass. One approved task contract governs its publication and rollout, not a manual approval at each build step.

Use existing coreq #22, dashq #4/#24 and agentq #60 for ownership and split bounded implementation work under this accepted contract. Reuse resource probes/qualification #85/#86 and dashboard #23; do not create another sampler. Production access and credential changes are not authorized by this design document.

Accepted UI: [connection lifecycle L1–L4](connections.md), including the demo reference and conformance gates. This adds no protocol or authority beyond C1–C7.
