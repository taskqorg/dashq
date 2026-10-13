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

## Gates

Shared component and TaskQ tray adapter qualify together. DashQ display uses the canonical board snapshot adapter; authenticated answers additionally require the browser identity/store boundary and the updated answer-route task. Reading questions and answering them are separate capabilities; do not block observation-only enrollment on this feature.

Before enabling production writes, qualify an authorized test task on the real board: option and free-text paths, matching receipt/readback, lost response without duplicate entry, stale question, revoked identity and cross-workspace denial. Preserve unrelated tasks. Record exact component/TaskQ/DashQ versions and guard configuration. UI fixtures alone do not pass this gate.
