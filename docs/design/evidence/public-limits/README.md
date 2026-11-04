# Public limits cutover qualification

Scope: [#255 A3](https://github.com/taskqorg/dashq/issues/255), continuing A1 and A2 in [PR #256](https://github.com/taskqorg/dashq/pull/256). Owner: "limits page stays open, it shows only limits and resets, as now, same look".

## Result

**A3 is blocked. No cutover was performed.** The read-only live comparison on 2026-11-04 at approximately 13:25 UTC failed because TaskQ omits five rows that ccusage still supplies. The producer fix is filed as held [taskq#2359](https://github.com/taskqorg/taskq/issues/2359).

Applied rules: [D27](../../../../dashq.md#d27-public-limits-push) requires a passing comparison before adding the push token or stopping ccusage; R2/D6 keep missing observations unknown. R5/R6 retain the guarded loopback-only DashQ surface, and R7 keeps credentials server side. No production config, publisher, Worker asset or login was changed.

## Live evidence

Candidate implementation: `e1c9482af63898632152279b082ffb029d564f86`.

- TaskQ's latest published release is [v4.6.74](https://github.com/taskqorg/taskq/releases/tag/v4.6.74), published at 12:26:56 UTC. The observed service heartbeats report `4.6.74` on Windows and Linux and `4.6.74-261104-3` on macOS.
- Heartbeats now contain limit rows for Claude, both Codex accounts and Antigravity. Shared account fingerprints merge across hosts; the private fingerprints are not included here.
- The public Worker root and `/u.json` both returned successfully without a login. Four account cards had producer observations less than 900 s old. The existing page still fetches `/u.json` every 30 s and `/h.json` every 300 s, and dims accounts after 900 s.
- A temporary local config selected `taskqorg/dashq` as its board and mapped the four private fingerprints to the existing public card ids. It contained no `limits_push.token`, and was removed after the reads. `dashq limits --check` is read-only regardless of token configuration.

The existing `dashq limits --check` command exited **1**. A subsequent `cargo run --locked -- limits --check` rebuilt/verified the candidate and reproduced the same five matching and five missing rows (exit **1**); the moving Antigravity reset times remained within tolerance. Initial output:

```text
ok claude 1w: dashq 34 reset 1794200400 / live 34 reset 1794200400
FAIL claude Fable 1w: only in the live page
ok codex-1788798682 1w: dashq 13 reset 1794379777 / live 13 reset 1794379777
ok codex-1789722405 1w: dashq 8 reset 1794382572 / live 8 reset 1794382572
FAIL gemini-1793542965 1w: only in the live page
FAIL gemini-1793542965 5h: only in the live page
ok gemini-1793542965 Claude and GPT models 1w: dashq 0 reset 1794403304 / live 0 reset 1794403365
ok gemini-1793542965 Claude and GPT models 5h: dashq 0 reset 1793816504 / live 0 reset 1793816565
FAIL gemini-1793542965 Gemini Models 1w: only in the live page
FAIL gemini-1793542965 Gemini Models 5h: only in the live page
limits check: FAIL (5 rows compared)
```

Five shared rows pass the existing tolerances (5 percentage points and 600 s). Five displayed rows are absent from the TaskQ payload. These are missing producer rows, not a percentage or reset-time tolerance failure:

- Claude: the heartbeat has only `seven_day` (34%). The page also shows Fable weekly (2%).
- Antigravity: the heartbeat has only `3p-weekly` and `3p-5h` (both 0%). The page also shows Gemini weekly (74%) and 5h (0%), both as the top-level windows and as the Gemini Models detail rows.
- Codex: both weekly windows match, including resets; shared-host duplicates do not create extra cards.

This is failure-before evidence only. There is no passing live comparison or producer fix in this revision. No tests or decided assertions were changed to permit the missing rows.

## Resume after the producer fix

1. Verify the released service heartbeats include the missing model windows. Keep ccusage publishing throughout comparison.
2. Run `dashq limits --check` with the existing public card mappings and no push token. Every card and row must pass; a missing value remains a failure.
3. Complete the side-by-side visual check of the unchanged Worker page with the real replacement payload, including refresh and stale behavior. This attempt did not establish visual acceptance.
4. Only after those checks pass, perform the D27 cutover and verify the public page continues refreshing from DashQ. Preserve a rollback to the existing publisher.

A1/A2 remain recorded as completed stages. A3 remains open; the public page still uses ccusage. The full implementation test suite from the previous candidate was not rerun for this evidence-only change (specq Testing, Run policy by stage).
