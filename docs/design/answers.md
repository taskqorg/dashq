# TaskQ answers in DashQ

Current combined visual authority: [published first-screen reference and pinned TaskQ sources](first-screen.md#published-reference). Older previews do not override it.

Accepted under [D13](../../dashq.md#d13-taskq-answer-card). [TaskQ shared answer card](https://github.com/taskqorg/taskq/blob/main/docs/design/answer-card.md) owns presentation, interaction, assets and fixtures. Reuse it; do not fork it. [Enrollment](enrollment.md) continues to authorize observation only.

Composition follows [first-screen F-A1–F-A5](first-screen.md); compact blocks inherit [TaskQ B1–B4](https://github.com/taskqorg/taskq/blob/main/docs/design/shared-blocks.md). The dashboard shell may place or disclose the shared component, never fork its internal layout/icons or remove required controls.

## Placement

Open the shared card directly under its project/task in Boards. Keep the linked board icon, aligned counts and charts. Attention links to the same card, not a duplicate editor. Retain expansion, drafts, focus and scroll on automatic refresh. After acceptance retain the local receipt; update counters only from the new authoritative snapshot. Never claim worker resumption from answer acceptance.

Read-only installations render the same card with a precise unavailable reason and the issue link. TaskQ choices, recommendations and free text are not reworded. The approved demo is in TaskQ's design package; it is not real task data or proof of an enabled write path.

## Browser action adapter

- Only guarded same-origin `POST /answer`. Verify the existing provider subject and workspace administrator mapping, board membership and explicit answer capability on every request; deny unmapped, expired, revoked or wrong-workspace actors. Machine/read/telemetry credentials cannot authorize it. No public API or host command channel.
- CSRF token bound to the browser session plus Origin validation; reject cross-site requests. Preserve loopback listener and narrow proxy rules. CSP allows reviewed same-origin forms only. Bound body to 4 KiB and answers to 10/min per actor; text follows TaskQ § 5 limits.
- Accept board/task identity, exact card revision, operation ID and exactly one of source choice ID or free text. Resolve board configuration server-side; no client-selected executable, host, credential or arbitrary command. Re-read with canonical TaskQ code and reject stale/changed/non-answerable cards before any mutation.
- Call the pinned TaskQ dispatcher with argument arrays and `--revision`, `--op-id`, `--json`; no shell, custom comment writer or copied state machine. TaskQ interprets and validates options. Choices that imply close/publication remain unavailable with a reason and link. No start, retry, requeue, repair or capacity operation.
- A dedicated server-side forge credential has the minimum supported permission needed by TaskQ's canonical answer write/readback, limited to configured repositories. The author must be trusted by the board. Never promise a comment-only scope the provider does not offer; never reuse an owner's admin credential. Credential provisioning stays in protected deployment setup, never in the browser, telemetry, repository or chat.
- Persist the verified actor, scoped target, immutable operation identity and sanitized receipt with the existing DashQ store. Retain pending/unknown operations for reconciliation; never silently prune an unresolved mutation. Do not log credentials or free-text answers into telemetry. TaskQ remains the authoritative operation/result store.
- Return the TaskQ receipt. Accepted means board readback; stale preserves the draft and requires reviewing the new question. Rejected shows reason/next. Unknown performs read-only reconciliation by the same operation ID; a retry uses that ID and payload, never creates a fresh answer blindly. Incomplete source data disables writing.

## Route (#41)

Implemented transport and authority; [#43](https://github.com/taskqorg/dashq/issues/43) embeds the card, [#44](https://github.com/taskqorg/dashq/issues/44) qualifies the real board before any production deployment sets `answer`.

- Off unless config has `answer: {"token": "<absolute path>", "boards": {"<owner/repo>": "<workspace>"}}` with `taskq.binary`; each board must be in `projects`. The token file is a dedicated forge credential (mode 0600), read per write and handed to taskq only through its scratch install, like the read path (adapters.md S3). Absent: `/answer` answers 503.
- Actor: the guard's subject (`X-Dashq-Subject`), a workspace administrator of the board's workspace, holding the answer capability that only the host administrator grants or removes: `dashq answerer WORKSPACE SUBJECT on|off`. Read from the store on every request. Bearer, device and read credentials are not identities here.
- `GET /answer?board=` returns `{csrf}`: HMAC of subject, route, board and UTC day (valid that day and the next). `POST /answer` takes a form (`application/x-www-form-urlencoded`, `Origin: https://<public_host>`, at most 4 KiB, no repeated field): `board`, `csrf`, `task`, `revision`, `op_id` (identifier, ≤ 64), and exactly one of `choice` (1–99) or `text` (1–4 lines ≤ 240 characters, no control characters, not starting with `-`). Ten answers a minute per actor.
- A new operation first re-reads the board with `taskq status --json` (fresh, no cache): the task must be `ask` or `review` at that revision, the choice must exist on a parsed card and not start with `close`, free text only for `ask`; an incomplete read refuses. Then `taskq answer N.K|N [--text T] --revision R --op-id dashq.<op_id> --json` runs once, no shell. `/data` choices carry `unavailable` with the reason for close options.
- The store records op ID, actor, workspace, board, task, revision, payload SHA-256 (never the text), state and the sanitized receipt (`task, op_id, outcome, state, revision, reason, next`; strings single-line ≤ 240). Records are never pruned. The same op ID with the same payload repeats a final receipt without a write; with another payload or actor it is refused. `pending` after a restart or `unknown` reconciles by rerunning the same command (same ID, payload, revision) without a fresh pre-read; TaskQ returns the first receipt or refuses a moved revision. A second request for an op ID in flight answers `unknown` without a run.
- Statuses: accepted 200, stale 409 (with the current revision; the client keeps its draft), rejected 422, unknown 202; refusals before a write use the same shape with `outcome: rejected`.

## Gates

Shared component and TaskQ tray adapter qualify together. DashQ display uses the canonical board snapshot adapter; authenticated answers additionally require the browser identity/store boundary and the updated answer-route task. Reading questions and answering them are separate capabilities; do not block observation-only enrollment on this feature.

Before enabling production writes, qualify an authorized test task on the real board: option and free-text paths, matching receipt/readback, lost response without duplicate entry, stale question, revoked identity and cross-workspace denial. Preserve unrelated tasks. Record exact component/TaskQ/DashQ versions and guard configuration. UI fixtures alone do not pass this gate.

## Embedded consumer (#43)

The browser loads the unchanged answer-card and project-blocks assets from the same-origin app/style responses. `assets/taskq/provenance.json` records the owner-approved source commit and SHA-256 per file, not a published release. `boards.tasks` preserves canonical row/question identity, choices, evidence and limits. Label-only rows have no parsed question and no answer capability. Source freshness remains beside each block. Navigation counters filter observed rows; Closed remains unknown.

Attention opens the single Boards card. The board container and component instances survive page refresh; the canonical component owns revision-bound drafts. This consumer always supplies `answer: false` until #44 qualifies the real adapter; no tray management capability or answer request is added. Static `--snapshot` remains script-free with source and issue links; browser evidence is required for the shared UI.

Checks: `node tests/boards.cjs` (in `cargo test`) replays the shared card fixtures, verifies the pinned checksums and the counter order, zero hiding, unknown `?`, quiet Closed and partial aggregate. `tests/boards-browser.cjs` (Chrome + Playwright, release/visual check) asserts the same counters in the page, provider icon contrast, Attention navigation, counter filtering and three refreshes keeping focus, scroll, disclosures and card instances; its [screenshots](evidence/boards/) cover 1024/360 px at 16/20 px in both themes and a narrow long-text fixture.
