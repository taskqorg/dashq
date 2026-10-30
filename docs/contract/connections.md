# Connections detail

Linked detail of [dashq.md](../../dashq.md) D4.3 (enrolled ServerQ), D11 (Independent product connections), D16 (AgentQ operations) and D25 (Host link relay). The contract rules R1–R9 apply; this page adds lower-priority detail and never weakens them. [Enrollment C1–C7](../design/enrollment.md) and [connections L1–L4](../design/connections.md) own the accepted design.

## Enrolled ServerQ reports

- Each `serverq-report/1` snapshot (serverq.md § 3 projection, schema 1) is its own `checks.installations` entry per installation and workspace: per-rule status, reason, `observed_at`, numeric evidence, open incidents and manual rules not verified.
- It fills the hosts row `<label>/<runtime>` (`serverq.installation`; the newest observation owns a shared row) and Attention items naming `installation`.
- Freshness is producer time, with the same 7200 s limit, since ServerQ sends no heartbeat between checks.
- Another payload schema is unknown with its reason; a revoked grant shows nothing current.

## Pairing

- Routes: `POST /machine/connect/device|token|ack` and `/machine/connection/revoke` per coreq wire `coreq-enrollment/1`; guarded `GET|POST /connect` approves or denies a code.
- The browser subject header is qualified in #4. Loopback callers are host-local and trusted like `dashq report`; a proxy-only listener or shared secret is a #4 qualification item if the host has untrusted local users.
- Re-pairing a connected installation warns that approval replaces it.
- Approval needs workspace membership, the `https://<public_host>` Origin and a CSRF token bound to account and code.
- `dashq admin WORKSPACE SUBJECT`, run by the host administrator, creates `dashq.db` and `dashq.key` (0600) next to the config and binds the first administrator. Until then enrollment routes answer 503.
- The token response is sealed with `dashq.key` (AES-256-GCM) and erased at ack or expiry. The store keeps only credential and device-code SHA-256 verifiers.

## Board scope

