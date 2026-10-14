# Service versions view qualification

Task #72 implements D18 V1–V4 from specification revision `c9f83071ab6a9bfe4534a93a50f7e2de6d182757`.
The reference HTML and sources manifest remain unchanged from that revision. Reference SHA-256:
`c07667794ebc27f92c11fd71cafd9540bbbeb56e76e9e21ac7eaece6a81dcfaa`.

The production renderer and stylesheet use its inventory columns, responsive three-column version row,
contextual detail and two-column update/diagnostic layout. Demo selectors are omitted.

- [1024 px / 16 px, light](service-versions-1024-16-light.png)
- [360 px / 20 px, dark](service-versions-360-20-dark.png)

These images show synthetic `fixture/versions.json` input through `data::build` and the production page
renderer. The browser probe changes observed version 0.4.16 to 0.4.15 to demonstrate refresh, then selects
the stale WSL installation and expands its evidence. Windows remains a separate row. Images were visually
compared with the pinned reference at both sizes; rows, selected detail and large text remain legible.

A local Chromium 156 / Playwright 1.64.0 probe served the generated HTML with production `assets/app.js`
and `assets/style.css`. At both sizes it exercised repeated scheduled 60-second refreshes (virtual clock),
changed values, an HTTP 503 and recovery. Assertions passed for selected installation/workspace, focused
row, scroll offset, open evidence, exactly one visible detail, no horizontal overflow and no script errors.
This is bounded browser evidence, not wall-clock or live-source qualification.

The retained chain `cargo test --locked service_versions_render_projected_evidence` checks every producer
fixture case reaches the view, unknown lifecycle/health and last-known versions, separate observation and
release times, reported engines and queue age, HTML escaping, omission of arbitrary diagnostic fields and
absence of lifecycle/demo controls. `fixture/page.html` changes only to add the D18 section.

Producer limitations from #70 remain: no health receipt, waiting/startup/checksum/rollback stage, update
history, safe reason code or coreq version is currently supplied. These fields read unknown/not reported;
synthetic reference receipts are never promoted into production facts. No real update, restart, rollback,
raw export or production deployment was performed. Owner visual acceptance remains separate.
