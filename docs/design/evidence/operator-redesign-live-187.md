# Operator redesign live run (#187)

Task #187 ran the DQ-01 to DQ-10 journeys against production on 2026-10-24, around epoch `1792850230`. Journeys that could not be proved remain open (A1). This page records observations only. It grants no new authority.

## Candidate and configuration

- DashQ v0.16.0 (`current`) as the FRA service, loopback `127.0.0.1:18882`, public name `dash.ufobe.com`. `previous` remains the coreq rollback (A5).
- The owner deployed it and set the configuration: `task_controls` for the six configured boards with a dedicated write token (mode 0600), the pinned TaskQ 4.6.38 binary and `pm_requests: true`. `answer` is unset.
- Owner grants: controller and PM requester in workspace `gradus`. The TaskQ PM host grant `dashq-pm-187` covers the taskq and dashq projects with runtime `claude` for 168 h.
- Sources read: Langfuse, GitHub, ServerQ report, TaskQ snapshot adapter, enrollment store and TaskQ telemetry. Forwarding export is not configured.
- Method: host-local loopback requests carrying the guard subject header, which D11 trusts like `dashq report`, plus `dashq report`. The browser path (forward auth, rendering, drafts) was not driven: the worker's browser tool was denied.

## Journeys

| Journey | Result | Observation |
| --- | --- | --- |
| DQ-01 overview | Data proved; browser open | `/data` returned 200, with Langfuse, GitHub, ServerQ, TaskQ, agent work and enrollment `ok`. Export reads `unknown` (`export not configured`). There are four host rows. The DESKTOP AgentQ heartbeat reads stale (18 h) and unknown values stay null. All seven views return 200; an unknown path returns 404 with an overview link. |
| DQ-02 changes | Proved | 24 h window: comparison `partial` (`some sources keep no baseline`). There are 11 rows and one unclassified ServerQ item. ServerQ and AgentQ report a baseline. Langfuse and TaskQ report `no baseline`. |
| DQ-03 server problem | Partly proved | Attention lists exactly one request: the ServerQ `disk-headroom` `below-minimum` finding on the Mac, linked to `/checks#finding-…`. Resource history exists only for DESKTOP/wsl (64 samples, stale 18 h). FRA and the Mac have no resource samples because AgentQ sends none (producer gap). |
| DQ-04 agent silent | Partly proved | Evidence stays separate: TaskQ online/queue observations are fresh, login reads `unknown` (`no observation`), and AgentQ snapshots are 5–9 days old, all marked stale. AgentQ evidence for a current failure cannot be shown while producers are silent. |
| DQ-05 waiting work | Proved | agentq #167 and #168 are `waiting` with admission `dependency` and TaskQ's reason. No ready task was open, so other admission codes were not seen live. |
| DQ-06 answer/manage | Controls proved; answer off | Answer: `GET /answer` returns 503 because `answer` is unset; cards stay read-only. Controls refusals: missing subject 401, foreign Origin 403, wrong CSRF 403, unconfigured board 404, stale revision 409 `stale` with nothing written. Owner-approved round trip on agentq #168: offered Later went from `waiting` to `later` with exactly one board entry (`dashq.control.dq187-later-168-1792851606`). The row then offered only Retry. Retry returned `accepted`, state `waiting`, revision `6385326469`, and its same-ID resend returned an identical receipt with one board entry. TaskQ then admitted the task and spawned a worker (see Findings). |
| DQ-07 ask PM | Proved (loopback) | One diagnose-only request (`dq187-dq07-1`). First reply `unknown` (`Queued for TaskQ`); the same ID returned the same operation. Changed input under the same ID: 409. Wrong CSRF 403, missing subject 401, unadvertised revision 403. After 190 s the receipt was `accepted` with native host `macbook-m2.local`, runtime `claude`, state `launched`. The stored request text was cleared, and `/data` contains no request text. The session changed no board task. |
| DQ-08 time/tokens/cost | Partly proved | `agent_work` has 42 attempts on two host names. The #187 attempt shows 4,093,593 context tokens (3/3 known), 35 calls and model `claude-opus-5-5`. `elapsed_ms` is null (`open-stage`) and the estimate is null (`context tokens not split by cache`). `model_use` has no `agentq.model` calls in 24 h, so no API-equivalent estimate. |
| DQ-09 telemetry trust | TaskQ proved; AgentQ open | Same event followed (A2): the Mac TaskQ stage record seq `6011223`, id `107d254601884bd142c484ccf67176ef` (`start`, task 187, attempt 6382012426), was stored at FRA in telemetry block `t-f529e376ee4974f4befb647e87dc5e8e` (seq 6010132–6011226, received 8 s after observation). `dashq report --period=1800` lists that attempt with stage `start`. Route: queued 0, accepted, projected and queryable known. No AgentQ event exists for a qualified runtime in the window. |
| DQ-10 outage | Source proved; drafts open | The ServerQ report was made unreadable for 65 s (mode 000, then restored to 600). ServerQ then read `unconfirmed` (`read failed (report missing), showing 14:02 UTC`) and kept its 5 results. The other six sources and all 6 boards stayed `ok`. After restore it read `ok` with a new time. The store holds no duplicate telemetry event IDs and no overlapping seq ranges, and the PM operation has one row. |

## Findings

- AgentQ producers are silent: enrolled snapshots are 5–9 days old, the only Langfuse heartbeat is 18 h old, and there are no `agentq.model` or resource events on FRA or the Mac. This blocks the A2 AgentQ trace, resource history (A4, DQ-03) and model-use estimates (DQ-08). It is an AgentQ-side producer gap, not a DashQ defect.
- One Mac appears under two host names: TaskQ stage records carry `mac` and `macbook-m2.local` for the same attempt, and host rows split them.
- The DESKTOP host has been silent for 18 h. All its values read stale or last-known, never ok.

- Owner Later replaced #168's `later --blocker` wait on pilotq#36 (still open), and Retry (`requeue`) then made it ready, so a worker started on 2026-10-24 14:21 UTC. This is a TaskQ control-semantics gap, filed as taskq#1689 (held). The worker was left running: it is the task's own queued work.

## Open

- Browser journeys (list → detail → return, drafts across navigation and outage, Origin/CSRF through the real proxy) need a guard-authenticated browser run.
- AgentQ live events (A2, A4 resource history, DQ-08 estimates) wait for the AgentQ producers: agentq#169 (held).
- The browser run was denied again after the owner's answer, because this worker runs in a non-interactive permission mode.
