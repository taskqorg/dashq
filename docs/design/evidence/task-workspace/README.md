# Task workspace fixture evidence (#186)

R13 / D10: eight TaskQ v4.6.34 release assets verified against SHA256SUMS, with the tag commit and hashes in `assets/taskq/workspace-provenance.json`. No vendor asset was edited.

R12 / Testing: `node tests/workspace-browser.cjs --capture` passed on macOS arm64, Playwright 1.56.1 / Chromium 141.0.7390.37. It replayed the published narrow (320/16), large-text (360/20) and keyboard journeys, then DashQ's adapter at 1024/16 light and 360/20 dark. These are invented observations, requests and native sessions. No real board or native runtime was written.

- Before: the embedding overflowed at 1024 px (content right edge 1040 px). TaskQ's native fit set `body` to 100%, excluding DashQ padding. After: both adapter sizes passed the unchanged horizontal-fit assertion with border-box sizing.
- Before: substituting the old main/boards detach behavior into the same browser journey failed `detail focus survives refresh` while Stop in task detail was focused. After: the connected workspace passed that assertion, visible keyboard focus, PM draft/selection, detail selection, navigation and outage preservation.
- The lost-response journey posted the identical PM input twice, including actor, op-id, target and revision; the second receipt named the invented native session. A subsequent board observation added the PM-created task without another creation adapter.
- `cargo test --locked pm_request`: two adapter/store checks passed, including separate authority, denial/expiry, stale targets, actor/generation/revision fencing, full storage, changed-input rejection, foreign receipt, restart, text clearing and revocation. These checks substitute the host exchange and do not prove native launch.

The single [reviewed screenshot](workspace.png) shows the implemented workspace and shared PM receipt from this synthetic browser journey. It is layout evidence, not owner acceptance or production qualification. A7's real PM request/readback remains in #195 per the owner's answer on #186; production PM stays off. Canonical candidate checks and CI are recorded in the PR/result rather than inferred from this image.

Input SHA-256:

- `assets/workspace.js` `75f870f5625b1d093157f5a72463e6c2bc4262a199480019c1bd24a5563eb7a5`
- `assets/workspace.css` `bbee0d56a9fda1ac06792c8e1fe3a3c20e0f31e9a782cdca1ff8b2ad7c9b2ce2`
- `assets/app.js` `2f1a5a697fb52c9608b3572f5cee87526489aa322691925a508677eed577783a`
- `tests/workspace-browser.cjs` `db4664949ebc64980258f7164b17231c1563c4ba4d1fb7ed0aa1f34a23e783fb`
