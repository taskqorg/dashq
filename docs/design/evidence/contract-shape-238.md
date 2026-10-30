# Contract shape: decision inventory (#238)

Inventory of every rule or decision in `dashq.md` on main at a360de9 (before the rewrite). One line per rule or decision, keyed by its section. The rewrite maps each line to its new place in a later commit of the same change.

## D1. Scope
- I001 D1: D11 narrowly replaces no-write, no-push and no-database for enrollment, ingestion and connection management; it authorizes no host commands.
- I002 D1: D13 authorizes gated task controls and the opt-in PM request exchange; enrollment grants no control authority.
- I003 D1: D25 adds the blind host link relay, transport only.
- I004 D1: Read-only: never writes to Langfuse, GitHub, serverq or any host; no state-changing buttons.
- I005 D1: No new auth system; the page sits behind the Langfuse UI forward-auth guard.
- I006 D1: No push channel from hosts; dashq only pulls D4 sources; host push is a later task (#4).
- I007 D1: No database: in-memory caches; a restart rebuilds everything from the sources.
- I008 D1: Metadata only: never conversation text, prompts, tool input/output, raw commands, stderr/stdout or stack traces.

## D2. Process
- I009 D2: One Rust binary `dashq` on the shared `taskqorg/coreq` crate.
- I010 D2: `serve` binds `127.0.0.1:18882` only; not configurable to a non-loopback address.
- I011 D2: `dashq report [--period] [--host] [--agent] [--session] [--cursor]` prints the `/data` JSON once.
- I012 D2: `dashq admin WORKSPACE SUBJECT` binds an enrollment administrator; `dashq profile [--window=S]` prints the profile report, default 86400 s.
- I013 D2: `selftest` loads config, renders the fixture snapshot, reads each source once; no writes.
- I014 D2: Host requirements: curl ≥ 7.20.2 always; taskq ≥ 4.6.32 only with `taskq.binary` (version from its coreq layout); Chrome ≥ 59 for `--snapshot` only (skipped when absent, unchecked on Windows).
- I015 D2: OS service tools have no minimum; no system library (static builds).
- I016 D2: `dashq doctor` prints one line per requirement, exits 1 naming missing/older tools with found, minimum and fix; serve/service run/install/update log `requirement:` lines.
- I017 D2: dashq installs no OS packages; a failed curl, taskq or Chrome run logs its command (never stdin or `--text`) and first stderr line (≤200 chars).
- I018 D2: `--snapshot DIR` writes `page.html` (inlined CSS, no script) and `page.png` via headless Chrome 1280 px; renderer lookup order; no Chrome fails naming the renderer after writing HTML; no config, no source reads.
- I019 D2: Baseline routes `GET /`, `/app.js`, `/style.css`, `/data`; other paths 404, other methods 405.
- I020 D2: Security headers on every response (no-store, nosniff, same-origin referrer, CSP); no third-party assets.
- I021 D2: Config file from `DASHQ_CONFIG` else `<install>/config.json`, mode 0600, with its listed keys.
- I022 D2: Requests whose `Host` is not loopback or the public name get 421.
- I023 D2: Secrets never reach the browser, `/data`, logs or `report`.
- I024 D2: Invalid config: refuse to start naming the key, never the value; selftest without config skips source reads and says so.
- I025 D2: `service install|uninstall|run` and `update [--check]` are coreq commands; `service run` is `serve` and marks healthy after 60 s.
- I026 D2: An update that makes a new release current restarts an installed unit; `--check`, unchanged or failed updates leave it running; failed restart exits non-zero naming the error.
- I027 D2: Automatic update: installed `service run` checks 900 s after start then every 3600 s; `DASHQ_UPDATE_EVERY` overrides; first check waits at most 900 s.
- I028 D2: A newer release is applied by coreq update; the service logs it and exits 75 so the unit or launcher starts the new `current`.
- I029 D2: One service-log line per check/apply with the listed texts; a failed check or apply keeps the running release.
- I030 D2: Profile ops `update.check` and `update`.
- I031 D2: Rollback on failed start: coreq restores `previous` at the third start without `healthy`; the service writes `<install>/rolled-back` and never auto-applies that version again; newer releases proceed; manual update still installs it.
- I032 D2: `serve` and `service run` outside the install never update themselves.
- I033 D2: Release CI on a `v<ver>` tag equal to `Cargo.toml` builds static linux x86_64/aarch64, `SHA256SUMS`, `latest.json`.
- I034 D2: Each artifact passes offline `selftest` before publishing; a failure publishes nothing.
- I035 D2: Every PR runs the same builds and selftests without publishing; `main` requires them with `tests`.
- I036 D2: Only owners push tags; installed services update themselves, other hosts with `dashq update`; rollback is coreq `previous`.

## D3. Deployment
- I037 D3: Runs on the production host (FRA) beside Langfuse and the serverq report.
- I038 D3: Served under `dash.` of the Langfuse UI domain; the proxy runs the Langfuse `forward_auth` guard first, then proxies to `127.0.0.1:18882`.
- I039 D3: No public port, firewall change, public listener or `/api/public` route; browser data including `/data` always guarded.
- I040 D3: D11 machine routes use their own narrow authorization; no wildcard exemption.
- I041 D3: Rollback: remove the `dash.` site block, stop the service.
- I042 D3: dashq supersedes the agentq `/serviceq` panel; its removal is agentq's task.

## D4. Sources
- I043 D4: Each source has its own cache, single flight and a 10 s timeout per HTTP call.
- I044 D4.1: Langfuse public API with project keys, server side; model activity is every `GENERATION` observation.
- I045 D4.1: AgentQ event spans and their attributes; Agents view sources for fleet, versions, last seen, health, usage and incidents.
- I046 D4.1: Last seen is the newest event time; no `agentq.installation` heartbeat in 7200 s is stale.
- I047 D4.1: Periods 1800/7200/86400 s, default 1800; health and incident history read 24 h.
- I048 D4.1: Client use reads `agentq.compress` and `agentq.retrieval` over 24 h; `agentq.installation` read takes the `io` field group.
- I049 D4.1: Cursor pagination, at most 10,000 rows per event name and period; the cap marks the source `incomplete`.
- I050 D4.1: Cache 30 s per period.
- I051 D4.2: Projects are an explicit config list; repositories are never discovered.
- I052 D4.2: Boards: open `taskq` issues, one page of 100; state from `q-<state>`, priority `p0|p1|p2`; over 100 is `incomplete`; no comment folding.
- I053 D4.2: Heartbeats from `taskq-host` issues; age = response `Date` minus `updated_at`, never the local clock.
- I054 D4.2: Latest releases per project repo, cached 10 min.
- I055 D4.2: Cache 60 s per repo; a 403/429 keeps the last value unconfirmed until reset.
- I056 D4.2: Snapshot adapter with `taskq.binary`: isolated scratch run of `taskq status --json` (read-only token install, no HOME, incident reports off, folder removed); single flight, 10 s, cache 60 s.
- I057 D4.2: Snapshot totals, questions and failure reasons replace label counts; heartbeats stay label reads; a failed/incomplete/unknown-schema run is a failed read listed in `sources.taskq`; absent key means labels only.
- I058 D4.3: Local serverq `report.json` read every refresh; last-good kept with its producer time, unconfirmed until a good read; absent config is unconfigured, distinct from missing report.
- I059 D4.3: Absolute config paths and fixtures use the target platform's path rules.
- I060 D4.3: Accepted schema 1 shape; another schema or invalid JSON is unknown with reason.
- I061 D4.3: Stale after 7200 s; manual rules are not verified, never passed.
- I062 D4.3: Enrolled `serverq-report/1` snapshots: one `checks.installations` entry per installation and workspace, hosts row, Attention items; producer-time freshness 7200 s; other payload schema unknown; revoked grant shows nothing current.
- I063 D4.4: Producer contract `agentq.resource`; enrolled history first, Langfuse (with `io`) for the rest; each row names `provenance`.
- I064 D4.4: Window = period; raw samples; at most 2016 per installation, newest kept, a cut is `incomplete`; Langfuse keeps D4.1 pagination.
- I065 D4.4: Per-sample fields; missing/invalid/inconsistent value is `null` with reason; nothing derived.
- I066 D4.4: Current = newest, stale after 900 s; gaps, resets and warmup below three samples; a non-ok row joins Attention gaps.

## D5. Refresh
- I067 D5: Redesign target is specq W4–W6 and D10; the mechanism is the baseline, not proof of stable updates.
- I068 D5: Fetch `/data` on load, every 60 s and on Refresh; no live push.
- I069 D5: Server renders the shell; D13 boards use the pinned TaskQ renderer; sections reload from `/` with the same query after a good fetch.
- I070 D5: Show `Updated <time>` and each source age; a failed fetch keeps the page with the banner, never blanks values.

## D6. Unknown and stale
- I071 D6: Every value carries its source state; missing data never becomes zero or healthy.
- I072 D6: Source states ok/stale/unconfirmed/unknown/incomplete, each non-ok with a one-line reason.
- I073 D6: Freshness limits: Langfuse 600 s; heartbeat `ttl` (300 s); serverq 7200 s; GitHub 180 s.
- I074 D6: Green only for ok and healthy; others gray/amber with reason and age; missing number `—` with reason; capped `≥N (capped)`.
- I075 D6: Counters that reset with a restart say so; a reset is not a drop.
- I076 D6: Historical incidents deduplicated, never current failures.
- I077 D6: A summary bar lists every non-ok source with its reason (placement amended by D10).
- I078 D6: `/data` sections carry `source: {state, at, age_s, reason, incomplete}`; unknown numbers are `null`.

## D7. /data JSON
- I079 D7: Query parameters, `block` and `cursor`; invalid 400; all sources failed 503; partial 200.
- I080 D7: Top-level keys, `schema` 2, `resources` shape.
- I081 D7: Session filters apply to model activity only.
- I082 D7: Aggregates and evidence trace IDs only, never prompts, commands, tool results, tokens or keys.
- I083 D7: Additive changes within `schema` 2; breaking change raises it; one declared removal (`session.line`).
- I084 D7: Agents read it with `dashq report` or through the guard.

## D8. Page
- I085 D8: The accepted operator redesign updates D8/D10; host-first baseline: hosts first, tasks through shared TaskQ UI, other information on demand.
- I086 D8: Design package records approval; production qualification pending; English; D1–D7 binding except the D10 source-summary amendment.
- I087 D8.1: Minimal Overview: one machine/kind per group, identity → state/age → resources with trends → detail/action; missing resources one unavailable state; partial metrics keep reasons.
- I088 D8.1: Versions/update, login probes, queue counts and receipts in host detail and D22 views; each value keeps source/time.
- I089 D8.1: `hosts[].taskq.observed_running` nullable; `boards[].attempts` adds `host_id` and `current`, not pending assignment.
- I090 D8.1: Silence means unknown; a heartbeat never proves login or service health.
- I091 D8.1: `taskq-board/1` observations supply online, login and assigned queue; `hosts[].taskq.observations` fields.
- I092 D8.1: Latest per board/workspace/host/kind wins; equal-time conflicts unknown; missing/expired/future/invalid/partial evidence unknown with TaskQ's reason; no inference from activity.
- I093 D8.1: H2 machine identity joins labels; absent identity keeps its own row; shared queues unassigned, never multiplied.
- I094 D8.2: Attention only for conditions needing a person and an action, with the four mappings; names host/task, reason, age, action.
- I095 D8.2: Gaps, revoked connections and incident history are detail; retries and waits are not requests; non-ok meaning stays beside values; a source outage needing intervention may appear.
- I096 D8.3: Tasks: host count opens its tasks; direct entry has all projects and unassigned work; shared TaskQ rows and controls; retained fields; missing release is an explicit gap.
- I097 D8.4: `/agents` starts with attempts and an inspector; selection/focus/return survive; client activity, D21, D17, D24 separate, never merged.
- I098 D8.4: Per-model sample counts separate; cost null unless reported; client observation rules; login separate.
- I099 D8.5: Server checks keep per-rule detail, incidents, linked work, evidence, manual rules not verified; a closed task never establishes recovery.

## D9. ServiceQ views
- I100 D9: Preservation ledger maps every section and prior ServiceQ capability; retired Node-monitor features stay retired.
- I101 D9: D7 report/JSON, D4 periods/caches/caps and source evidence remain available independent of page placement.

## D10. Dashboard redesign target
- I102 D10: The accepted operator redesign package owns the target and ledger; runtime authority and qualification gates remain.
- I103 D10: Ownership of composition, resources, boards and connections pages; W1–W15 apply.
- I104 D10: Live producer qualification remains a gap; full TaskQ workspace adopted.
- I105 D10: Overview cards across the width, one column when narrow without smaller type; detail expandable; Attention is the D23 request list after hosts with header shortcut/count; no side-by-side boards; expected hosts stay visible.
- I106 D10: Source status collapsed with count and reasons; stale/superseded/revoked records hidden by default with filters; never hides a current warning; D11/D16 overview sections in detail; no empty panels.
- I107 D10: Resource bars and 30-minute 0–100% trends from D4.4 samples; three samples; gaps/resets break lines; states explicit; detailed history in host detail.
- I108 D10: Thresholds producer/policy-owned; no demo threshold becomes policy.
- I109 D10: Browser fidelity check pins the approval hash and compares geometry at the listed sizes; fixture evidence never qualifies production.
- I110 D10: 60 s refresh, 1.3 s tint (none under reduced motion); preserve rows, selection, focus, drafts, disclosures, scroll; never reorder hosts; freshness is producer time.
- I111 D10: TaskQ interiors only from published releases pinned by version and hashes; the three pinned asset sets; action gates separate.
- I112 D10: Target decisions: expenses and estimates separate; telemetry events/metrics only, worker-line content retired without retrospective deletion; PM request with native-session continuation through TaskQ UI; nothing enabled by the target.
- I113 D10: Acceptance: #169 A1–A5 and review package; prototypes prove layout only; no deployment, sign-in, host command, restart/update button or new mutation authority.

## D12. Security
- I114 D12: Follows the specq security baseline; reports via `SECURITY.md`.
- I115 D12: Every workflow `permissions: contents: read`; publishing job adds only `contents: write`.
- I116 D12: Repository is public; shared baseline applies.

## D11. Independent product connections
- I117 D11: Enrollment C1–C7 and connections L1–L4 own the contract and presentation.
- I118 D11: One process and one SQLite store (WAL/FULL); no extra daemon or database server.
- I119 D11: C3 routes/authority, C4 acceptance/replay, C5 limits; CSP only for reviewed approval forms; loopback listener; no read APIs via machine routes.
- I120 D11: Registry supplies expected installations; optional sources never block enrollment or hide snapshots; no duplicate identities by hostname.
- I121 D11: Actor scope applied server-side everywhere; local report is a host-administrator capability; producer-time freshness; connection and export health separate.
- I122 D11: Approval, scoped overview, explicit revoke; first completion needs the browser's projection; observation grants give no answer or command authority.
- I123 D11: Pairing routes per `coreq-enrollment/1`; guarded `/connect`.
- I124 D11: Browser subject only from proxy-set `X-Dashq-Subject`; loopback callers trusted like `report`; proxy-only listener or secret is a #4 item.
- I125 D11: Re-pairing warns; approval needs membership, Origin and a CSRF token bound to account and code.
- I126 D11: `dashq admin` creates `dashq.db`/`dashq.key` (0600); until then enrollment answers 503; sealed token erased at ack/expiry; store keeps only SHA-256 verifiers.
- I127 D11: Board scope on device request; malformed `invalid_request`; shown on approval; stored and named in replies; other scope 403 `scope`; revoke always succeeds.
- I128 D11: Ingestion routes; conformance traces; grant supplies installation/workspace.
- I129 D11: Rejections `scope` (schema or board), `payload` (metadata allowlist), `replay_mismatch`, `expired`; full store 503.
- I130 D11: Metadata allowlist definition and the #125 `choices[].text` exception.
- I131 D11: Per device 60 requests/min, burst 10.
- I132 D11: `agentq-event/1` shape enters the outbox, never a snapshot; other schemas are snapshots; AgentQ handoff open.
- I133 D11: Forwarding (`langfuse.forward: true`): batching, OTLP endpoint, header, span mapping, DashQ ID and deterministic IDs.
- I134 D11: Forwarding outcomes: 200 confirms; rejected spans become an export gap; 4xx one-by-one resend; outage/5xx stay; 7-day expiry gap; full outbox 503 `outbox_full`.
- I135 D11: `sources.export` states on loopback `/data`.
- I136 D11: Two-key payload without `metadata` and inexpressible `observed_at` rejected `payload`; queued such rows become gaps.
- I137 D11: Readers tolerate a transient duplicate; the opt-in live check is rerun after a Langfuse upgrade; forwarding off by default.
- I138 D11: Data rows keep their grant's workspace; a new approval after revoke may pick another workspace, which starts empty.
- I139 D11: One projection function serves `/data`, `report` and a 2 s pass; `projected` only when read as current and rendered for the three schemas; others stay `accepted`.
- I140 D11: `/data` `installations` and `sources.enrollment` row fields and freshness (180 s, serverq 7200 s, future unknown).
- I141 D11: Scope: loopback and report see all; public requests see the subject's workspaces; none without a subject; optional source failure never hides snapshots or makes 503.
- I142 D11: Connections detail fields per row; revoked never an active failure.
- I143 D11: Approval wait refresh (2 s, 60 s) and redirect once projected.
- I144 D11: Revoke confirmation, Origin/CSRF bound to account/installation/workspace; only administrator or approving member revokes.
- I145 D11: Refresh keeps disclosures/focus/scroll; report store read needs 0600 config and key.
- I146 D11: Connections separates activation, readback, receipt time, producer time; no receipt = contact unknown; export unknown for AgentQ without forwarder, n/a otherwise; reconnect shows destination and command; retries same generation.
- I147 D11: `taskq-telemetry/1` needs a grant naming it; payload shape, limits and cursor rules; malformed is `payload`.
- I148 D11: Telemetry lines pass the allowlist with the #181 `:`-segment exception; a disallowed line is dropped alone, block re-encoded, loss cause `allowlist`.
- I149 D11: `status` counters; conformance fixture; a block stored once with C4 receipts in its own table.
- I150 D11: Telemetry bounds config defaults and ranges; prune by retention; budget drops; oversize block `payload`; known loss by cause outlives blocks; full store acknowledges nothing; older/rolled-back blocks get seq from payload; restored rows read not received.
- I151 D11: `/data` installation `telemetry` states and fields; connection panel line.
- I152 D11: `block=ID` returns `telemetry_block`; no-longer-stored is `unknown`.
- I153 D11: Telemetry view renders coverage fields; `telemetry.evidence` newest 200 blocks; links; no range proves all-event coverage; content analysis later; FRA deploy needs owner approval.
- I154 D11: `taskq-resources/1` grant, payload, target identity, newest-replaces storage; `project_limits` states and fields; fixture.
- I155 D11: Security exception: non-expiring, revocable, per-installation, observation-only ingestion credential.

## D13. TaskQ answer card
- I156 D13: TaskQ owns one card; DashQ pins assets and supplies the scoped adapter; amends D1 with guarded `POST /answer`; enrollment has no action authority.
- I157 D13: Canonical dispatcher, expected revision, op ID, verified receipt; consequential operations excluded.
- I158 D13: Route off until `answer` configured; production writes wait for live qualification; capability via `GET /answer` per board.
- I159 D13: Tasks owns one card per board/task; Attention navigates; canonical question fields kept.
- I160 D13: Production `answer` is configured on FRA for `taskqorg/dashq` only; other boards read-only.
- I161 D13: Session progress `session {host, runtime, stage, age_s}` on doing/review rows; no `codex://` off host.
- I162 D13: Worker line text never reaches views; `line` removed; ingestion strips it; stored ones age out, never projected; nothing deleted; the full log never leaves its host.
- I163 D13: Static snapshots keep issue links and a component-unavailable reason.
- I164 D13: Task controls route, off unless config and `taskq.binary`; boards must be configured projects.
- I165 D13: `dashq controller` grant; actor must be workspace administrator with the grant; other grants give no control authority.
- I166 D13: `controls` list is canonical, never inferred; TaskQ v4.6.30 owns renderer, commands and fencing; Close is a link.
- I167 D13: Actor, fresh row/revision and qualified adapter gate dispatch; Origin/CSRF, journal, same-ID retry; unknown delivery never accepted or fresh.
- I168 D13: Production `task_controls` configured on FRA for the six configured boards.
- I169 D13: PM request routes using `taskq.pm/1`; `pm_requests` off by default; separate `pm-requester` grant.
- I170 D13: Actor is SHA-256 of the subject; TaskQ must advertise a matching host PM grant and revision; Origin/CSRF and rate limits.
- I171 D13: Bounded temporary outbox keeps exact input until terminal receipt or expiry; digest tombstones forbid reuse.
- I172 D13: No conversation, reply bridge, task admission or host-command authority; receipts never contain request text; `/data`/report contain no PM text.
- I173 D13: Drafts and uncertain requests stay in the page session; only the same request reconciles.
- I174 D13: Production `pm_requests` is on.

## D14. TaskQ resource controls
- I175 D14: Reuses TaskQ components; separately authorized typed host-policy capability; no `/answer` or observation authority; TaskQ owns limits and readback; no remote shell or second scheduler.
- I176 D14: Operations stay off; `resources` set only after taskq#931 ships and live qualification passes.
- I177 D14: Project limits: read `taskq-resources/1`, offer `taskq.project-limits/1` under a `project-limits` grant on pinned v4.6.25 assets; live qualification is #158.

## D15. ServerQ finding handoff
- I178 D15: Authorized TaskQ create/optional-start adapter and composer; no authority for observation or `/answer`; ownership split.
- I179 D15: A closed task alone never removes a problem from Attention.
- I180 D15: Finding identity workspace/installation/rule/first_seen; no inferred resource join; H4/H7 grouping unchanged.
- I181 D15: Episodes from retained samples; only a complete fresh matching passing rule with ordered `verified_at` resolves; missing/capped history explicit.
- I182 D15: Vendored catalog only on SHA-256 match, else thresholds and recommendations unavailable; creation pending.

## D16. AgentQ integration settings
- I183 D16: Separately authorized typed AgentQ config adapter; AgentQ owns plans and verification; no observation or `/answer` authority; release gated.
- I184 D16: `agentq_configure` (default off) enables `/agentq/configure`; off until AgentQ A5 qualification.
- I185 D16: Device `capabilities` (only `agentq.configure`); malformed `invalid_request`; separate unticked effects; grant keeps only ticked ones; observation grants read-only.
- I186 D16: Operation routes; authority only from the server (actor, workspace, profiles); others refused; payload never adds authority.
- I187 D16: One unresolved apply per installation; revoke resolves delivered apply `rejected` (`revoked`); new apply after reconnect accepted.
- I188 D16: Read-only Agent integration view per profile; missing/older projection unknown; stale never green; configured never verified; no control.

## D17. AgentQ optimization results
- I189 D17: Read-only results with AgentQ accounting; no config authority, optimizer or experiment runner.
- I190 D17: `/data.optimization` through `agentq.accounting/1` with 29 conformance cases.
- I191 D17: Empty reads unknown, counts partial, buckets never invent samples; whole-task effect unknown without experiment source.
- I192 D17: Inherits filters; disclosures/focus/selection survive refresh; only the macOS run adapter is live-qualified.

## D18. Service versions and diagnostics
- I193 D18: Distinguish available, selected, running, verified health; lifecycle owned by coreq and consumers; allowlisted diagnostics; no remote update/restart/rollback.
- I194 D18: `versions` rows from the listed sources and their identity rules.
- I195 D18: Row fields and `update` only from a fresh verified comparison.
- I196 D18: `lifecycle`/`health` from AgentQ `release` with the verification conditions; others report neither.
- I197 D18: Producer timestamps; stale or not running keeps `last_known`; unconfirmed never verified; host filter before `mixed`.
- I198 D18: Producer gaps listed; cases fixture.
- I199 D18: Services view: one selected detail, separate times, explicit gaps; state survives refresh; no lifecycle action.
- I200 D18: Versions are `MAJOR.MINOR.PATCH[-pre]`, other forms unknown; other enrolled fields and Langfuse adapters await authorization.

## D19. First launch and partial availability
- I201 D19: Startup U1–U4 compose existing rules without new authority; no empty panels; last-known values and drafts kept; per-source action gating.
- I202 D19: First-launch entry continues to `/connect`; optional inputs listed once; TaskQ-only has no empty panels.
- I203 D19: Failed boards keep historic counts beside unknown counters; failed refresh keeps page and components, marks unconfirmed, ages advance; recovery clears banner without writes.
- I204 D19: ServerQ keeps last-good; Langfuse resources not fresh after failure; trends need three samples; shared adapters read-only until qualification.
- I205 D19: Config validation fixtures use native absolute paths.

## D20. Profiling
- I206 D20: Applies specq built-in profiling.
- I207 D20: Operations with p95 and error budgets; times include writing; cache hits are not reads; 5xx failure, 4xx caller; assets and 404 unmeasured.
- I208 D20: Counters always on in serve, service run, report, update; record shape; error class rule; no secrets/content/URLs/paths; selftest and tests write nothing.
- I209 D20: Store `profile.jsonl` 0600, 4 MiB prune to newest half; failed writes never fail the operation.
- I210 D20: `DASHQ_TRACE=1` spans and step lines.
- I211 D20: Report JSON, ranking, unknown and incomplete rules; never zero or ok.

## D21. Agent work
- I212 D21: Source: telemetry `stage` observations primary, local `taskq.log` fallback; cutover dedup; only valid codes; no row without either.
- I213 D21: Period by end time; host filter; `/data.agent_work` shape, rows, stages, usage rules; `tokens` never added to another count; `time_ms` meaning.
- I214 D21: Attempts: newest 200 with fields; link only for configured projects.
- I215 D21: Elapsed/execution only for complete coverage; states and reasons otherwise; open markers never samples.
- I216 D21: `queue_wait_s` from the ready transition, else null with reason.
- I217 D21: `output_tokens`, `models`; `estimate` null; multi-host attempt is one row.
- I218 D21: Unknown/stale/incomplete rules; never zero or ok; budgets unknown.
- I219 D21: Page: attempts first when configured; inspector; additive timeline fields; overlaps never become complete or idle; progress unknown without evidence; admission is current evidence only; totals separate.

## D22. Shareable views
- I220 D22: Applies specq F7; `/` overview; listed guarded page paths; unknown 404 with recovery; unavailable views say so.
- I221 D22: Shareable state is path plus D7 filters; local state stays local and survives navigation; reload restores view and filters, not drafts; secrets never in URLs.
- I222 D22: `/reports` reads only D21/D23/D24; `agent_work.totals` definition and rules; same values in report; filters scope; shows coverage; links to `/agents`.

## D23. Waiting work, changes and route
- I223 D23: Read-only; one projection serves `/data`, page and report.
- I224 D23: Admission from snapshot or board; pinned vocabulary; unknown rules; never infers a stall; Tasks lists waiting work.
- I225 D23: Changes shape; new/ongoing/resolved; unclassified; coverage per source; comparison state never "nothing changed".
- I226 D23: Route six stages with known/unknown/n/a; sources; Telemetry view rendering; known false not passed; unknown never passed.
- I227 D23: `attention.requests` only producer requests needing a person; three kinds with targets; Attention renders exactly these.
- I228 D23: Query shape; cursor paging by 200; UTC.

## D24. Model use and estimates
- I229 D24: Read-only; no billing source, credential or authority.
- I230 D24: Price table config shape and validation; entry effective on or before the call day.
- I231 D24: `model_use` shape; estimate preconditions and reasons; currencies separate; incomplete only on capped read.
- I232 D24: Runtimes and their unknown reasons; TaskQ stage tokens never priced.
- I233 D24: Estimates labelled `API-equivalent estimate`; report prints the same.
- I234 D24: Reports displays estimates with coverage; retained range unknown; no expense or TaskQ-token price inferred.

## D25. Host link relay
- I235 D25: Blind relay; TaskQ owns keys, encryption and schema; DashQ never decodes, holds no key, runs no command, adds no authority.
- I236 D25: Routes and authentication; same-workspace acknowledged unrevoked TaskQ grants; error answers; 5 req/s burst 20 apart from C5.
- I237 D25: Send limits; one pending message per (from, to, id); receive semantics; newer receive ends older.
- I238 D25: Memory only: 60 s expiry; 4 MiB caps; revoked/disconnected handling; nothing reaches SQLite, logs, `/data` or report; restart drops messages.
