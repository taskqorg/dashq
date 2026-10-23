# TaskQ workspace and PM requests

[D13](../../dashq.md#d13-taskq-answer-card), #186, adopts the unchanged TaskQ v4.6.34 workspace, detail popover, menus and PM composer. [workspace-provenance.json](../../assets/taskq/workspace-provenance.json) pins the eight release assets against [SHA256SUMS](https://github.com/taskqorg/taskq/releases/download/v4.6.34/SHA256SUMS). [TaskQ workspace](https://github.com/taskqorg/taskq/blob/v4.6.34/docs/design/task-workspace.md) and [PM wire](https://github.com/taskqorg/taskq/blob/v4.6.34/docs/design/pm-requests.md) own their APIs. The older project/answer/control and resource assets keep their separate pins and conformance checks.

## Workspace adapter

`/projects` embeds the released `TaskqTaskWorkspace` through `assets/workspace.js`. A native CSS scope isolates its unchanged window stylesheet from the other DashQ views. The embedding includes DashQ padding in TaskQ's full-width body measurement. Browser support for CSS `@scope` is required. No iframe, extra listener or CSP exception is added.

| Existing capability | Destination and authority |
| --- | --- |
| All configured boards and unassigned tasks | TaskQ workspace list, counters, saved views, search and project menus |
| Host count | Same workspace with current attempt evidence only; host queue coverage stays unknown |
| Task detail, state, issue, failures | TaskQ detail; board identity and producer coverage stay beside the workspace |
| Question and Attention link | Same workspace question; existing guarded `/answer` dispatcher |
| Start, Stop, Later, Retry | Existing `/task/control`; a fresh canonical offer, revision and controller grant remain necessary |
| Close, unsupported management and host operations | Explicit unavailable response or the authoritative issue/Closed board link; no new dispatcher |
| Resources, connections, services, checks, agent work | Existing DashQ routes and D14/D16 components remain reachable |
| Conversations/PM entry | Shared PM composer below the workspace; continuation stays native |

The adapter supplies observed board rows, not a second scheduler. Missing counts remain null; service health, host queues and native settings remain unknown. Observation freshness never establishes a running service. Session progress retains its producer host, runtime, stage and age; no worker-line or conversation content is added.

Workspace instances remain connected during refresh. TaskQ owns local saved views, selection, popovers and answer drafts; DashQ retains the PM draft and exact pending request in page-session memory. Navigation and a failed refresh keep that state. Reload does not restore unsent drafts. Changing filters still applies the authoritative source scope. The released composer freezes a submitted request and offers Check delivery for that identity.

## PM authority and exchange

Off by default: a deployment must explicitly set `pm_requests: true`. No binary path or forge write token is used for PM delivery. A host administrator separately runs `dashq pm-requester WORKSPACE SUBJECT on|off`; only an administrator of that workspace with this grant may submit or read receipts. The command prints the stable actor ID, the SHA-256 of the guard-verified subject. Neither answer/controller grants nor observation credentials confer this permission.

The owner must separately authorize that actor on the enrolled TaskQ host using TaskQ's `service pm` command, bound to its installation/generation, registered project, runtime and expiry. DashQ cannot create that host grant. TaskQ advertises only its authorized targets. DashQ shows a target only to its matching actor, in the connection's workspace, for 90 seconds after the last exchange and before grant expiry.

- `GET /pm/request` requires the verified browser subject and returns that actor's authorized targets and actor/day/route-bound CSRF token.
- `POST /pm/request` accepts the unchanged `taskq.pm/1` JSON request (at most 96 KiB encoded, 64 KiB UTF-8 text). It requires the configured HTTPS Origin, JSON content type, `X-Dashq-CSRF`, matching actor, active connection, separate workspace grant and fresh advertised revision/target. It shares the ten-operations-per-minute actor limit with answers and controls.
- `POST /machine/pm/v1/exchange` accepts TaskQ's exact schema, authenticated by its active connection credential and installation/generation. Other products and revoked generations are refused. The existing device rate limit applies. It replaces target advertisements, accepts only correlated metadata receipts and returns at most eight queued requests. The host initiates every exchange; DashQ runs no host commands.

Before acknowledging submission the existing WAL/FULL SQLite store commits the exact request and its digest. One outbox is bounded by 4096 permanent operation identities and the existing store ceiling/reserve. Full or unavailable storage acknowledges nothing. A repeated actor/op-id with other input is refused. The same input reads its existing receipt after restart; no fresh identity or native launch is manufactured. Revoked workspace grants stop delivery and receipt access immediately. Revoked or replaced connection generations receive no requests.

Request text is temporary input, not conversation storage: it is cleared on a terminal receipt, on the next PM write/exchange after expiry, or by the hourly prune. Identity digests and sanitized receipts remain. Expired or uncertain delivery remains unknown, even when input has been cleared; it never permits recreation. Host receipts contain no request text, paths or arbitrary reason prose; native URLs are limited to TaskQ's HTTPS Claude session shape. An accepted request is not proof of launch: the shared receipt must name the observed native host/runtime/session. PM-created tasks appear only through ordinary board observations. `/data`, `report`, profiling and telemetry contain no PM request text or dialogue.

## Verification and rollout

`node tests/workspace.cjs` verifies release hashes, typed request limits, fixture outcomes and actor/op-id/revision receipt correlation. `cargo test --locked pm_request` covers the browser/host exchange, separate grants, stale targets, generation and actor fencing, duplicate/changed input, foreign receipts, restart, expiry and revoked delivery. Existing answer/control tests retain their protection. The config check verifies the opt-in flag and rejects non-booleans.

`node tests/workspace-browser.cjs` replays all three published workspace journeys (narrow, large text, keyboard), then the DashQ adapter at 1024/16 light and 360/20 dark. It checks refresh/navigation/outage preservation, lost reply and identical-request reconciliation through a native-session receipt, task details and horizontal fit. These checks use invented boards and native sessions, never a forge write or real runtime launch. The candidate gate remains `cargo test --locked` and `cargo run -- selftest`; merge-result CI qualifies publication (R12, TaskQ § 7).

The owner answer on #186 moves A7 live test-board/readback qualification to [#195](https://github.com/taskqorg/dashq/issues/195). Production `pm_requests` remains absent until that check passes and the owner approves configuration. No production grant, config or host was changed here.

Rollback: unset `pm_requests` or revoke the workspace/host PM grant. This stops new exchange/delivery; it does not undo board changes, erase operation identities or stop native sessions already launched. Older binaries leave the new tables intact. Re-enabling reads the existing identities rather than replaying native launch.
