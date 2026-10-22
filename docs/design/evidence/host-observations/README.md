# TaskQ host observations (#178)

D8 and R13 consume published TaskQ v4.6.30 host observations without enabling actions, changing enrollment or deploying. The approved public redesign package is copied from `4227606a205fc6fada4e90b195865ae5aa6ec859`; D8/D10 record its next target, and D12 records GitHub's public visibility without changing settings.

## Contract and retained protection

`fixture/taskq/host-observations.json` preserves the exact upstream `tests/fixtures/host-observations.json` bytes; its source and SHA-256 are in the adjacent provenance file. `taskq_host_observations_released_cases` reads every released case. A3 is deliberately stricter than the producer's partial display: incomplete online/login/queue values become unknown, counts null, retaining the producer's reason. Missing kinds, expiry, invalid/future time and equal-time conflicts cannot establish online, logged-in or idle state. Board/workspace/installation, producer, host and original validity times remain in the additive schema 2 projection.

H2's existing heartbeat machine identity joins host labels; no alias map or hostname-derived producer identity is introduced. Reports from several installations coalesce per board/workspace/host/kind; unrelated boards are not summed. Revoked and previous-generation snapshots provide no current observations. Host filtering happens after the canonical join so a sender's observations for another host are retained, within the authenticated workspace scope.

The existing enrollment chain now sends observations through ingestion and SQLite, reads them through `/data`, compares the host fields with the `App::doc` path used by `dashq report`, and verifies absent browser authority sees none. Existing metadata rejection, receipts and unknown-schema checks remain. No live producer or native CLI delivery claim follows from this synthetic chain.

## Failure and correction

- Before: `cargo test --locked taskq_host_observations_released_cases` failed with `observation host must exist`. After: the same test passes all upstream cases; identity/freshness coverage adds expiry-boundary, duplicate, conflict, revocation and previous-generation checks.
- The additive field required updating the Hosts golden fixture, preserving every H1–H7 identity assertion. The page fixture now carries an explicitly invented TaskQ observation. The version outage check now scopes its GitHub failure assertion to heartbeat provenance and separately verifies independent enrollment remains fresh; it retains last-known versions and unknown update protection (Testing § Before adding or changing a test; § When evidence disagrees).
- Before: `snapshot_writes_html_and_png` failed with `snapshot: no page.png (exit status: 1)`; Chrome reported it could not create its default headless profile. After: the unchanged PNG/HTML check passes with a fresh owned temporary profile, removed after the renderer exits. No existing profile is removed.
- Visual inspection found evidence expanded inside a narrow identity column. Evidence now spans the host row in labelled groups; expiry uses its future UTC time without the misleading `0s ago` suffix.
- Review correction (D5/D6/D19): before, `node tests/host-observations-browser.cjs` failed `D5/D6: failed refresh retains unexpired values as unconfirmed`: expected `logged in · unconfirmed`, observed `unknown`. After, the same check passes all eight cells. Failed refreshes retain unexpired values with a visible gray `unconfirmed` label and the original observation time in the title. Repeated failures neither duplicate the label nor restart validity; the timer still changes expired values to unknown, and recovery reads fresh evidence. Existing expiry, focus/disclosure and no-write protection remains (Testing § Before adding or changing a test; TaskQ § 7.4).

## Runtime pictures and browser checks

[Light](stored-light.png) and [dark](stored-dark.png) show the actual renderer after synthetic observations were durably ingested into the test SQLite store. The expanded evidence names producer, workspace, observation time and expiry. These are invented test hosts and login/queue states, not a production inventory. Both images were visually inspected on macOS / Chrome 155.0.8059.85.

`tests/host-observations-browser.cjs` verifies eight width/text/theme cells (1280/360 px, 16/20 px, light/dark), horizontal overflow, producer/expiry evidence, expiry without a fetch, retained unconfirmed values through repeated failed refreshes, expiry during an outage, recovery, focus/disclosure preservation and no writes. CI uses the existing Playwright installation. The original host-first browser chain retains its 24 layout/navigation cells and truthful missing-adapter states.

Reproduce after `cargo test --locked` exports the stored page with `DASHQ_HOST_OBSERVATIONS_EVIDENCE` set to a temporary directory, then run `node tests/host-observations-browser.cjs` with the same environment. Add `--capture` to update the two pictures. `DASHQ_CHROME` optionally selects the local Chrome executable; CI uses Playwright Chromium.

Publication acceptance remains green `merge_group` CI under R12 / TaskQ § 7. The worker opens the PR and does not wait for CI. Production source qualification, deployment, real logins, grant changes and repository-setting qualification remain unverified and outside this task.
