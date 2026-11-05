# AssetQ summary evidence (#246)

Evidence for [D28](../../../../dashq.md#d28-assetq-summary) and the [design package](../../assetq-summary.md). Each section names what it proves and what it leaves open. All values are public-safe: status codes, states and invented data, with no credential or private path.

The owner withdrew A8 and the four write actions, so A6 needs no AssetQ R9 amendment and no grant rows. A7 stays read-only, and the owner allowed closing on source evidence when A7 needs the merged build on the target; the PM runs the live readback after the release.

## Scope of this evidence

| Acceptance | Executable evidence | Open |
|---|---|---|
| A1 | D28 and the design package precede the code in history. The owner approved the layout and then removed its actions. | None |
| A2 | `assetq_reads_carry_both_credentials_and_only_get` reads status, jobs, methods, objects, a comparison and its recommendations from an in-process service over real HTTP. An earlier adapter ran against a real local AssetQ, below. | The deployed service |
| A3 | Superseded by the owner's decision: DashQ has no write. `assetq_reads_carry_both_credentials_and_only_get` sees only GET, `assetq_page_evidence` finds no write control and `tests/assets-browser.cjs` sees no non-GET request. | None |
| A4 | `assetq_projection_drops_content`, `assetq_unknowns_stay_explicit`, `assetq_capped_counts_read_at_least`, `assetq_times_use_shared_format`, `assetq_date_only_time_shows_its_date`, `assetq_nested_green_needs_ok_source`, `assetq_failed_jobs_keep_their_reason`, `assetq_read_failures_name_their_reason`, `assetq_view_is_scoped_and_absent_without_config`. | The viewer's own session on the linked pages |
| A5 | `tests/assets-browser.cjs` and the other CI browser suites, restored to `main`'s versions. | `real configured service`: see [PM live readback](#pm-live-readback) |
| A6 | [Read-back](#read-back-for-a6) below. | Config and credential files on the target |
| A7 | `selftest_reads_each_assetq_project` proves that selftest reads the configured project. | The run on the target: see [PM live readback](#pm-live-readback) |
| A8 | Withdrawn by the owner. | None |

## Read-back for A6

The following table records the earlier Windows worker readback through the public origin, with the agent's Access credential where named. The fresh Pipe readback below supersedes its origin-move observation. No credential, grant or configuration was created or changed.

| Input | Observed | State |
|---|---|---|
| [assetq#25](https://github.com/taskqorg/assetq/issues/25) | Closed. `GET /v1/health` through Access: 200, `service: up`, `database: ok`, version 0.1.0. | Present |
| AssetQ R9 and method-evaluation | Both allow summary metadata and links, which is now all DashQ does. | Present |
| Machine read path | `GET /v1/status` with an invented Bearer value and no Access credential: 302 to the Access sign-in. With the Access credential and the same value: 401, so the request reached AssetQ, which refused it. | Present with a service token |
| Browser link paths | Through Access, `/assets`, `/jobs`, `/methods`, `/benchmarks`, `/operations` and the invented `/jobs/{id}`, `/assets/{id}`, `/methods/{id}/versions/1` and `/benchmarks/{id}` each read 200 HTML. `/browser/v1/objects/{id}/content` for an invented project reads 403 JSON. | Present |
| `assetq` config, credential files | The Windows worker has no SSH access to the target. `https://dash.ufobe.com` runs a release without this feature: `/assets` reads 404 and `/data` has no `assets` key. | Unknown: the PM provisions them |
| Origin move | `pipe.ufobe.com` does not answer yet; `assets.ufobe.com` does. [assetq#51](https://github.com/taskqorg/assetq/issues/51) holds the move. | See the [design](../../assetq-summary.md#provisioning) |

### Pipe readback, 2026-11-05 17:01 UTC

- `assetq#7` and `assetq#25` are closed. `assetq#51` remains open, but `GET https://pipe.ufobe.com/v1/health` through Access returned 200 with `service: up`, `database: ok`, version `0.1.0`.
- Pipe's current R9 and `docs/method-evaluation.md` allow DashQ summary metadata and links, with no generation authority. The compatibility `assetq.md` points to `pipe.md`; the PM's token command is now `pipe token <project> --role member`.
- `GET https://dash.ufobe.com/assets` through Access returned 404. A5 real configured service and A7 remain unverified: this feature needs the merged release and PM-provisioned project Bearer/config files. The worker created no configuration, credential or grant and made no write request.
- Every adapter URL and browser link derives from `assetq.base_url`. The configured-origin fixture uses an invented host, independent of either production host name.

## Real local service

An earlier revision of this adapter ran against AssetQ source `47f2cbd18c94bdbc6070696f767e6c66d8601640`, built unchanged, with a disposable PostgreSQL 18 cluster on loopback and the synthetic provider. It read real registry rows, `general dispatch refused` qualification records, unknown backup and restore, and an `incomplete` comparison with `job not terminal`, `usage unknown`, null credits and no winner. Neither prompt nor dataset case text reached `/data` or the page.

That run predates the failed-jobs read and the Access headers, and it used the since-removed actions. This Windows host now has no PostgreSQL binaries, so it was not repeated. AssetQ's service page calls its later status changes additive. The PM's readback covers the real service.

## Defects found by these checks

- D6/R2, review of `0ab0087`: a retained provider, integrity, backup or restore value stayed green under a source that was not `ok`. Before: `assetq_nested_green_needs_ok_source` failed with 4 green values under a stale source. After: 0 under stale, unconfirmed or incomplete, each value names its source state, and `tests/assets-browser.cjs` proves the same after a failed page refresh.
- M2, review of `0ab0087`: a real date-only qualification time read `? unavailable`. Before: `assetq_date_only_time_shows_its_date` failed with `? unavailable`. After: the page shows the exact date; invalid dates such as `2026-02-30` stay unavailable.
- R2/D6, review of `ac4bddb`: recommendation scopes showed an exact count after a failed or capped comparisons read. Before: `assetq_capped_counts_read_at_least` failed on the unknown-scopes assertion. After: scope counts use comparison coverage, retaining exact counts only for complete reads and lower bounds for pagination or skipped details; the same check passes.
- D6/specq Time, review of `ac4bddb`: producer times and source ages bypassed the shared format. Before: `assetq_times_use_shared_format` failed because none of the producer times used the common timestamp markup. After: jobs, provider/integrity/backup/restore observations, qualifications, recommendation scopes and records use shared relative/exact time, including PostgreSQL fractions and offsets; invalid times remain `? unavailable`. Durations and source ages use the shared duration formatter. The same check passes; browser checks cover JavaScript and server-only output.
- Before: a deployed read could not pass Access; the live origin answered a Bearer-only request with 302. After: `assetq_reads_carry_both_credentials_and_only_get` sends both credentials on every read, and without the token the reason is `redirected to a sign-in`. Protection kept: `assetq_read_failures_name_their_reason`.
- Before: page links named the machine `/v1` paths, which AssetQ's service page says take no browser session, so a person following one would read a refusal. After: links open the site's published browser paths, and `assetq_projection_drops_content` and `tests/assets-browser.cjs` fail on a machine path.
- Before: on `main`'s D4 rule, the view's read would have made a browser request wait for AssetQ. After: the read uses the background refresh, and `assetq_view_is_scoped_and_absent_without_config` reads `refresh in progress` on a cold cache.
- Earlier review findings stay guarded: `selftest_reads_each_assetq_project` (an empty scope that read nothing), `assetq_capped_counts_read_at_least` (a truncated or unread list shown as an exact total), `assetq_failed_jobs_keep_their_reason` (failed work visible only as a count) and the not-configured case in `tests/assets-browser.cjs`.

## Browser and binary

`tests/assets-browser.cjs` covers eight cells (1280/360 px, light/dark, 16/24 px) on the actual Rust HTML. It checks visibility, selected navigation, unknown text, no content leak, no overflow, no write control, no non-GET request, links only to the AssetQ site and never a machine path, an open disclosure across refresh, failed-refresh retention, no green under a source that is not `ok` before and after a failed refresh, failed-job rows, the page without JavaScript and the not-configured page across a refresh.

The browser fixture does not exercise the forward-auth guard. The fresh origin probe above passed Access for the health route, but it does not qualify the deployed `/assets` view.

The layout images were refreshed and inspected after each review correction; the approved structure is unchanged. The task result records the final candidate SHA and full project checks.

## PM live readback

Run after the release that contains this change. The [design](../../assetq-summary.md#provisioning) names the config key and files.

1. On the target, run `dashq selftest` and expect `assetq: ok`, or `assetq: incomplete` with a named cap, and no credential in the output. If the read fails, the line names the reason.
2. Through the real guard and HTTPS origin, open `https://dash.ufobe.com/assets`. Expect real jobs, methods, comparisons and storage, with unknown, stale and capped states explicit, no form or action, and links that open the AssetQ site.
3. Open `/data` there and confirm that `assets` holds no credential and no request text.
4. Record the release, the outcome and any gap here.

Until that run, A5 `real configured service` and A7 are unverified.
