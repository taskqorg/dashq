# Shareable dashboard views (#137)

D22 adopts specq § F7. `/` keeps the combined overview. The six detail paths select existing sections without remounting their controls. This evidence uses the existing D19 recovery fixture: six configured projects, one fresh complete board, unavailable/partial sources and long labels. No remote settings or deployment changed.

## Evidence

- [Desktop overview](overview-1024-light.png), [desktop projects](projects-1024-light.png): Chrome on macOS, 1024 × 768 CSS px, light, 16 px main text.
- [Narrow overview](overview-360-dark.png), [narrow projects](projects-360-dark.png): 360 × 768 CSS px, dark, 20 px main text. Projects images capture the full scrollable page; overview images capture the viewport.
- Both configurations use reduced motion and the existing system font stack. The images show synthetic fixture observations, not live fleet qualification. The fixture denies answer access; this is not a production permission receipt.
- `SHA256SUMS` identifies the rendered browser assets, fixture and images. The enclosing commit identifies the implementation and test revision. Shared TaskQ assets and their checksum checks remain unchanged.

## Verification

Before: `cargo test --locked routes_and_headers` failed because `/projects` returned 404. After: all six direct paths return HTML with their selected view; unsupported paths retain 404 with an overview link, and invalid filters retain 400.

`tests/startup-browser.cjs` reuses the D19 fixtures and runner. It covers tab changes, Back/Forward, filtered direct/shared entry and reload, unchanged board projection, retained draft/disclosures/card identity, focused tab links through automatic refresh, explicit partial counts, narrow overflow and no writes. Existing outage/recovery checks still cover three consecutive automatic refreshes, advancing producer ages and retained state. `tests/boards.cjs` retains counter order, unknown/zero/partial semantics, immutable assets and revision-bound answer protection.

The local canonical Rust run could not create the Chrome CLI snapshot (`snapshot: no page.png (exit status: 1)`): sandbox-blocked, not PASS. Browser interaction/screenshot checks use installed Chrome through Playwright. R12 requires green CI on the exact result SHA for the blocked host check. `selftest` passes its fixture; with no config, source reads are explicitly skipped.

Visual inspection checks navigation wrapping and selected labels, accessible readable counters, all six project identities and explicit coverage, with no horizontal overflow. Owner visual acceptance and live-source qualification remain open; these fixture images do not establish either.
