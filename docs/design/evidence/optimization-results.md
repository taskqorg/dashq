# Results fixture and browser qualification

Task #67. The owner explicitly deferred P3 live local-stats/export/guarded-dashboard qualification to a separate authorized task. No production access, deployment, profile changes or test telemetry writes were performed.

The consumer uses the pure AgentQ aggregate from `taskqorg/agentq@f69a781706c0d2f9d60ac456651c920d3fe1bd1a`, with its MIT notice and unchanged 29-case fixture. Only the pure aggregate and its conformance tests are retained; producer filesystem and export readers are not copied. `accounting::tests::consumer_fixtures` checks every expected outcome, reason, deduplication, byte pair, cohort, coverage and rejection. `optimization::tests::observations_to_results` qualifies the Langfuse metadata boundary, privacy allowlist, source time, failed reads, original event buckets and period/host/client/session filtering. Producer `claude` maps to DashQ's existing `claude-code` query value without changing producer cohorts.

Browser: Chrome 155.0.8059.41 on macOS. The production server renderer and `assets/app.js` ran against published synthetic accounting cases through a loopback fixture server. The check covers 1024px/16px light and 360px/20px dark, reduced motion, three refreshes with changed counts, selected outcome, open disclosures, keyboard focus, scroll, text selection, period/client navigation, a selected missing client and error/pass reasons. No page exceptions or horizontal overflow. These are adapter/browser checks, not live source qualification.

The inspected layout preserves the pinned reference's outcome strip, composition bar, common-scale event history, paired volume beside history on desktop, reflow on narrow screens, client/recipe rows and supporting disclosures. Byte pairs are displayed separately by native RTK and headroom recipe; small measurements use exact bytes instead of rounding to zero MiB. The 1970 dates and small counts below are upstream fixtures, not live data or scaled reference counts. Whole-task effect remains unknown: the published schema supplies no comparable experiment cohort. No experiment or monetary savings is inferred.

- [1024px / 16px light](optimization-results/1024-16-light.png)
- [360px / 20px dark](optimization-results/360-20-dark.png)
- [Complete empty window](optimization-results/360-valid_zero.png)
- [Missing source](optimization-results/360-missing_source_is_unknown.png)
- [Partial source and missing buckets](optimization-results/360-partial_source_is_lower_bound.png)
- [Reason disclosure](optimization-results/360-pass_through_and_failure_are_distinct.png)

To reproduce the development browser proof from the repository root (Chrome required; Playwright is a temporary test tool, not a product dependency):

```sh
npm install --prefix /tmp/dashq-67-browser --no-audit --no-fund playwright
DASHQ_RESULTS_PROOF=/tmp/dashq-67-browser/proof cargo test --locked optimization::tests
PLAYWRIGHT_MODULE=/tmp/dashq-67-browser/node_modules/playwright node docs/design/optimization-results-check.cjs
```

Canonical checks remain `cargo test --locked` and `cargo run -- selftest`. D4.1/D7/D17 are updated with the implementation; the existing `fixture/data.json` and `fixture/page.html` goldens are refreshed for the added Results projection and section, without weakening their comparisons.
