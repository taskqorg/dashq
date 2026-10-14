# Optimization results live qualification

Task #68 qualifies P3 of [optimization results](../optimization-results.md) against real AgentQ events. The owner authorized read-only access to local AgentQ stats and export on one development Mac, a private 0600 DashQ config with Langfuse keys outside the repository, and a few real `agentq run` commands on that Mac. No profile, routing, agent setting, deployment or production DashQ instance was changed. No synthetic event was written to any source.

## Versions and run

- Producer: AgentQ `0.5.9` (macOS, installation `000018de-2422-a185-2a58-0001706f0000`), RTK `0.51.0`, headroom `0.40.0`, profile and routing unchanged (`agentq doctor`: Claude/Codex/Hermes direct).
- Source: self-hosted Langfuse v4 (events-only), `GET /api/public/v2/observations` only.
- Local consumer: `dashq report --period=7200` built from `14a3b4a6e02fca58e555abf66f0d6cd3a8d8eec4` (dashq 0.7.0), Langfuse configured, GitHub absent, serverq report missing.
- Guarded consumer: the deployed FRA service, dashq `0.8.0`, tag `v0.8.0` on `21843d8e42611a7dc51eca7830bbdaa82b7c9636`. That commit only bumps the version on top of `14a3b4a`; the renderer and accounting code are the same. The owner read `GET /data?period=7200` on the service loopback (`generated_at` 1791986690) and attached the redacted `optimization` excerpt in [issue comment 6170234160](https://github.com/taskqorg/dashq/issues/68#issuecomment-6170234160).
- Events: five real `agentq run` calls on 2026-10-14 13:45–13:46 UTC from the dashq worktree: three git read commands, `cargo check --locked` and `cargo test --locked` (66 passed).

## A1. Same IDs, window and scope

Each source was read for a window that contains the five events (13:45–13:46 UTC), all clients, all hosts:

| Source | Read window (UTC, `[start, end)`) | Events in read window |
|---|---|---|
| Local stats `data/stats/2026-10-14.jsonl` | whole day up to its read after the five runs | exactly the five below |
| Langfuse `agentq.compress` | 2026-09-01 up to its read after the five runs | exactly the five below |
| Local DashQ 0.7.0 `/data` | `[11:47:27, 13:47:27)` = `[1791978447, 1791985647)` | exactly the five below |
| Guarded DashQ 0.8.0 `/data` | `[12:04:50, 14:04:50)` = `[1791979490, 1791986690)` | exactly the five below |

The comparison window is the intersection `[2026-10-14T12:04:50Z, 2026-10-14T13:47:27Z)` = `[1791979490, 1791985647)`. All five events lie inside it, and no source lists any other event in its read window. So, within the comparison window, every read lists exactly the same five events.

The guarded window comes from the `/data` JSON. The prose of the same comment gives `[1791979284, 1791986484)` and converts it to 12:41:24Z–14:41:24Z; both differ from the JSON and from each other, so this record uses the JSON values.

| Event ID | Local stats | Langfuse `agentq.compress` | Local `/data` 0.7.0 | Guarded `/data` 0.8.0 |
|---|---|---|---|---|
| `…6951a416723000014df30001` | passed · not_qualified · bytes null | same | same cohort, evidence ID | same cohort, evidence ID |
| `…6951a836c04000014e440001` | passed · not_qualified · bytes null | same | same | same |
| `…6951adbef70800014e6d0001` | passed · not_qualified · bytes null | same | same | same |
| `…695bb2fce270000158880001` | passed · short · 0 → 0 B · `cargo check` | same | same | same |
| `…695ff054636000015b7e0001` | compressed · rtk 0.51.0 · 4747 → 131 B · `cargo test` | same | same | same |

`agentq stats --json` gives the same per-engine totals. In both `/data` documents the three `not_qualified` events form one cohort (runner null), and all five carry installation `000018de-2422-a185-2a58-0001706f0000`, runtime `macos`, AgentQ `0.5.9`.

Between those direct reads and 14:04:50Z, only the two `/data` reads cover the window; neither counted a further event on any of the six fleet installations. Local stats and Langfuse were not read again.

## A2. Totals, pairs and source states

- Outcomes: 5 calls = 1 compressed + 4 passed + 0 failed. Reasons: not_qualified 3, short 1, compressed 1. The totals are equal in all four reads.
- Paired bytes: run · rtk 1/1 paired of 1 observed compressed, 4747 → 131 B, reduction 97.2 % (guarded `reduction` 0.9724). Estimated tokens 1187 → 33 stay labelled `ceil(bytes/4)` estimates. The four pass-throughs add no bytes.
- State `partial`, lower bound: exported events carry no interval coverage. The other five fleet installations (`0.5.9` heartbeats, no calls) read `unknown`, not zero, in both `/data` documents. The guarded coverage reads `read 5 · counted 5 · duplicates 0 · out_of_window 0 · legacy_run 0 · capped false`.
- Guarded source state: `stale` (last observation 13:46:02Z, 18 min old, limit 10 min). The page shows `Source: stale · 18m old`; the counts stay a partial lower bound and do not read as current.
- Late delivery: a query at 13:45 UTC, when the agentq queue held the three first events as pending, returned 0. After delivery, the events kept their source time. DashQ counted all five in the event-time bucket `[13:37:27, 13:47:27)`; delivery time did not set the bucket.
- Duplicates: the five real Langfuse rows, fed twice to the production `optimization::build` in a temporary development check, still give 5 calls with the same outcomes, reasons and bytes, and `duplicates 5`.
- Unavailable: the same config with an invalid secret key gives Langfuse `unknown` (`read failed (HTTP 401)`). Results show state `unknown`, calls/outcomes `null` and no volume. No value reads zero or ok.

## A3. Evidence, read-only scope and content

- DashQ only issues Langfuse GET reads. The key is the existing project key that the session tracing uses; Langfuse does not issue a read-only scope for it, so read-only is a property of the client, not of the key.
- A scan of the Langfuse rows, `/data` and the rendered HTML found no command arguments, paths, user name, output text or key. The runner is a producer enum (`cargo test`, `cargo check`); git commands export `runner: null`. Langfuse keeps the opaque recovery ref `rtk:<id>`; `/data` drops it.
- Evidence references in `/data` are core event IDs and installation IDs only.

## A4. Rendered evidence

### Guarded excerpt, dashq 0.8.0

The guarded `/data` excerpt (`generated_at`, `period`, `optimization`, with empty filters) was rendered through `page::render`, `assets/style.css` and `assets/app.js` at `21843d8e42611a7dc51eca7830bbdaa82b7c9636` (temporary local hook, not committed). The same local server returned the excerpt for `/data`. Chrome 155.0.8059.41 on Windows, Results section only. The check opened the Passed reasons and both disclosures, then refreshed twice: the selection and disclosures stayed open, with no page error and no horizontal overflow. The page shows window `2026-10-14T12:04:50Z – 2026-10-14T14:04:50Z`, `5 Calls`, `1 Compressed`, `4 Passed`, `0 Failed`, `run · rtk · 97.2% reduction`, 4747 B → 131 B and the five core event IDs.

- [1024 px / 16 px, light](optimization-results-guarded/1024-16-light.png)
- [360 px / 20 px, dark](optimization-results-guarded/360-20-dark.png)

### Local read, dashq 0.7.0

The live `/data` document was rendered through the production `page::render`, `assets/style.css` and `assets/app.js` (temporary local hook, not committed; the installed DashQ service held the loopback port). Chrome 155.0.8059.41, Results section only. The check opened the Passed reasons and both disclosures, then refreshed twice: the selection and disclosures stayed open, with no page error and no horizontal overflow.

- [1024 px / 16 px, light](optimization-results-live/1024-16-light.png)
- [360 px / 20 px, dark](optimization-results-live/360-20-dark.png)

Whole-task effect reads `not measured`: no paired experiment exists, and five calls prove only these runs.

## Gaps

- The guarded comparison uses the owner's loopback read of the deployed service, not a read through Cloudflare Access; workers have no Access grant. The guarded screenshots render that excerpt with the deployed release's code; they are not captures of the deployed page.
- Only the `run` adapter on macOS produced events. Hook/provider adapters, the headroom recipe, failed outcomes, other runtimes and legacy `agentq.run` overlap have no live events in this window.
- `agentq stats --json` reports `bytes_in: null` for the `none` group, although its `cargo check` record measured 0 → 0 B (`short`). This is a producer display gap; the per-event values match.
- The Coverage & evidence disclosure prints raw JSON (`excluded {}`, `Observed []`, epoch gap pairs). The values are correct, but they are hard to read.
