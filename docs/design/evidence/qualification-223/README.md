# Redesign qualification on real scoped data (#223)

Task #223 checked the finished redesign (#219–#222) against the approved reference, then ran the DQ-01 to DQ-10 journeys on production data. The run happened on 2026-10-26 between 16:32 and 17:28 UTC (epoch `1793032377`–`1793035688`). This page holds only counts, states and reasons. It contains no private input, credential or task content, and it grants no authority. Nothing was deployed, enrolled, granted or changed on a producer under this task (A3).

## Candidate and method

- FRA runs DashQ 0.17.1 (`releases/0.17.1`). The PM released it from main `e34bb3a`, and that commit contains #225–#228. The served `/style.css` and `/app.js` bundles are byte-identical to that commit.
- Two sources are compared: host-local loopback `/data` (D11 scope: all), and `dashq report` run as the service user from the 0.17.1 binary. Browser runs reach the loopback through an SSH tunnel and abort every non-GET request. Results: no blocked writes and no page errors.
- Fidelity check: [`tests/qualification-browser.cjs`](../../../../tests/qualification-browser.cjs).
  - It uses #219's method: the public reference pinned by `approval.json`, and a matrix of 1280/16, 360/20 and 1280/24 in light and dark.
  - Overview: card padding and radius match the reference. The order is identity, then state, then facts, then action. There is one card per `/data` host.
  - Agents and Reports: grid columns and gap match the reference.
  - Telemetry: six ordered stages per sender, and the coverage grid reflows like the reference diagnosis grid.
  - Tasks: every configured board keeps its coverage line, and the workspace fits the viewport.
  - System: each of the six reference destinations opens its retained D22 view.
  - Every view: the requested text size is never reduced, and ordinary text, including a long identifier, reflows without horizontal scroll.
  - CI runs the check on the exported fixture. Setting `DASHQ_QUALIFY_URL` runs the same check on live data, with the server-rendered page and `/data` from the deployed release.
- Live journeys: [`live-journeys.cjs`](live-journeys.cjs).

## A1 fidelity

| View | Fixture (6 cells) | Live 0.17.1 data (6 cells) |
| --- | --- | --- |
| Overview | PASS (3 hosts) | PASS (4 hosts) |
| Agents | PASS (8 attempts) | PASS (attempts in the 30 min window) |
| Reports | PASS | PASS |
| Telemetry | PASS (3 senders) | PASS (14 senders) |
| Tasks | PASS (3 boards) | PASS (6 boards, 4 open tasks) |
| System detail | PASS (6 destinations) | PASS (6 destinations) |

The existing per-view checks also pass on macOS with Chromium 141: `overview`, `agents`, `reports`, `telemetry`, `workspace` and `task-controls`. Live readback: [live-fidelity.json](live-fidelity.json).

The private mock was re-verified on host mac without publishing it. The packet manifest's SHA-256 equals `approval.json` `private_packet_manifest_sha256`, and all 45 listed files match their manifest hashes. The accepted mock's hash equals `accepted_mock_sha256`. No private file was copied, quoted or committed.

### Defect found and fixed

- Before: on live data at 360/20, `/checks` scrolled horizontally (document 400 px wide in a 360 px viewport). A 32-character ServerQ installation ID in a row heading cannot wrap. The fixture IDs were short, so no existing check failed.
- After: `code { overflow-wrap: anywhere; }` in `assets/style.css`. The check now inserts a 64-character token into every System destination. It failed on the fixture before the fix (`Checks & findings: ordinary text reflows`) and passes after it, on the fixture and on live data.

## A2 journeys

`/data` and `dashq report`, taken in parallel for the 2 h period, differ in exactly one field. That field is the TaskQ admission `observed` time of one task, refreshed between the two reads. `telemetry_block` for the same block ID is identical through `/data?block=` and `dashq report --block=`. The browser checks compare UI counts with the `/data` document read in the same session.

