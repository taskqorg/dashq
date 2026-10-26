# Scoped Reports (#221)

[Rendered Reports](reports.png) uses invented TaskQ stages and AgentQ model observations. D22/D24 and R13 define `/reports`; D21 keeps time host-scoped and model use retains its agent/session filters. No source, billing import or mutation authority is added. The accepted public reference supplies the two-column summary and narrow reflow; its illustrative time-allocation chart cannot be inferred from these measurements.

## Evidence

- A1: the same `agent_work.totals` serves `/data`, the browser and `dashq report`. It sums unique attempts before pagination. Wall time unions nested intervals within complete attempts; execution adds stage durations; ready-queue waits remain separate. Parallel attempts add. The 202-attempt fixture retains one open attempt, nested stages and a single recorded queue wait. Links preserve filters/cursor and reach the existing Agents stage inspector, including direct reload.
- A2: each measurement shows known/of and reasons; source states and per-host retained start remain visible, with an unknown end watermark. Model-source retention remains unknown. Estimates use the existing projection, separate EUR/USD subtotals and configured price dates, sources and rates. No actual-expense figure is displayed. Missing durations, calls or prices remain unknown.
- A3: `/reports` resolves through the existing guarded page router. Browser checks exercise filtered direct entry, paging, Back/focus, refresh/disclosures, outage/recovery, absent-host results and no writes. The existing stored-telemetry chain additionally compares browser and CLI totals.
- A4: [fidelity readback](fidelity.json) pins source inputs, rendered fixture, reference and browser version. Six Windows Chromium cells cover 1280/16, 360/20 and 1280/24 in light/dark, including no-script rendering. Independent review and canonical candidate checks are recorded in the PR/task result.

Before: the browser check against the unchanged baseline fixture failed `#221: scoped Reports view missing`. After: the projection check and six-cell browser check pass. Retained protection covers pre-pagination totals, nested-stage semantics, open/unknown measurements, currency separation, known/of, price basis, exact inspector links and preserved reading context. The HTML/JSON goldens change intentionally for the new route and additive totals; no protection is removed (Testing, Before adding or changing a test / When evidence disagrees).

Independent Claude review identified missing lower-bound labels and retained-source coverage. The added regression failed `#221: partial time must read as a lower bound` before correction and passes after. Incomplete denominators/status now say `at least`, partial/incomplete sums use `≥`, and incomplete/stale/unconfirmed fixtures retain their numbers with source reasons and times. The same correction pass preserves focus when a report link disappears, deduplicates retention notes, keeps tiny known estimates from rounding to zero and explains unconfigured measurements. The browser matrix covers these direct effects.

Final independent Claude verdict: task fit PASS; implementation quality PASS. Review was read-only and did not execute checks or inspect the image. Its final narrow correction check used source excerpts; the primary verified the UTF-8 separator and that retention notes are raw strings escaped once. Canonical gates and candidate SHA are recorded in the PR/task result.

## Reproduction and limits

Set `DASHQ_REPORTS_EVIDENCE` to a temporary directory. Run `cargo test --locked reports_view`, then `node tests/reports-browser.cjs` with Playwright 1.56.1 on `NODE_PATH`. `--capture` saves one review image. CI runs the same check beside the existing Agents checks; the project gates remain `cargo test --locked` and `cargo run -- selftest`.

These are fixture/model/adapter/browser observations, not live producer or production qualification. No deployment or owner visual acceptance is claimed. Publication acceptance remains green merge-result `merge_group` CI (R12; TaskQ § 7).
