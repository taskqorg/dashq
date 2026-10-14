# Connections #60 evidence

Synthetic installations rendered through the actual enrollment routes and dashboard renderer; no production access or grants. The accepted reference is `connections-reference.html` at `03967eca6cbc3a9b181a7da9f76d434609e5b175` (unchanged here). Screenshots use Chrome 155.0.8059.41 on macOS.

| Acceptance | Observed evidence |
|---|---|
| A1 | A fresh durable receipt and stale producer observation appear separately. Unknown export stays unknown; expired export gaps do not hide snapshots. CPU warmup and resource sample counts come from the resource projection, not the snapshot counter. |
| A2 | `connections_lifecycle_reference_chain` follows approval, activation, timeout, same-event retry, matching-generation projection, revoke, reapproval and separate lost identity. `connections_offline_revoke_host_and_server_chain` uses the pinned coreq client against the real handlers/store: local sending stops offline, the revoke-only request remains pending, acknowledgement survives restart, and reapproval keeps one identity. |
| A3 | Browser checks rendered 8 width/font/theme combinations and 26 lifecycle screens. Selection, disclosure, focus, draft/caret and scroll survive two refreshes and a failed read followed by recovery. No horizontal overflow in Connections. Deep links select the matching installation. |

Before/after: the same lifecycle check on baseline `b9d8bd2` fails because approval says the installation is connected before activation/readback. The updated chain passes. The existing 14 enrollment tests also passed unchanged; the two new chains join `cargo test --locked`.

Browser evidence uses a temporary local HTTP harness serving generated HTML plus the actual CSS/JavaScript. Refresh receives new server-rendered markup; an instrumented unsent textarea checks draft/caret retention. This is browser rendering/refresh evidence, not a live product enrollment or an implemented editor. The generated pages can be reproduced with `DASHQ_CONNECTION_EVIDENCE=DIR cargo test --locked connections_` and served with `assets/style.css` and `assets/app.js` at their normal routes.

| Viewport / text | Light | Dark |
|---|---|---|
| 736 / 16 | [image](connections-736-16-light.png) | [image](connections-736-16-dark.png) |
| 736 / 20 | [image](connections-736-20-light.png) | [image](connections-736-20-dark.png) |
| 360 / 16 | [image](connections-360-16-light.png) | [image](connections-360-16-dark.png) |
| 360 / 20 | [image](connections-360-20-light.png) | [image](connections-360-20-dark.png) |

Lifecycle examples at 360 / 20 dark: [entry](entry-360-20-dark.png), [approval](approval-360-20-dark.png), [separate capability approval](approval-capability-360-20-dark.png), [activation](activation-360-20-dark.png), [first snapshot](snapshot-360-20-dark.png), [timeout/retry](timeout-360-20-dark.png), [stale producer](stale-360-20-dark.png), [revocation review](revoke-360-20-dark.png), [revoked/reconnect](revoked-360-20-dark.png), [no recent receipt](no-contact-360-20-dark.png), [export gap](export-gap-360-20-dark.png). The list includes two same-label AgentQ identities, plus independent ServerQ and TaskQ rows.

Visual inspection compared the three-column list, selected detail below the list, narrow reflow, large text, both palettes, approval facts and lifecycle actions to the reference. Long labels and scope/evidence wrap without clipping. Task fit: L1-L3 UI behavior is represented; owner visual acceptance and L4 release qualification remain separate. Implementation review: machine routes, grants, CSRF, workspace filtering and database schema are unchanged; added projection fields read existing scoped records. Rebased onto coreq 0.11.0: its optional request fields are explicit in the client test, and separately approved operation capabilities are displayed apart from observation authority.

Not deployed. Live product services, forward-auth, native Windows/WSL/Linux enrollment, restart/outage recovery on real hosts and forwarding remain under #4 / E1-E10. Absence of export gaps does not qualify forwarding. Offline pending revocation is a host state per the #60 owner answer, never an inferred DashQ state.