| Journey | Result | Observation |
| --- | --- | --- |
| DQ-01 start of day | Proved | Langfuse, GitHub, ServerQ, TaskQ, agent work and enrollment read `ok`. Export reads `unknown` (`export not configured`). There are four host cards, one per `/data` host. Three hosts without an AgentQ heartbeat read `unknown` (`no agentq heartbeat`), never offline. ServerQ rows older than 11 days read `stale`. All eight views keep `period=7200` from navigation. |
| DQ-02 changes | Proved | In the 24 h window, the comparison reads `partial` (`some sources keep no baseline`): 10 rows (3 new, 7 ongoing) and 1 unclassified ServerQ item. ServerQ and AgentQ keep a baseline. Langfuse (`model errors carry no start time`) and TaskQ (`TaskQ reports no state change times`) do not. |
| DQ-03 server problem | Proved; resource history partial | Attention lists exactly the 2 `/data` requests, both ServerQ `disk-headroom` warnings. The finding action opens its `/checks#finding-…` target, and Back returns to Overview. Only one host row has resource history (Langfuse provenance). The other hosts have no samples (producer gap below). |
| DQ-04 agent silent or slow | Proved | Evidence stays separate. Mac TaskQ login is `logged in` with a producer expiry. Other hosts' login reads `unknown` (`no observation`). AgentQ on FRA and the Mac reads lifecycle `pending_disable` (`host_disabled`) with verified health. The attempt inspector names its coverage state. |
| DQ-05 waiting work | Proved | One waiting task carries admission `dependency` with TaskQ's reason, and the Tasks view shows it. Other admission codes were not observable live. |
| DQ-06 answer/manage | Read-only proved | `/data` offers 6 controls across 4 tasks, each listed by TaskQ. `GET /answer` and `GET /task/control` without a subject return 401. No answer or control was sent: write journeys stay with #187/#195 evidence. |
| DQ-07 ask PM | Read-only proved | A draft (never sent) survives view navigation, Back, a failed refresh and recovery. `/data` contains no PM text. |
| DQ-08 time/tokens/cost | Proved; model estimate open | 24 h: 98 attempts on three host names. Wall time and execution are known for 82 of 98 (`adopted`, `no terminal record`, `open-stage`). Queue wait is known for 61 of 98 (`reviewer wait not recorded`). A Reports attempt link opens its Agents inspector. `model_use` has 0 rows (`no agentq.model calls in the period`). The price table reads `ok`, so no estimate is invented. Attempt estimates stay null (`context tokens not split by cache`). |
| DQ-09 telemetry trust | TaskQ proved; others unknown | One stage record of this task's attempt (seq `6587501`) is stored in block `t-02ecf50ee6c3ddb30b087cab38642697` (seq 6587031–6587751). Its observed and received times are 1 s apart. The same attempt appears in `agent_work`. TaskQ route: queued 0; accepted, projected and queryable are known. Emitted is `unknown` for every sender. 14 senders are listed. The listed block link reads `ok`. |
| DQ-10 outage and recovery | Client side proved | A failed `/data` fetch shows `Refresh failed (Failed to fetch)` and keeps rows and the draft. Recovery clears the banner. No production source was made unavailable under this task. #187 recorded the ServerQ outage, and the ServerQ, TaskQ and Langfuse outage paths are protected by `startup_serverq_retains_last_good`, `client_receipt_and_scoped_data_project_the_same_snapshot_through_outages` and `cached_outages_preserve_last_known_versions`. |

## A3 preservation and telemetry rows

