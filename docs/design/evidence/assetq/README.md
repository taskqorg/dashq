# AssetQ controls evidence (#246)

Evidence for [D27](../../../../dashq.md#d27-assetq-controls) and the [design package](../../assetq-controls.md). Each section names what it proves and what it leaves open.

## Scope of this evidence

| Acceptance | Executable evidence | Open |
|---|---|---|
| A1 | D27, R3, R4, the views and process detail and the design package precede the code in history. The owner chose full A3 and approved the [page layout](../../assetq-controls.md#approved-layout) on #246. | AssetQ's own R9 and method-evaluation text still say summary only. |
| A2 | `assetq_live_local_service` reads status, jobs, methods with qualification records, a planned comparison, its scoped recommendation read and objects from a real AssetQ. | Deployed service. |
| A3 | `assetq_actions_dispatch_once_with_verified_receipts`, `assetq_authority_stays_separate`, `assetq_exact_revisions_refuse_stale_actions`, `assetq_unknown_delivery_reconciles_without_a_second_effect`, `assetq_refusal_keeps_only_its_code`, `assetq_journal_holds_the_design_columns`, `assetq_route_gates`, and all four actions in the live check. | AssetQ has no conditional write, so a revision check narrows a race without closing it. |
| A4 | `assetq_projection_drops_content`, `assetq_unknowns_stay_explicit`, `assetq_capped_counts_read_at_least`, `assetq_failed_jobs_keep_their_reason`, `assetq_view_is_scoped_and_absent_without_config`. Links point at AssetQ's job, result, object, download and comparison records. | AssetQ's browser session for those links ([assetq#9](https://github.com/taskqorg/assetq/issues/9)). |
| A5 | `tests/assets-browser.cjs`, the local-service check below, the built-binary check below, and the unchanged existing suites. | `real configured service`: see [Not verified](#not-verified). |

## Real local service

`assetq_live_local_service` is ignored by default and needs a running service. It ran against AssetQ source `47f2cbd18c94bdbc6070696f767e6c66d8601640`, built unchanged, with a disposable PostgreSQL 18 cluster on loopback, on Windows, with the synthetic provider and no provider credential. No paid effect is possible on that method.

```sh
ASSETQ_BASE=http://127.0.0.1:18740 ASSETQ_PROJECT=<project> ASSETQ_MEMBER_TOKEN=<member> ASSETQ_OWNER_TOKEN=<owner> \
  cargo test --locked assetq_live_local_service -- --ignored --nocapture
```

The plain-HTTP base works only in test builds, as for the other in-process receivers. Observed, with invented project data:

- Read `ok`: real registry rows, `general dispatch refused` qualification records, unknown backup and restore. Neither prompt nor dataset case text reached `/data` or the page.
- `job.submit`: `accepted`, durable job, and the same job on a repeated operation.
- `job.cancel`: `accepted`, effect `requested`.
- `method.save`: `accepted`, next version `synthetic.fake@2`; the earlier revision then reads `stale`.
- `method.save` through a `member` credential: `refused`, code `forbidden`. AssetQ's role still decides.
- `recommendation.promote`: `accepted`, effect `promoted`, after two `refused` receipts with code `admission_busy` while AssetQ's executor held admission.
- A comparison reads `incomplete` with `job not terminal` and `usage unknown`, null credits and no winner.

This proves the adapter against AssetQ's real API and storage semantics. It does not prove a real provider, the managed store or a deployment. This run predates the failed-jobs read and the journal columns; the local service was not started again. [#259](https://github.com/taskqorg/dashq/issues/259) covers them against the real service.

## Defects found by these checks

- Before: every cancellation of a running job read `stale` against the real service, because AssetQ updates `updated_at` on each poll. The stub had hidden it. After: the job revision is `phase/step/effect/cancel`, and the same check reads `accepted`.
- Before: after the refresh a stale receipt triggers, the method form lost its receipt, because a field named `id` shadowed `form.id`, the key of drafts and receipts. After: the field is `method_id`, and the browser check waits through that refresh.

Review of that result found five more. The four Rust tests below failed first with the quoted result, then passed. The browser case was added with its fix.

- `selftest_reads_each_assetq_project`. Before: `projects: []`, reason `no AssetQ project in this actor's workspaces`, because selftest had no scope; D2 requires one read of each source. After: one project read, and its failure reads `read failed`.
- `assetq_capped_counts_read_at_least`. Before: a list with a further page showed an exact total. After: `≥N (capped)`, and a list whose read failed counts `— · unknown`, never 0.
- `assetq_failed_jobs_keep_their_reason`. Before: failed jobs existed only as a count, so the design's Failed state had no row. After: a closed `Failed jobs` disclosure shows AssetQ's first-line reason and the job record, with no cancellation and no result link.
- `assetq_journal_holds_the_design_columns`. Before: `assetq_ops` held `op_id, project, digest, state, receipt, baseline, at`, which could not name an actor, action or revision. After: it holds the design's columns, with a digest for the actor.
- `tests/assets-browser.cjs` not-configured case. Before: `/assets` without config read the generic `No observations available for this view.` After: it names the missing `assetq` key, also after a refresh, with no navigation entry and no panel.

## Browser and binary

- `tests/assets-browser.cjs`: eight cells (1280/360 px, light/dark, 16/24 px) on the actual Rust HTML with a stubbed control route. It checks visibility, selected navigation, unknown text, no content, no overflow, draft survival, one operation ID across `Check delivery`, a disabled submit while delivery is unknown, invalid JSON sending nothing, a stale receipt keeping its draft, failed-refresh retention, failed-job rows without an action, the not-configured page across a refresh and no enabled action without the script.
- Built binary on loopback, with an unreachable AssetQ origin and invented credential files: `/assetq/control` reads 401 without a subject and 405 for PUT; an actor reads `{"jobs":true,"methods":false}`; a POST reads 503 `assetq_unavailable` with no effect; a foreign Origin reads 401; `/data` holds no credential and reads `unknown` with its reason; actions read off; `/assets` and `dashq report` still answer.
- The forward-auth guard itself was not exercised; these requests set the guard's verified header directly.
- `dashq selftest` with an invented config and a closed port prints `assetq: unknown invented-project: read failed (curl exit 7)`.

## Not verified

A5 `real configured service`, A7 and A8 are not met by this evidence. Read-back for A6, from the Windows worker:

| Input | Read back | State |
|---|---|---|
| [assetq#25](https://github.com/taskqorg/assetq/issues/25) published at `https://assets.ufobe.com` | Issue closed. `GET /v1/health` with the agent Cloudflare Access credential: 200, `service: up`, `database: ok`, version 0.1.0. | Present |
| AssetQ R9 and `docs/method-evaluation.md` allow these actions | `main` R9 still reads summary metadata and links only. `docs/method-evaluation.md` still says neither integration receives generation authority. | Missing: owner item |
| `assetq` config, credential files and `dashq assetq-grant` rows on the target | `https://dash.ufobe.com` runs a release without this feature: `/assets` reads 404 and `/data` has no `assets` key. The Windows worker has no SSH access to the target, so the files are unknown. No agent created or changed any of them. | Unknown: owner item |
| DashQ can read `/v1` through the public origin | `GET /v1/status` with a Bearer value and no Access credential: 302 to the Cloudflare Access sign-in. The adapter sends only the Bearer token, so a deployed read would fail. With the Access credential and no Bearer token: 401, so AssetQ still decides. | Gap: owner choice |

Also open:

- A7 needs a build with `/assets` on the target. The latest release, v0.18.1, does not contain it.
- [assetq#42](https://github.com/taskqorg/assetq/issues/42) is moving the origin to `pipe.ufobe.com`, with a redirect from `assets.ufobe.com`. The adapter sends a Bearer token and reads with no redirect handling, so `base_url` must name the final origin.
- This branch's D27 number is taken on `main` by public limits push, and the branch conflicts there. The next requested rebase renumbers this section.

Production enablement needs those inputs and then the live check above through the real guard and HTTPS origin, with the `assetq` config, `read_token`, `control_token` and `dashq assetq-grant` set explicitly.
