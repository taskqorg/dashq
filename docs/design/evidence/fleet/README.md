# Fleet evidence (#244)

D26 implements the [owner request](https://github.com/taskqorg/dashq/issues/244) and selected hourly-rate/upstream-latest option. R1/R7 keep content and credentials out; R2/D6 preserve unknowns; D11 scopes stored observations; D18 adds no update or restart authority.

## Scenarios and preservation

| Acceptance | Implementation and executable evidence |
|---|---|
| A1 | D3 and documented config examples name pds18.ufobe.com; tracked contract/config-example search finds no former production host. This changes no deployment. |
| A2 | `enroll::tests::fleet_through_ingest_and_scoped_projection`: real CoreQ pairing/ingestion, durable replay and scoped projection into `/data.fleet` and the page. `fleet_usage_windows_and_cadence` protects 24 h/7 d project/attempt/account grouping, duplicate IDs, stage exclusion, future/untimed rows and missing token splits. |
| A3 | Known percent/reset and missing/invalid fields in the model check; account row keeps time, reason and limit window. Current snapshots expire after 300 s or their reset; retained observations stay visible. |
| A4 | Released TaskQ `transition` to `closed` and successful `worker` operation `spawn`; distinct stable IDs count once. Producer host beats installation-label fallback. Fixed-window completions/hour is an observed lower bound when coverage is incomplete. |
| A5 | `fleet_versions_keep_both_source_states` protects parsed CLI versions and separate heartbeat/upstream freshness; stale/failed comparisons stay null. Browser fixture includes known drift and an unknown CLI. |
| A6 | Existing destinations and controls are preserved. Fleet issues no mutation requests; D18 remains binding. |

`/fleet` has the existing navigation, URL host scope, source summary, refresh, keyboard/disclosure behavior and no-script server rendering. Its two windows ignore the model-activity period and agent/session filters. Installation/workspace coverage remains separate inside each visual host group; no hostname creates a new grant identity.

## Evidence

- [Synthetic fixture screenshot](fleet.png): actual Rust HTML and shared CSS, Windows Chromium, 1280 x 1000 viewport, system font, light theme, 16 px root, reduced motion. All observations and accounts are invented. This is review material, not owner visual acceptance.
- `tests/fleet-browser.cjs`: eight cells (1280/360 px, light/dark, 16/24 px). Checks route visibility, selected navigation, usage and reason text, overflow, disclosure preservation, failed-refresh retention/unconfirmed state and zero writes. CI runs it after generating the fixture through real D11 ingestion.
- Before: worker-start check failed (`null` instead of 1) because the initial reader expected `worker.spawn`; after matching the released `spawn` operation, the same check passes.
- Before: a telemetry-only host had no CLI rows (`[]` instead of claude and codex unknown); a capped read kept the oldest rows and showed limit 42 as current while a newer turn was dropped; refresh lost summary focus (active id `''`). After: `fleet_telemetry_only_host_shows_cli_unknowns`, `fleet_capped_limits_are_not_current`, `fleet_cap_keeps_newest_rows` and the browser focus assertion pass.
- Before: browser check found horizontal overflow on Fleet; after applying the existing page sizing pattern, all eight cells pass. Existing fixture pins were regenerated for the additive Fleet output and navigation; their assertions remain unchanged.

The CoreQ library prerequisite is published annotated tag `v0.17.0`, tag object `0a5d5aa8c60488a4af286740ca379e8b0a8286f6`, commit `7e196edd4e93ad2f3861d6ec6ed35d3216f92e3d` (CoreQ #107/#108). TaskQ release and immutable producer schema evidence are in [#247's report](../../../research/247.md). CoreQ is a library tag; GitHub has no release object for it. DashQ consumes observation data without changing its runtime dependency.

## Remaining qualification

No production host was accessed, configured, updated, restarted or deployed. Full 24 h/7 d producer coverage, per-account native-limit availability, live D11 delivery on every host and owner visual acceptance remain unverified. Without a whole-window producer baseline Fleet reports observed/partial counts, never complete totals or healthy empty data. The canonical project gates and merge-result CI are recorded in the PR/task result, separately from these model/adapter/browser observations.