Each ledger row of the [implementation brief](../../operator-redesign/implementation-brief.md#capability-preservation-ledger) and each T row of [scenarios and data](../../operator-redesign/scenarios-and-data.md#telemetry-requirements) names its check.

| Row | Check |
| --- | --- |
| Hosts, resources, freshness | `minimal_overview`, `window_keeps_gaps_resets_stale_and_provenance`, `overview-browser.cjs`; live DQ-01/DQ-03 |
| TaskQ presence/login/queue | `taskq_host_observations_identity_and_freshness`, `host-observations-browser.cjs`; live DQ-04 |
| Versions/updater | `service_versions_render_projected_evidence`, `version_cases_project_only_reported_evidence`; System → `/services` live |
| Services | `service_versions_render_projected_evidence`; System → `/services` live |
| Checks, findings, incidents | `requests_open_the_exact_finding_and_changes_resolve_episodes`, `serverq_episode_chain_keeps_identity_recovery_and_recurrence`, `findings-browser.cjs`; live DQ-03 |
| Projects, tasks, answers, menus | `shared_workspace_conformance`, `workspace-browser.cjs`; live Tasks cells |
| Start/Stop/Later/Retry | `task_controls_fixture_dispatch_and_replay`, `task-controls-browser.cjs`; live DQ-06 (offers only) |
| Close task | `shared_workspace_conformance` (board link only); live Closed links in Tasks coverage |
| Project resource controls | `only_a_scoped_resource_grant_authorizes`, `resources-browser.cjs`, `limits-browser.cjs` |
| AgentQ configuration | `agentq_profiles_render_read_only_from_snapshots`, `configure_operations_carry_server_authority_to_the_host` |
| Connections/enrollment | `coreq_conformance_traces`, `connections_offline_revoke_host_and_server_chain`; System → `/integrations` live |
| Agent work | `attempt_inspector`, `agent_work_agrees_across_data_report_and_page`, `agents-browser.cjs`; live DQ-04/DQ-08 |
| Stage aggregates | `stage_records_to_agent_work`; System → `/agents#agenttotals` live |
| Optimization/accounting | `observations_to_results` (pinned `agentq.accounting/1` cases) |
| Reports | `reports_view`, `reports-browser.cjs`; live DQ-08 |
| Telemetry and source health | `telemetry_view`, `route_names_each_stage_known_or_unknown`, `telemetry-browser.cjs`; live DQ-09 |
| PM | `pm_request_exchange_receipts_and_restart`, `pm_request_guards_and_expiry`; live DQ-07 (draft only) |
| T1 Identity | `ingest_takes_ownership_from_the_grant_and_allowlists_payloads`, `hosts_join_one_row_per_machine`; live event ID followed in DQ-09 |
| T2 Time | `attempt_inspector`, `dedup_mismatch_late_and_future_samples`; live observed/received times in DQ-09 |
| T3 Coverage | `reports_view`, `telemetry_evidence_is_scoped_and_bounded`; live `incomplete` host retained from 09:27 UTC |
| T4 Delivery | `telemetry_view`, `forwarding_retries_by_event_id_and_reports_gaps`; live route stages in DQ-09 |
| T5 Lifecycle | `admission_codes_pass_through_from_the_pinned_release`, `admission_without_a_decision_stays_unknown`; live DQ-05 |
| T6 Accounting | `estimates_from_priced_calls`, `agent_work_agrees_across_data_report_and_page`; live DQ-08 |
| T7 Query | `live_waiting_work_report_matches_data`, `telemetry_block_reads_back_alike_through_data_and_report`; live report/`/data` diff above |
| T8 Storage lifecycle | `telemetry_survives_migration_rollback_and_restore`, `telemetry_budget_is_per_installation_and_loss_stays_known`; live 1931 retained blocks |
| T9 Privacy/authority | `taskq_session_line_is_never_stored_or_projected`, `guard_scope_and_refusals_queue_nothing`; live: no writes, 401 without a subject |

## Producer gaps (unknown, with owner)

| Gap | Reads | Owner |
| --- | --- | --- |
| AgentQ resource samples on FRA, the Mac and DESKTOP native | no samples; Overview shows the unavailable state | AgentQ (both hosts disabled by owner) |
| AgentQ `agentq.model` calls (API-equivalent estimate) | `no agentq.model calls in the period` | AgentQ |
| Emitted counts for every sender | `unknown` | TaskQ, AgentQ, ServerQ senders |
| TaskQ task state age and state change times | `age unknown`; changes `no baseline` | TaskQ |
| TaskQ context tokens split by cache | estimate null | TaskQ |
| One Mac reported under `mac` and `macbook-m2.local`, plus one host name that is a LAN address | separate stage-host rows | TaskQ host identity |
| Langfuse model-error start times | changes `no baseline` | Langfuse adapter / AgentQ |
| Export forwarding | `unknown` (`export not configured`) | Owner configuration (off by default) |

Publication acceptance remains green merge-result CI (R12). Owner visual acceptance of the finished redesign is separate from this evidence.
