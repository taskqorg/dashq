# Task controls

DashQ embeds TaskQ v4.6.30's `taskq.task-controls/1` renderer unchanged and transports only its start, stop, later and retry operations. [D13](../../dashq.md#d13-taskq-answer-card) authorizes this narrow exception to D1/D2/D7. Close remains a board link; answers and resource policies retain their existing authority.

## Authority and route

Off by default. Protected deployment configuration may set `task_controls: {"token": "<absolute credential path>", "boards": {"<owner/repo>": "<workspace>"}}`, with an absolute pinned `taskq.binary` and each board in `projects`. The credential is a dedicated owner-provisioned forge token in a mode-0600 file, trusted by that board and limited to the required repositories and canonical write/readback permissions. It is never the read token. Missing configuration returns 503; missing or unreadable credentials cannot enable controls.

The host administrator grants or revokes `dashq controller WORKSPACE SUBJECT on|off`. Every request requires the guard-verified `X-Dashq-Subject` to administer the configured workspace and hold this separate controller grant. An answerer, observation credential or administrator without the grant has no control authority. Revocation is checked on every request, including delivery checks. The existing D11 proxy subject boundary applies; no machine-route exemption or new identity system is added.

- `GET /task/control?board=<owner/repo>` returns a CSRF token only for that actor/board. It is bound to the actor, route, board and day, accepting today or yesterday like `/answer`.
- `POST /task/control` takes form fields `board`, `csrf`, `task`, `revision`, `op_id`, `action`. Other fields, duplicates, unknown actions and Close are refused. The same-origin HTTPS Origin and form content type are mandatory. The existing 4096-byte body bound and combined ten-write-attempts-per-minute actor limit apply.
- The server reads a fresh complete canonical task row with the dedicated credential before a new operation. It requires the supplied revision and the exact action in that row's `controls`; it never derives offers from state. Labels, missing revision, unsupported/incomplete snapshots and unavailable adapters cannot authorize a write.
- The pinned fixture supplies the entire command and fixed reason. The scratch TaskQ runner passes argument arrays, no HOME and a private scratch token, with no shell or host command authority. Every dispatch carries `--json`, `--op-id dashq.control.<op_id>` and `--revision <row revision>`.

The existing SQLite operation journal is reused with a distinct namespace and a payload digest bound to actor, workspace, board, task, revision and action. A new control is checked before a pending dispatch identity is committed, so a failed attempt to record a refusal cannot bypass the row gate on retry. A stored terminal receipt repeats without dispatch. A concurrent request waits; an unknown or restart-pending operation reconciles only through TaskQ with its original ID, revision and payload. A changed payload with the same ID is refused. No extra scheduler or operation service exists.

## Presentation and receipts

`/data` adds the canonical `boards[].tasks[].controls`, or null when unavailable. The browser uses the unmodified released control renderer and `op()` builder. A disabled outer fieldset names the missing actor/source/revision capability; only listed controls appear. TaskQ v4.6.30 removes the Closed counter, so an explicit `Closed on board` link preserves access without fabricating its total.

The adapter shows Sending, accepted, stale, rejected and unknown with receipt reasons. An accepted/stale operation waits for a changed board revision before another action; counters change only from observations. Unknown delivery survives refresh and view navigation in the page session. `Check delivery` repeats exactly the original operation; even a refusal during reconciliation keeps that operation uncertain and cannot create a fresh ID. A reload does not restore local operation state; server and TaskQ journals retain canonical deduplication for a supplied ID. Production qualification must include the native TaskQ readback when a browser session is lost.

## Qualification and rollback

Owner decision in #179 separates live qualification into held [#195](https://github.com/taskqorg/dashq/issues/195). Production `task_controls` must remain absent until #195 passes and the owner approves configuration. No production config, credentials or board were changed by #179.

The [#195 test-only real-board evidence](evidence/task-controls-live-195.md) records an accepted control, same-ID replay and non-mutating denials using the owner-authorized existing PAT and a synthetic host-local actor. Production enablement still requires explicit owner approval; browser forward-auth and native macOS are not qualified by that container run. The owner moved #195 A5 (PM composer/request readback) to [#186 A3/A7](https://github.com/taskqorg/dashq/issues/186); #195 adds no PM adapter.

`assets/taskq/provenance.json` pins version v4.6.30, source commit and release asset SHA-256 hashes verified against that release's `SHA256SUMS`. `node tests/boards.cjs` runs the answer, project and task-control fixtures over those unmodified browser bundles. `cargo test --locked task_controls` checks release argv, separate authority, missing gate, fresh row offers, stale/refused/foreign receipts, durable replay and same-ID reconciliation after restart. The existing answer tests retain their authority/draft/idempotency protection. `dashq selftest` also validates the task-control fixture's argv without writing.

`node tests/task-controls-browser.cjs` uses the existing Playwright runner for off/denied capabilities, exact offers, op-id/revision, lost reply and same-ID delivery checks, stale state, refresh preservation and narrow reflow. Its synthetic board never writes to a forge. [Fixture screenshots](evidence/task-controls/) show 1024/16 light and 360/20 dark; they are layout evidence, not live qualification. Required project candidate checks remain `cargo test --locked` and `cargo run -- selftest`; merge-result CI is the publication gate (R12 / TaskQ § 7).

Rollback: remove `task_controls` and restart, or revoke one controller grant immediately. This disables further dispatch; it does not undo TaskQ transitions or erase receipts. Existing answers and observation ingestion retain their previous authority.
