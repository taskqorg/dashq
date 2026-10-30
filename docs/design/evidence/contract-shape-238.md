# Contract shape: decision inventory (#238)

Inventory of every rule or decision in `dashq.md` on main at a360de9 (before the rewrite). One line per rule or decision, keyed by its section. Each line ends with its new place after the rewrite (`→`). `dashq.md` keeps the rules; `process.md`, `connections.md` and `views.md` are the linked detail files under `docs/contract/`. No decision is changed, dropped or weakened; history, dates, owner/task attributions and evidence narratives were removed.

## D1. Scope
- I001 D1: D11 narrowly replaces no-write, no-push and no-database for enrollment, ingestion and connection management; it authorizes no host commands. → dashq.md R3 (sub-bullet), R4
- I002 D1: D13 authorizes gated task controls and the opt-in PM request exchange; enrollment grants no control authority. → dashq.md R3, R4
- I003 D1: D25 adds the blind host link relay, transport only. → dashq.md R3
- I004 D1: Read-only: never writes to Langfuse, GitHub, serverq or any host; no state-changing buttons. → dashq.md R3
- I005 D1: No new auth system; the page sits behind the Langfuse UI forward-auth guard. → dashq.md R5
- I006 D1: No push channel from hosts; dashq only pulls D4 sources; host push is a later task (#4). → dashq.md R8
- I007 D1: No database: in-memory caches; a restart rebuilds everything from the sources. → dashq.md R9
- I008 D1: Metadata only: never conversation text, prompts, tool input/output, raw commands, stderr/stdout or stack traces. → dashq.md R1

## D2. Process
- I009 D2: One Rust binary `dashq` on the shared `taskqorg/coreq` crate. → dashq.md D2
- I010 D2: `serve` binds `127.0.0.1:18882` only; not configurable to a non-loopback address. → dashq.md R6
- I011 D2: `dashq report [--period] [--host] [--agent] [--session] [--cursor]` prints the `/data` JSON once. → dashq.md D2
- I012 D2: `dashq admin WORKSPACE SUBJECT` binds an enrollment administrator; `dashq profile [--window=S]` prints the profile report, default 86400 s. → dashq.md D2
- I013 D2: `selftest` loads config, renders the fixture snapshot, reads each source once; no writes. → dashq.md D2
- I014 D2: Host requirements: curl ≥ 7.20.2 always; taskq ≥ 4.6.32 only with `taskq.binary` (version from its coreq layout); Chrome ≥ 59 for `--snapshot` only (skipped when absent, unchecked on Windows). → process.md § Host requirements
- I015 D2: OS service tools have no minimum; no system library (static builds). → process.md § Host requirements
- I016 D2: `dashq doctor` prints one line per requirement, exits 1 naming missing/older tools with found, minimum and fix; serve/service run/install/update log `requirement:` lines. → process.md § Host requirements
- I017 D2: dashq installs no OS packages; a failed curl, taskq or Chrome run logs its command (never stdin or `--text`) and first stderr line (≤200 chars). → process.md § Host requirements
- I018 D2: `--snapshot DIR` writes `page.html` (inlined CSS, no script) and `page.png` via headless Chrome 1280 px; renderer lookup order; no Chrome fails naming the renderer after writing HTML; no config, no source reads. → process.md § Snapshot command
- I019 D2: Baseline routes `GET /`, `/app.js`, `/style.css`, `/data`; other paths 404, other methods 405. → dashq.md D2
- I020 D2: Security headers on every response (no-store, nosniff, same-origin referrer, CSP); no third-party assets. → dashq.md D2
- I021 D2: Config file from `DASHQ_CONFIG` else `<install>/config.json`, mode 0600, with its listed keys. → dashq.md D2 (file, mode, invalid config); process.md § Config (keys)
- I022 D2: Requests whose `Host` is not loopback or the public name get 421. → dashq.md D2
- I023 D2: Secrets never reach the browser, `/data`, logs or `report`. → dashq.md R7
- I024 D2: Invalid config: refuse to start naming the key, never the value; selftest without config skips source reads and says so. → dashq.md D2
- I025 D2: `service install|uninstall|run` and `update [--check]` are coreq commands; `service run` is `serve` and marks healthy after 60 s. → dashq.md D2
- I026 D2: An update that makes a new release current restarts an installed unit; `--check`, unchanged or failed updates leave it running; failed restart exits non-zero naming the error. → process.md § Update and rollback
- I027 D2: Automatic update: installed `service run` checks 900 s after start then every 3600 s; `DASHQ_UPDATE_EVERY` overrides; first check waits at most 900 s. → process.md § Update and rollback
- I028 D2: A newer release is applied by coreq update; the service logs it and exits 75 so the unit or launcher starts the new `current`. → process.md § Update and rollback
- I029 D2: One service-log line per check/apply with the listed texts; a failed check or apply keeps the running release. → process.md § Update and rollback
- I030 D2: Profile ops `update.check` and `update`. → process.md § Update and rollback
- I031 D2: Rollback on failed start: coreq restores `previous` at the third start without `healthy`; the service writes `<install>/rolled-back` and never auto-applies that version again; newer releases proceed; manual update still installs it. → process.md § Update and rollback
- I032 D2: `serve` and `service run` outside the install never update themselves. → process.md § Update and rollback
- I033 D2: Release CI on a `v<ver>` tag equal to `Cargo.toml` builds static linux x86_64/aarch64, `SHA256SUMS`, `latest.json`. → process.md § Release
- I034 D2: Each artifact passes offline `selftest` before publishing; a failure publishes nothing. → process.md § Release
- I035 D2: Every PR runs the same builds and selftests without publishing; `main` requires them with `tests`. → process.md § Release
- I036 D2: Only owners push tags; installed services update themselves, other hosts with `dashq update`; rollback is coreq `previous`. → dashq.md D2 (owners push tags); process.md § Release

## D3. Deployment
- I037 D3: Runs on the production host (FRA) beside Langfuse and the serverq report. → dashq.md D3
- I038 D3: Served under `dash.` of the Langfuse UI domain; the proxy runs the Langfuse `forward_auth` guard first, then proxies to `127.0.0.1:18882`. → dashq.md D3
- I039 D3: No public port, firewall change, public listener or `/api/public` route; browser data including `/data` always guarded. → dashq.md D3, R5, R6
- I040 D3: D11 machine routes use their own narrow authorization; no wildcard exemption. → dashq.md R5
- I041 D3: Rollback: remove the `dash.` site block, stop the service. → dashq.md D3
- I042 D3: dashq supersedes the agentq `/serviceq` panel; its removal is agentq's task. → dashq.md D3

## D4. Sources
- I043 D4: Each source has its own cache, single flight and a 10 s timeout per HTTP call. → dashq.md D4
- I044 D4.1: Langfuse public API with project keys, server side; model activity is every `GENERATION` observation. → dashq.md D4.1
- I045 D4.1: AgentQ event spans and their attributes; Agents view sources for fleet, versions, last seen, health, usage and incidents. → dashq.md D4.1
- I046 D4.1: Last seen is the newest event time; no `agentq.installation` heartbeat in 7200 s is stale. → dashq.md D4.1
- I047 D4.1: Periods 1800/7200/86400 s, default 1800; health and incident history read 24 h. → dashq.md D4.1
- I048 D4.1: Client use reads `agentq.compress` and `agentq.retrieval` over 24 h; `agentq.installation` read takes the `io` field group. → views.md § Langfuse reads
- I049 D4.1: Cursor pagination, at most 10,000 rows per event name and period; the cap marks the source `incomplete`. → dashq.md D4.1
- I050 D4.1: Cache 30 s per period. → dashq.md D4.1
- I051 D4.2: Projects are an explicit config list; repositories are never discovered. → dashq.md D4.2
- I052 D4.2: Boards: open `taskq` issues, one page of 100; state from `q-<state>`, priority `p0|p1|p2`; over 100 is `incomplete`; no comment folding. → dashq.md D4.2
- I053 D4.2: Heartbeats from `taskq-host` issues; age = response `Date` minus `updated_at`, never the local clock. → dashq.md D4.2
- I054 D4.2: Latest releases per project repo, cached 10 min. → dashq.md D4.2
- I055 D4.2: Cache 60 s per repo; a 403/429 keeps the last value unconfirmed until reset. → dashq.md D4.2
- I056 D4.2: Snapshot adapter with `taskq.binary`: isolated scratch run of `taskq status --json` (read-only token install, no HOME, incident reports off, folder removed); single flight, 10 s, cache 60 s. → dashq.md D4.2 (command, single flight, cache); process.md § Snapshot adapter run
- I057 D4.2: Snapshot totals, questions and failure reasons replace label counts; heartbeats stay label reads; a failed/incomplete/unknown-schema run is a failed read listed in `sources.taskq`; absent key means labels only. → dashq.md D4.2
- I058 D4.3: Local serverq `report.json` read every refresh; last-good kept with its producer time, unconfirmed until a good read; absent config is unconfigured, distinct from missing report. → dashq.md D4.3
- I059 D4.3: Absolute config paths and fixtures use the target platform's path rules. → dashq.md D4.3
- I060 D4.3: Accepted schema 1 shape; another schema or invalid JSON is unknown with reason. → dashq.md D4.3
- I061 D4.3: Stale after 7200 s; manual rules are not verified, never passed. → dashq.md D4.3
- I062 D4.3: Enrolled `serverq-report/1` snapshots: one `checks.installations` entry per installation and workspace, hosts row, Attention items; producer-time freshness 7200 s; other payload schema unknown; revoked grant shows nothing current. → connections.md § Enrolled ServerQ reports (linked from D4.3)
- I063 D4.4: Producer contract `agentq.resource`; enrolled history first, Langfuse (with `io`) for the rest; each row names `provenance`. → dashq.md D4.4
- I064 D4.4: Window = period; raw samples; at most 2016 per installation, newest kept, a cut is `incomplete`; Langfuse keeps D4.1 pagination. → dashq.md D4.4
- I065 D4.4: Per-sample fields; missing/invalid/inconsistent value is `null` with reason; nothing derived. → dashq.md D4.4 (null rule); views.md § Resource samples (fields)
- I066 D4.4: Current = newest, stale after 900 s; gaps, resets and warmup below three samples; a non-ok row joins Attention gaps. → dashq.md D4.4

## D5. Refresh
- I067 D5: Redesign target is specq W4–W6 and D10; the mechanism is the baseline, not proof of stable updates. → dashq.md D5
- I068 D5: Fetch `/data` on load, every 60 s and on Refresh; no live push. → dashq.md D5
- I069 D5: Server renders the shell; D13 boards use the pinned TaskQ renderer; sections reload from `/` with the same query after a good fetch. → dashq.md D5
- I070 D5: Show `Updated <time>` and each source age; a failed fetch keeps the page with the banner, never blanks values. → dashq.md D5

## D6. Unknown and stale
- I071 D6: Every value carries its source state; missing data never becomes zero or healthy. → dashq.md R2
- I072 D6: Source states ok/stale/unconfirmed/unknown/incomplete, each non-ok with a one-line reason. → dashq.md D6
- I073 D6: Freshness limits: Langfuse 600 s; heartbeat `ttl` (300 s); serverq 7200 s; GitHub 180 s. → dashq.md D6
- I074 D6: Green only for ok and healthy; others gray/amber with reason and age; missing number `—` with reason; capped `≥N (capped)`. → dashq.md D6
- I075 D6: Counters that reset with a restart say so; a reset is not a drop. → dashq.md D6
- I076 D6: Historical incidents deduplicated, never current failures. → dashq.md D6
- I077 D6: A summary bar lists every non-ok source with its reason (placement amended by D10). → dashq.md D6
- I078 D6: `/data` sections carry `source: {state, at, age_s, reason, incomplete}`; unknown numbers are `null`. → dashq.md D6

## D7. /data JSON
- I079 D7: Query parameters, `block` and `cursor`; invalid 400; all sources failed 503; partial 200. → dashq.md D7
- I080 D7: Top-level keys, `schema` 2, `resources` shape. → views.md § Data document
- I081 D7: Session filters apply to model activity only. → dashq.md D7
- I082 D7: Aggregates and evidence trace IDs only, never prompts, commands, tool results, tokens or keys. → dashq.md R1
- I083 D7: Additive changes within `schema` 2; breaking change raises it; one declared removal (`session.line`). → dashq.md D7 (rule); views.md § Data document (declared exceptions)
- I084 D7: Agents read it with `dashq report` or through the guard. → dashq.md D7

## D8. Page
- I085 D8: The accepted operator redesign updates D8/D10; host-first baseline: hosts first, tasks through shared TaskQ UI, other information on demand. → dashq.md D8 intro
- I086 D8: Design package records approval; production qualification pending; English; D1–D7 binding except the D10 source-summary amendment. → dashq.md D8 intro
- I087 D8.1: Minimal Overview: one machine/kind per group, identity → state/age → resources with trends → detail/action; missing resources one unavailable state; partial metrics keep reasons. → dashq.md D8.1
- I088 D8.1: Versions/update, login probes, queue counts and receipts in host detail and D22 views; each value keeps source/time. → dashq.md D8.1
- I089 D8.1: `hosts[].taskq.observed_running` nullable; `boards[].attempts` adds `host_id` and `current`, not pending assignment. → views.md § Host observations
- I090 D8.1: Silence means unknown; a heartbeat never proves login or service health. → dashq.md D8.1
- I091 D8.1: `taskq-board/1` observations supply online, login and assigned queue; `hosts[].taskq.observations` fields. → views.md § Host observations
- I092 D8.1: Latest per board/workspace/host/kind wins; equal-time conflicts unknown; missing/expired/future/invalid/partial evidence unknown with TaskQ's reason; no inference from activity. → views.md § Host observations
- I093 D8.1: H2 machine identity joins labels; absent identity keeps its own row; shared queues unassigned, never multiplied. → dashq.md D8.1 (shared queues); views.md § Host observations (identity join)
- I094 D8.2: Attention only for conditions needing a person and an action, with the four mappings; names host/task, reason, age, action. → dashq.md D8.2
- I095 D8.2: Gaps, revoked connections and incident history are detail; retries and waits are not requests; non-ok meaning stays beside values; a source outage needing intervention may appear. → dashq.md D8.2
- I096 D8.3: Tasks: host count opens its tasks; direct entry has all projects and unassigned work; shared TaskQ rows and controls; retained fields; missing release is an explicit gap. → dashq.md D8.3
- I097 D8.4: `/agents` starts with attempts and an inspector; selection/focus/return survive; client activity, D21, D17, D24 separate, never merged. → dashq.md D8.4
- I098 D8.4: Per-model sample counts separate; cost null unless reported; client observation rules; login separate. → dashq.md D8.4
- I099 D8.5: Server checks keep per-rule detail, incidents, linked work, evidence, manual rules not verified; a closed task never establishes recovery. → dashq.md D8.5

## D9. ServiceQ views
- I100 D9: Preservation ledger maps every section and prior ServiceQ capability; retired Node-monitor features stay retired. → dashq.md D9
- I101 D9: D7 report/JSON, D4 periods/caches/caps and source evidence remain available independent of page placement. → dashq.md D9

## D10. Dashboard redesign target
- I102 D10: The accepted operator redesign package owns the target and ledger; runtime authority and qualification gates remain. → dashq.md D10
- I103 D10: Ownership of composition, resources, boards and connections pages; W1–W15 apply. → dashq.md D10
- I104 D10: Live producer qualification remains a gap; full TaskQ workspace adopted. → dashq.md D10
- I105 D10: Overview cards across the width, one column when narrow without smaller type; detail expandable; Attention is the D23 request list after hosts with header shortcut/count; no side-by-side boards; expected hosts stay visible. → dashq.md D10
- I106 D10: Source status collapsed with count and reasons; stale/superseded/revoked records hidden by default with filters; never hides a current warning; D11/D16 overview sections in detail; no empty panels. → dashq.md D10
- I107 D10: Resource bars and 30-minute 0–100% trends from D4.4 samples; three samples; gaps/resets break lines; states explicit; detailed history in host detail. → views.md § Overview resources
- I108 D10: Thresholds producer/policy-owned; no demo threshold becomes policy. → dashq.md D10
- I109 D10: Browser fidelity check pins the approval hash and compares geometry at the listed sizes; fixture evidence never qualifies production. → views.md § Overview resources
- I110 D10: 60 s refresh, 1.3 s tint (none under reduced motion); preserve rows, selection, focus, drafts, disclosures, scroll; never reorder hosts; freshness is producer time. → dashq.md D10
- I111 D10: TaskQ interiors only from published releases pinned by version and hashes; the three pinned asset sets; action gates separate. → dashq.md D10 (rule); views.md § Pinned TaskQ assets
- I112 D10: Target decisions: expenses and estimates separate; telemetry events/metrics only, worker-line content retired without retrospective deletion; PM request with native-session continuation through TaskQ UI; nothing enabled by the target. → views.md § Redesign target decisions; dashq.md D10 intro (nothing enabled)
- I113 D10: Acceptance: #169 A1–A5 and review package; prototypes prove layout only; no deployment, sign-in, host command, restart/update button or new mutation authority. → dashq.md D10

## D12. Security
- I114 D12: Follows the specq security baseline; reports via `SECURITY.md`. → dashq.md D12
- I115 D12: Every workflow `permissions: contents: read`; publishing job adds only `contents: write`. → dashq.md D12
- I116 D12: Repository is public; shared baseline applies. → dashq.md D12

## D11. Independent product connections
- I117 D11: Enrollment C1–C7 and connections L1–L4 own the contract and presentation. → dashq.md D11
- I118 D11: One process and one SQLite store (WAL/FULL); no extra daemon or database server. → dashq.md D11
- I119 D11: C3 routes/authority, C4 acceptance/replay, C5 limits; CSP only for reviewed approval forms; loopback listener; no read APIs via machine routes. → dashq.md D11
- I120 D11: Registry supplies expected installations; optional sources never block enrollment or hide snapshots; no duplicate identities by hostname. → dashq.md D11
- I121 D11: Actor scope applied server-side everywhere; local report is a host-administrator capability; producer-time freshness; connection and export health separate. → dashq.md D11
- I122 D11: Approval, scoped overview, explicit revoke; first completion needs the browser's projection; observation grants give no answer or command authority. → connections.md § Lifecycle UI; dashq.md R4
- I123 D11: Pairing routes per `coreq-enrollment/1`; guarded `/connect`. → connections.md § Pairing
- I124 D11: Browser subject only from proxy-set `X-Dashq-Subject`; loopback callers trusted like `report`; proxy-only listener or secret is a #4 item. → dashq.md D11 (subject header); connections.md § Pairing (loopback, #4)
- I125 D11: Re-pairing warns; approval needs membership, Origin and a CSRF token bound to account and code. → connections.md § Pairing
- I126 D11: `dashq admin` creates `dashq.db`/`dashq.key` (0600); until then enrollment answers 503; sealed token erased at ack/expiry; store keeps only SHA-256 verifiers. → connections.md § Pairing
- I127 D11: Board scope on device request; malformed `invalid_request`; shown on approval; stored and named in replies; other scope 403 `scope`; revoke always succeeds. → connections.md § Board scope
- I128 D11: Ingestion routes; conformance traces; grant supplies installation/workspace. → connections.md § Ingestion
- I129 D11: Rejections `scope` (schema or board), `payload` (metadata allowlist), `replay_mismatch`, `expired`; full store 503. → connections.md § Ingestion
- I130 D11: Metadata allowlist definition and the #125 `choices[].text` exception. → connections.md § Metadata allowlist
- I131 D11: Per device 60 requests/min, burst 10. → connections.md § Ingestion
- I132 D11: `agentq-event/1` shape enters the outbox, never a snapshot; other schemas are snapshots; AgentQ handoff open. → connections.md § Langfuse forwarding
- I133 D11: Forwarding (`langfuse.forward: true`): batching, OTLP endpoint, header, span mapping, DashQ ID and deterministic IDs. → connections.md § Langfuse forwarding
- I134 D11: Forwarding outcomes: 200 confirms; rejected spans become an export gap; 4xx one-by-one resend; outage/5xx stay; 7-day expiry gap; full outbox 503 `outbox_full`. → connections.md § Langfuse forwarding
- I135 D11: `sources.export` states on loopback `/data`. → connections.md § Langfuse forwarding
- I136 D11: Two-key payload without `metadata` and inexpressible `observed_at` rejected `payload`; queued such rows become gaps. → connections.md § Langfuse forwarding
- I137 D11: Readers tolerate a transient duplicate; the opt-in live check is rerun after a Langfuse upgrade; forwarding off by default. → connections.md § Langfuse forwarding; dashq.md D11 (off by default)
- I138 D11: Data rows keep their grant's workspace; a new approval after revoke may pick another workspace, which starts empty. → dashq.md D11
- I139 D11: One projection function serves `/data`, `report` and a 2 s pass; `projected` only when read as current and rendered for the three schemas; others stay `accepted`. → dashq.md D11 (rule); connections.md § Projection and installations (schemas)
- I140 D11: `/data` `installations` and `sources.enrollment` row fields and freshness (180 s, serverq 7200 s, future unknown). → connections.md § Projection and installations
- I141 D11: Scope: loopback and report see all; public requests see the subject's workspaces; none without a subject; optional source failure never hides snapshots or makes 503. → dashq.md D11
- I142 D11: Connections detail fields per row; revoked never an active failure. → connections.md § Lifecycle UI
- I143 D11: Approval wait refresh (2 s, 60 s) and redirect once projected. → connections.md § Lifecycle UI
- I144 D11: Revoke confirmation, Origin/CSRF bound to account/installation/workspace; only administrator or approving member revokes. → connections.md § Lifecycle UI
- I145 D11: Refresh keeps disclosures/focus/scroll; report store read needs 0600 config and key. → connections.md § Lifecycle UI; dashq.md D11 (report capability)
- I146 D11: Connections separates activation, readback, receipt time, producer time; no receipt = contact unknown; export unknown for AgentQ without forwarder, n/a otherwise; reconnect shows destination and command; retries same generation. → dashq.md D11 (contact unknown); connections.md § Lifecycle UI
- I147 D11: `taskq-telemetry/1` needs a grant naming it; payload shape, limits and cursor rules; malformed is `payload`. → connections.md § TaskQ telemetry
- I148 D11: Telemetry lines pass the allowlist with the #181 `:`-segment exception; a disallowed line is dropped alone, block re-encoded, loss cause `allowlist`. → connections.md § TaskQ telemetry
- I149 D11: `status` counters; conformance fixture; a block stored once with C4 receipts in its own table. → connections.md § TaskQ telemetry
- I150 D11: Telemetry bounds config defaults and ranges; prune by retention; budget drops; oversize block `payload`; known loss by cause outlives blocks; full store acknowledges nothing; older/rolled-back blocks get seq from payload; restored rows read not received. → connections.md § Telemetry bounds
- I151 D11: `/data` installation `telemetry` states and fields; connection panel line. → connections.md § Telemetry in /data
- I152 D11: `block=ID` returns `telemetry_block`; no-longer-stored is `unknown`. → connections.md § Telemetry in /data
- I153 D11: Telemetry view renders coverage fields; `telemetry.evidence` newest 200 blocks; links; no range proves all-event coverage; content analysis later; FRA deploy needs owner approval. → connections.md § Telemetry in /data
- I154 D11: `taskq-resources/1` grant, payload, target identity, newest-replaces storage; `project_limits` states and fields; fixture. → connections.md § TaskQ resources
- I155 D11: Security exception: non-expiring, revocable, per-installation, observation-only ingestion credential. → dashq.md D12 (recorded exception)

## D13. TaskQ answer card
- I156 D13: TaskQ owns one card; DashQ pins assets and supplies the scoped adapter; amends D1 with guarded `POST /answer`; enrollment has no action authority. → dashq.md D13 § TaskQ answer card, R3, R4
- I157 D13: Canonical dispatcher, expected revision, op ID, verified receipt; consequential operations excluded. → dashq.md D13 § TaskQ answer card
- I158 D13: Route off until `answer` configured; production writes wait for live qualification; capability via `GET /answer` per board. → dashq.md D13 § TaskQ answer card
- I159 D13: Tasks owns one card per board/task; Attention navigates; canonical question fields kept. → dashq.md D13 § TaskQ answer card
- I160 D13: Production `answer` is configured on FRA for `taskqorg/dashq` only; other boards read-only. → dashq.md D13 § TaskQ answer card
- I161 D13: Session progress `session {host, runtime, stage, age_s}` on doing/review rows; no `codex://` off host. → views.md § Task rows
- I162 D13: Worker line text never reaches views; `line` removed; ingestion strips it; stored ones age out, never projected; nothing deleted; the full log never leaves its host. → dashq.md D13 § TaskQ answer card
- I163 D13: Static snapshots keep issue links and a component-unavailable reason. → views.md § Task rows
- I164 D13: Task controls route, off unless config and `taskq.binary`; boards must be configured projects. → dashq.md D13 § Task controls
- I165 D13: `dashq controller` grant; actor must be workspace administrator with the grant; other grants give no control authority. → dashq.md D13 § Task controls, R4
- I166 D13: `controls` list is canonical, never inferred; TaskQ v4.6.30 owns renderer, commands and fencing; Close is a link. → dashq.md D13 § Task controls
- I167 D13: Actor, fresh row/revision and qualified adapter gate dispatch; Origin/CSRF, journal, same-ID retry; unknown delivery never accepted or fresh. → dashq.md D13 § Task controls
- I168 D13: Production `task_controls` configured on FRA for the six configured boards. → dashq.md D13 § Task controls
- I169 D13: PM request routes using `taskq.pm/1`; `pm_requests` off by default; separate `pm-requester` grant. → dashq.md D13 § PM requests
- I170 D13: Actor is SHA-256 of the subject; TaskQ must advertise a matching host PM grant and revision; Origin/CSRF and rate limits. → dashq.md D13 § PM requests
- I171 D13: Bounded temporary outbox keeps exact input until terminal receipt or expiry; digest tombstones forbid reuse. → dashq.md D13 § PM requests
- I172 D13: No conversation, reply bridge, task admission or host-command authority; receipts never contain request text; `/data`/report contain no PM text. → dashq.md D13 § PM requests
- I173 D13: Drafts and uncertain requests stay in the page session; only the same request reconciles. → dashq.md D13 § PM requests
- I174 D13: Production `pm_requests` is on. → dashq.md D13 § PM requests

## D14. TaskQ resource controls
- I175 D14: Reuses TaskQ components; separately authorized typed host-policy capability; no `/answer` or observation authority; TaskQ owns limits and readback; no remote shell or second scheduler. → dashq.md D14
- I176 D14: Operations stay off; `resources` set only after taskq#931 ships and live qualification passes. → dashq.md D14
- I177 D14: Project limits: read `taskq-resources/1`, offer `taskq.project-limits/1` under a `project-limits` grant on pinned v4.6.25 assets; live qualification is #158. → dashq.md D14

## D15. ServerQ finding handoff
- I178 D15: Authorized TaskQ create/optional-start adapter and composer; no authority for observation or `/answer`; ownership split. → dashq.md D15
- I179 D15: A closed task alone never removes a problem from Attention. → dashq.md D15
- I180 D15: Finding identity workspace/installation/rule/first_seen; no inferred resource join; H4/H7 grouping unchanged. → dashq.md D15
- I181 D15: Episodes from retained samples; only a complete fresh matching passing rule with ordered `verified_at` resolves; missing/capped history explicit. → views.md § ServerQ findings
- I182 D15: Vendored catalog only on SHA-256 match, else thresholds and recommendations unavailable; creation pending. → views.md § ServerQ findings

## D16. AgentQ integration settings
- I183 D16: Separately authorized typed AgentQ config adapter; AgentQ owns plans and verification; no observation or `/answer` authority; release gated. → dashq.md D16
- I184 D16: `agentq_configure` (default off) enables `/agentq/configure`; off until AgentQ A5 qualification. → dashq.md D16
- I185 D16: Device `capabilities` (only `agentq.configure`); malformed `invalid_request`; separate unticked effects; grant keeps only ticked ones; observation grants read-only. → dashq.md D16 (no capability by approval); connections.md § AgentQ operations
- I186 D16: Operation routes; authority only from the server (actor, workspace, profiles); others refused; payload never adds authority. → dashq.md D16 (server-only authority); connections.md § AgentQ operations
- I187 D16: One unresolved apply per installation; revoke resolves delivered apply `rejected` (`revoked`); new apply after reconnect accepted. → connections.md § AgentQ operations
- I188 D16: Read-only Agent integration view per profile; missing/older projection unknown; stale never green; configured never verified; no control. → dashq.md D16 (read-only rules); connections.md § AgentQ operations (view fields)

## D17. AgentQ optimization results
- I189 D17: Read-only results with AgentQ accounting; no config authority, optimizer or experiment runner. → dashq.md D17
- I190 D17: `/data.optimization` through `agentq.accounting/1` with 29 conformance cases. → views.md § Optimization results
- I191 D17: Empty reads unknown, counts partial, buckets never invent samples; whole-task effect unknown without experiment source. → dashq.md D17
- I192 D17: Inherits filters; disclosures/focus/selection survive refresh; only the macOS run adapter is live-qualified. → views.md § Optimization results

## D18. Service versions and diagnostics
- I193 D18: Distinguish available, selected, running, verified health; lifecycle owned by coreq and consumers; allowlisted diagnostics; no remote update/restart/rollback. → dashq.md D18
- I194 D18: `versions` rows from the listed sources and their identity rules. → views.md § Service versions
- I195 D18: Row fields and `update` only from a fresh verified comparison. → dashq.md D18 (update, last_known); views.md § Service versions (fields)
- I196 D18: `lifecycle`/`health` from AgentQ `release` with the verification conditions; others report neither. → views.md § Service versions
- I197 D18: Producer timestamps; stale or not running keeps `last_known`; unconfirmed never verified; host filter before `mixed`. → dashq.md D18 (update, last_known); views.md § Service versions (fields)
- I198 D18: Producer gaps listed; cases fixture. → views.md § Service versions
- I199 D18: Services view: one selected detail, separate times, explicit gaps; state survives refresh; no lifecycle action. → views.md § Service versions
- I200 D18: Versions are `MAJOR.MINOR.PATCH[-pre]`, other forms unknown; other enrolled fields and Langfuse adapters await authorization. → dashq.md D18 (version form); views.md § Service versions (awaiting authorization)

## D19. First launch and partial availability
- I201 D19: Startup U1–U4 compose existing rules without new authority; no empty panels; last-known values and drafts kept; per-source action gating. → dashq.md D19
- I202 D19: First-launch entry continues to `/connect`; optional inputs listed once; TaskQ-only has no empty panels. → views.md § First launch and outages
- I203 D19: Failed boards keep historic counts beside unknown counters; failed refresh keeps page and components, marks unconfirmed, ages advance; recovery clears banner without writes. → dashq.md D19 (failed refresh); views.md § First launch and outages (failed boards)
- I204 D19: ServerQ keeps last-good; Langfuse resources not fresh after failure; trends need three samples; shared adapters read-only until qualification. → dashq.md D19 (adapters read-only); views.md § First launch and outages
- I205 D19: Config validation fixtures use native absolute paths. → views.md § First launch and outages

## D20. Profiling
- I206 D20: Applies specq built-in profiling. → dashq.md D20 (rule); process.md § Profiling (detail)
- I207 D20: Operations with p95 and error budgets; times include writing; cache hits are not reads; 5xx failure, 4xx caller; assets and 404 unmeasured. → process.md § Profiling
- I208 D20: Counters always on in serve, service run, report, update; record shape; error class rule; no secrets/content/URLs/paths; selftest and tests write nothing. → dashq.md D20 (rule); process.md § Profiling (detail)
- I209 D20: Store `profile.jsonl` 0600, 4 MiB prune to newest half; failed writes never fail the operation. → dashq.md D20 (rule); process.md § Profiling (detail)
- I210 D20: `DASHQ_TRACE=1` spans and step lines. → process.md § Profiling
- I211 D20: Report JSON, ranking, unknown and incomplete rules; never zero or ok. → dashq.md D20 (rule); process.md § Profiling (detail)

## D21. Agent work
- I212 D21: Source: telemetry `stage` observations primary, local `taskq.log` fallback; cutover dedup; only valid codes; no row without either. → dashq.md D21 (sources, codes); views.md § Agent work › Reading
- I213 D21: Period by end time; host filter; `/data.agent_work` shape, rows, stages, usage rules; `tokens` never added to another count; `time_ms` meaning. → dashq.md D21 (tokens); views.md § Agent work › Rows
- I214 D21: Attempts: newest 200 with fields; link only for configured projects. → views.md § Agent work › Attempts
- I215 D21: Elapsed/execution only for complete coverage; states and reasons otherwise; open markers never samples. → dashq.md D21 (complete coverage); views.md § Agent work › Attempts
- I216 D21: `queue_wait_s` from the ready transition, else null with reason. → views.md § Agent work › Attempts
- I217 D21: `output_tokens`, `models`; `estimate` null; multi-host attempt is one row. → views.md § Agent work › Attempts
- I218 D21: Unknown/stale/incomplete rules; never zero or ok; budgets unknown. → dashq.md D21 (unknown rules); views.md § Agent work › Unknown and incomplete
- I219 D21: Page: attempts first when configured; inspector; additive timeline fields; overlaps never become complete or idle; progress unknown without evidence; admission is current evidence only; totals separate. → dashq.md D21 (page, overlap rule); views.md § Agent work › Page and inspector

## D22. Shareable views
- I220 D22: Applies specq F7; `/` overview; listed guarded page paths; unknown 404 with recovery; unavailable views say so. → dashq.md D22
- I221 D22: Shareable state is path plus D7 filters; local state stays local and survives navigation; reload restores view and filters, not drafts; secrets never in URLs. → dashq.md D22
- I222 D22: `/reports` reads only D21/D23/D24; `agent_work.totals` definition and rules; same values in report; filters scope; shows coverage; links to `/agents`. → dashq.md D22 (sources); views.md § Reports

## D23. Waiting work, changes and route
- I223 D23: Read-only; one projection serves `/data`, page and report. → dashq.md D23 intro
- I224 D23: Admission from snapshot or board; pinned vocabulary; unknown rules; never infers a stall; Tasks lists waiting work. → dashq.md D23 (admission rule); views.md § Waiting work › Admission
- I225 D23: Changes shape; new/ongoing/resolved; unclassified; coverage per source; comparison state never "nothing changed". → dashq.md D23 (classification); views.md § Waiting work › Changes
- I226 D23: Route six stages with known/unknown/n/a; sources; Telemetry view rendering; known false not passed; unknown never passed. → dashq.md D23 (pass rule); views.md § Waiting work › Route
- I227 D23: `attention.requests` only producer requests needing a person; three kinds with targets; Attention renders exactly these. → dashq.md D23 (requests rule); views.md § Waiting work › Requests
- I228 D23: Query shape; cursor paging by 200; UTC. → views.md § Waiting work › Query

## D24. Model use and estimates
- I229 D24: Read-only; no billing source, credential or authority. → dashq.md D24
- I230 D24: Price table config shape and validation; entry effective on or before the call day. → dashq.md D24
- I231 D24: `model_use` shape; estimate preconditions and reasons; currencies separate; incomplete only on capped read. → dashq.md D24 (estimate rule); views.md § Model use (shape, reasons)
- I232 D24: Runtimes and their unknown reasons; TaskQ stage tokens never priced. → dashq.md D24 (TaskQ tokens); views.md § Model use (runtimes)
- I233 D24: Estimates labelled `API-equivalent estimate`; report prints the same. → dashq.md D24
- I234 D24: Reports displays estimates with coverage; retained range unknown; no expense or TaskQ-token price inferred. → dashq.md D24 (no inference); views.md § Reports

## D25. Host link relay
- I235 D25: Blind relay; TaskQ owns keys, encryption and schema; DashQ never decodes, holds no key, runs no command, adds no authority. → dashq.md D25
- I236 D25: Routes and authentication; same-workspace acknowledged unrevoked TaskQ grants; error answers; 5 req/s burst 20 apart from C5. → dashq.md D25
- I237 D25: Send limits; one pending message per (from, to, id); receive semantics; newer receive ends older. → connections.md § Host link relay
- I238 D25: Memory only: 60 s expiry; 4 MiB caps; revoked/disconnected handling; nothing reaches SQLite, logs, `/data` or report; restart drops messages. → dashq.md D25 (memory only); connections.md § Host link relay (expiry, caps)

## Size and outline

Measured with specq `engineering/scripts/contract_lint.py` at 7e9adf5 (LF content; visible line length excludes link targets). Cap: default 61440 bytes and 300 lines; 70% is 43008 bytes and 210 lines.

| | Bytes | Lines | Longest line (visible / raw) | Lint |
|---|---|---|---|---|
| Before (a360de9) | 73925 (120%) | 198 (66%) | 3916 / 3936 | FAIL, 64 errors |
| After | 36816 (60%) | 209 (70%) | 390 / 525 | PASS, 0 errors |

Heading outline after the rewrite:

- dashq
  - Rules
  - Runtime and access
    - D2. Process
    - D3. Deployment
    - D12. Security
    - D20. Profiling
  - Data and sources
    - D6. Unknown and stale
    - D4. Sources
      - D4.1 Langfuse API
      - D4.2 GitHub boards and TaskQ heartbeats
      - D4.3 ServerQ report
      - D4.4 Resource samples
    - D7. /data JSON
    - D5. Refresh
  - Operator page
    - D8. Page
    - D10. Dashboard redesign target
    - D9. ServiceQ views
    - D22. Shareable views
    - D19. First launch and partial availability
  - Product connections
    - D11. Independent product connections
    - D25. Host link relay
  - Gated actions
    - D13. TaskQ answers, task controls and PM requests
      - TaskQ answer card
      - Task controls
      - PM requests
    - D14. TaskQ resource controls
    - D15. ServerQ finding handoff
    - D16. AgentQ integration settings
  - Agent and service views
    - D21. Agent work
    - D24. Model use and estimates
    - D17. AgentQ optimization results
    - D23. Waiting work, changes and route
    - D18. Service versions and diagnostics
  - Linked detail