- `/machine/connect/device` may carry `scope` `{name: "<id>/<version>", value: <single line ≤128>}` (coreq#36); malformed is `invalid_request`.
- The approval page names it under Board scope; none renders as before.
- The grant stores it; token, ack/revoke status and receipt replies name it as `scope`.
- A later token, ack, ingest or receipt request naming another scope gets 403 `scope`; revoke always succeeds.

## Ingestion

- Routes: `POST /machine/ingest` and `/machine/connection/receipt` per `coreq-enrollment/1`. Per device 60 requests/min, burst 10.
- DashQ runs coreq's conformance traces in [fixture/enrollment](../../fixture/enrollment), taken from the pinned tag with routes under `/machine/`. Its ServerQ trace carries the minimal schema 1 projection, since only a renderable payload reads `projected`.
- The grant supplies installation and workspace.
- Rejected `scope`: a schema outside the grant, or a `taskq-board/1` record whose `board` (`<kind>:<host>/<repo>`) is not the granted board scope.
- Rejected `payload`: a payload outside the metadata allowlist (below).
- Rejected `replay_mismatch`: a reused event ID with other content. Rejected `expired`: a record older than the receipt window.
- A full store answers 503, so nothing is acknowledged.

### Metadata allowlist

- Allowed: short identifier keys, numbers, booleans and short single-line strings; objects five deep, a scalar array in the fifth.
- Refused: content, command, output, path or secret fields, and path-like values.
- Recorded exception (#125): a `taskq-board/1` ask-card option's `text` at `tasks[].choices[].text`, still a short single-line string, because TaskQ's observation allowlist scopes choices as board content. `text` anywhere else or in another schema stays refused.

## Langfuse forwarding

- `agentq-event/1` has a payload of exactly `{name, metadata}`, with name one of agentq.md § 6 `agentq.installation|run|compress|retrieval|incident|resource`. It is DashQ's proposal; its handoff to AgentQ is open until AgentQ has an export-schema issue.
- `agentq-event/1` enters the bounded outbox in the acceptance transaction and never becomes a snapshot; every other schema is a snapshot.
- A payload of two keys without `metadata` is rejected `payload`, as is an export whose `observed_at` OTLP cannot express (unsigned nanoseconds: after 2554). A pass turns any such row already queued into an export gap and never overflows.
- With `langfuse.forward: true` (off by default) a separate thread posts the oldest 100 rows of the oldest row's grant every 5 s as OTLP/JSON to that project's `/api/public/otel/v1/traces`.
- Each post uses the server-side keys and `x-langfuse-ingestion-version: 4`; v4 `events_only` projects refuse the legacy `/api/public/ingestion`.
- Each row is one complete `event` span at producer time, with metadata as `langfuse.observation.metadata.*`.
- The DashQ ID `<installation>:<generation>:<event_id>` goes in `dashq.id`; `:` never occurs in either ID, so distinct events never share one. Trace and span IDs come from its SHA-256, so a retry re-sends the same span.
- A 200 confirms the request's rows and they leave.
- When its OTLP `partialSuccess.rejectedSpans` is n > 0, Langfuse does not name the lost spans. Those rows leave without a resend and become one export gap of n events in that grant's workspace over their observation window. A message with zero rejected is a warning.
- A 4xx other than 429 resends that pass's rows one by one, and each refused row becomes an export gap. Outage, 5xx or unconfirmed rows stay.
- Expiry (7 days, hourly prune) becomes a gap. A full outbox answers 503 `outbox_full` while snapshot-only batches stay accepted.
- Loopback `/data` adds `sources.export`: `unknown` unset or before a pass, `ok` after a confirmed pass, `stale` `export delayed: <reason>`, with `pending` and `gap_events`.
- Two deliveries of a span read back as one observation once Langfuse merges them, but reads can show the span twice for up to 3 min, so readers tolerate a transient duplicate.
- Langfuse documents no read-path deduplication guarantee, so the opt-in live check (`cargo test -- --ignored live`) is rerun after a Langfuse upgrade.

## Projection and installations

- Schemas that become `projected`: `agentq-snapshot/1`; `serverq-report/1` with a schema 1 ServerQ projection payload; `taskq-board/1` with a schema 1 TaskQ board observation (Connections shows its board, totals or the incomplete reason, and observed task count). Any other schema or payload stays `accepted`.
- `/data` adds `installations` `{source, rows}` and `sources.enrollment`, only once enrollment is set up.
- One row per installation and workspace from its newest grant, with `connection` `active|pending|revoked`, `observation` and `snapshot` `{event_id, schema, payload}`.
- `observation` comes from producer time: stale after 180 s (`serverq-report/1` after 7200 s); a future time is unknown (`clock uncertain`); none is `no snapshot yet`.

## Lifecycle UI

[L1–L4](../design/connections.md) own the presentation.

- D8 adds approval, a scoped installation overview and explicit revoke. First completion requires the same projection as the browser; resource history needs real samples.

- Connections detail (D10) shows per row: connection (`active`, `pending`, or `revoked` with history kept, never an active failure), data age by producer time, snapshot history counts (not resource trends), real resource warmup from D4.4, export gaps of that workspace and the current snapshot's event ID as evidence.
- Approval refreshes `/connect?wait=` every 2 s for at most 60 s. It redirects to the row (`/#inst-<installation>-<workspace>`) once the scoped projection reads that generation's snapshot, which makes its receipt `projected`.
- `Revoke…` opens a guarded confirmation that names the installation and the effect. Its POST needs the Origin and a CSRF token bound to account, installation and workspace. Only the workspace administrator or the approving member revokes.
- Refresh keeps open disclosures, focus and scroll; charts belong to D10.
- Connections separates grant activation, matching-generation projected readback, last durable receipt time and producer time.
- Export without a qualified forwarder is unknown for AgentQ and not applicable to other products.
- Add service/reconnect shows the configured HTTPS destination and the existing product command; retries re-read the same generation.

## TaskQ telemetry

- `taskq-telemetry/1` is a separate TaskQ schema the approval page lists; only a grant naming it may send it, else `scope`.
- Payload `{schema: 1, first, last, rows, bytes, gzip, status?}`. `first`/`last` are `{seq, id}` as taskq `telemetry_sync` sends them (a bare seq number stays accepted), naming the block's first and last line, whose seqs rise.
- `gzip` is the base64 gzip (≤512 KiB) of `bytes` (≤1 MiB) bytes holding `rows` JSON lines of the coreq export. A line that is not a JSON object, or wrong counts or cursors, is `payload`.
- Each line must pass the metadata allowlist. Recorded exception (#181): a line's keys may join allowlisted segments with `:` (TaskQ `method:route` request counters such as `GET:issues.N`), each segment still a short identifier outside the deny list.
- A line outside the allowlist is dropped alone (#181): the block is stored re-encoded without it, with the kept `rows`/`bytes`, `dropped` and the sent cursors, and the drop joins known loss as cause `allowlist`.
- `status` is the sender's `connection.telemetry` counters `backlog_rows`, `backlog_bytes`, `gap_rows` (numbers or null); taskq v4.6.30 sends none.
- Conformance: [fixture/taskq-telemetry](../../fixture/taskq-telemetry) holds the records taskq v4.6.30's unmodified sender posted.
- A block is stored once (C4 receipts: a replay returns the stored receipt, changed content `replay_mismatch`) in its own table, never a snapshot or export.

### Telemetry bounds

- Config `telemetry` `{retention_days 1–90 (default 7), max_mib 1–1024 (default 160)}`; `max_mib` applies per installation and workspace. The default is about twice a measured TaskQ v4.6.30 host's 7-day volume (75 MiB).
- The hourly prune drops blocks older than retention by receipt time. Every accept drops that installation's oldest blocks over its budget. A block larger than the budget is `payload`.
- Each drop adds to that installation's known loss by cause (`capacity`, `retention`, `allowlist`: blocks, rows, highest seq), which outlives the blocks.
- A full store acknowledges nothing, so the sender keeps its backlog.
- Blocks of an older store, or written by a rolled-back binary, get their seq range from their own payload on open. After a restore, rows the sender already delivered read as not received.

### Telemetry in /data

- Installation rows add `telemetry`: `off` (`telemetry not in grant`), `unknown` (`no telemetry block yet`, or every block pruned) or `ok`.
- `ok` carries `blocks`, `last_synced` (newest receipt time), `retained` `{blocks, rows, bytes, first_seq, last_seq, oldest_received, newest_received}` (receipt order) and `pruned`.
- It also carries `holes` (seq ranges between stored blocks: never received, by a sender gap or a store restore; at most 32 listed, `holes_total`, `hole_rows`) and `restarts` (a seq that goes back: the sender store was replaced).
- `sender` holds the newest block's counters, `unknown` with `status not reported by taskq` when missing; top-level `backlog_rows`, `backlog_bytes`, `gap_rows` repeat them.
- The TaskQ connection panel shows `Telemetry: last sync · backlog · gap · retained seq a–b`, then DashQ's removals and unreceived rows.
- `block=ID` (D7, also `dashq report --block=ID`) returns `telemetry_block` `{event_id, state, reason, blocks}`.
- Each listed block is a stored block of that event id within the caller's projected rows, with installation, workspace, generation, `observed_at`, `received_at`, receipt, cursors and the payload as stored, plus its installation's coverage (the same shape, including bounded metadata evidence).
- An accepted block no longer stored is `unknown` (`accepted, no longer retained`); none is `unknown`.
- The Telemetry view renders these coverage fields separately from delivery stages, including sender backlog/gap, receipt-ordered retained seq range and times, holes (at most 32 listed with total), restarts and known loss by cause.
- `telemetry.evidence` lists the newest 200 retained blocks per installation/workspace (event ID, generation, observation/receipt times, seq range and rows; no payload); `blocks` counts all.
- Each listed block links to scoped `/data?block=ID` evidence and its Connections row. No retained range proves all-event coverage.
- Analysis over block contents remains a later task. Production deploy to FRA needs owner approval.

## TaskQ resources

- `taskq-resources/1` is a separate TaskQ schema; only a grant naming it may send it, else `scope`.
- Its payload is the `taskq.resources/1` projection (metadata allowlist, else `payload`) whose every target names the sending installation and generation, else `scope`.
- The newest record per installation and workspace replaces the last whole, in its own table, never a snapshot, so the board snapshot keeps the connection's readback.
- `/data` TaskQ installation rows add `project_limits`: `off` (`resources not in grant`), `unknown` (`no resources record yet`, `no status from this host`, future status time) or the D6 state of the payload's `observed_at` (status time; stale after 600 s, two board send intervals).
- A known state carries `targets` `{project_id, runtime_id, policy_schemas, current_revision, limit (null: no override), effective_revision, effective (null: unknown, never zero), blockers {code, knowledge}, completeness}`; the TaskQ connection panel lists them.
- Contract fixture: `enrollment_record` of taskq v4.6.25 `resource-policy-fixtures`, pinned as [fixture/taskq-resources/resource-policy-v1.json](../../fixture/taskq-resources/resource-policy-v1.json).

## AgentQ operations

Source: coreq v0.11.0 [operations](https://github.com/taskqorg/coreq/blob/v0.11.0/docs/design/enrollment.md).

- `/machine/connect/device` may request `capabilities`: only AgentQ's `agentq.configure`. Malformed, duplicate or another product's is `invalid_request`.
- The approval page lists each capability as a separate unticked effect. Approving the connection alone grants none; the grant and token reply keep only the ticked subset. Observation grants stay read-only.
- Queued operations ([delivery](../design/agentq-settings.md#delivery-96)): store, `POST /machine/operations`, `/machine/operations/result`, internal typed enqueue and readback; browser review/apply is the D16 adapter.
- `authority` comes from the server only: `actor` is the verified subject, `workspace` the grant's, `profiles` every profile ID of the installation's current AgentQ snapshot when the actor is that workspace's administrator or the grant's approving member. Anyone else is refused; payload claims never add authority.
- One unresolved apply per installation. A revoke clears that block: a delivered apply without a result resolves `rejected` (`revoked`), and a new apply after reconnect is accepted.
- Read-only view: Agent integration detail (D10) renders each active AgentQ installation's snapshot `configuration` ([fixture/agentq](../../fixture/agentq), agentq ac859d3) per profile.
- Per profile it shows routing, MCP/hooks or plugin, preference, effective adapter state, aliases, overrides, conflicts and the projection's write-disabled reasons. No control, task or saving.

## Host link relay

- Send: `to` and `id` are short identifiers; `body` is standard padded base64 of at most 256 KiB decoded, else 413 `too_large`. One pending message per (from, to, id); a resend replaces it.
- Receive returns `{messages: [{from, id, body}]}` addressed to the caller, each taken once, waiting up to `wait` s (default and most 25, inside Cloudflare's 100 s); empty on timeout.
- A newer receive of the same installation ends the older wait empty, so one host holds one thread.
- Messages expire 60 s after send. Pending bytes per installation, addressed to it and sent by it each, stay at most 4 MiB, else 429 `cap`.
- A receiver revoked meanwhile gets nothing. Messages from a sender no longer connected in that workspace are dropped at delivery.
