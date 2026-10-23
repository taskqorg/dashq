# Task-control qualification #195

Task controls passed a test-only real-board run. The owner moved PM qualification A5 to [#186 A3/A7](https://github.com/taskqorg/dashq/issues/186); #195 adds no PM adapter. This evidence does not enable production controls.

## Candidate and authority

- DashQ v0.15.0, source `9850f5bc1bf6d56a3547ab246ec410ec2cb400fb`; TaskQ v4.6.30, source `09cb0d7db35d0de51bef83d9fa3ec803eafad42d`.
- Both Linux x86_64 binaries matched their published `SHA256SUMS`. The full binary and component hashes, receipts and assertions are in [the recorded trace](task-controls-live-195.json).
- An isolated Docker container on the owner host ran the unmodified release at its fixed loopback address, without published ports. The native loopback port was occupied by another task's candidate; that process was left running.
- The owner's answer in [#195](https://github.com/taskqorg/dashq/issues/195) authorized a test-only exception: reuse the existing host PAT file (mode 0600), one synthetic subject, a private candidate config/store and controller grant. No new token was issued. This does not qualify a guard-verified browser identity or a production credential setup.
- The TaskQ-owned released renderer supplied the offered Start operation and its revision/op-id. Assertions drove the actual DashQ HTTP adapter, TaskQ binary, GitHub board and readback. This was a one-time trace, not a browser click test or a new regression suite.

## Observed result

The single disposable [task #205](https://github.com/taskqorg/dashq/issues/205) depended on #195, preventing worker admission. A temporary harness setup failure closed it before dispatch; the same task was reopened and used for the completed run. No second disposable task was created.

| Check | Observation |
| --- | --- |
| Fresh row | Complete canonical snapshot, held, revision `6356531142`, controls `["start"]` |
| Missing subject | 401 |
| Revoked controller | 403 |
| Actor in another workspace | 403 |
| Cross-origin request | 403 |
| Wrong CSRF | 403 |
| Stale revision | 409, outcome `stale` |
| Denial readback | After each probe, the canonical comment IDs and revision were unchanged |
| Offered Start | 200, `accepted`, state `waiting`, revision `6356558193`, no attempt |
| Same-ID retry | Identical parsed receipt, unchanged canonical comment IDs |
| DashQ projection | Fresh complete board row, `waiting`, revision `6356558193` |

Operation `dashq.control.live195.start.205.6356531142` has exactly one [canonical entry](https://github.com/taskqorg/dashq/issues/205#issuecomment-6356558193). Task #205 was then closed and read back as `CLOSED` / `COMPLETED`. The candidate process, container, store, copied scratch credential and scratch board were removed. The original credential was not changed.

## Remaining qualification

- A5 is moved to #186 A3/A7 by the [owner's answer](https://github.com/taskqorg/dashq/issues/195#issuecomment-6356608853). All four TaskQ v4.6.33 PM composer assets matched their published checksums, but the candidate had no consumer adapter/exchange route. No PM request was sent and no PM host grant was created. #195 makes no PM implementation or qualification claim.
- D10/D13 retain their existing authority. The PM consumer contract, route and test-board request belong to #186; the owner's scope answer resolves the R13 / TaskQ § 7.1 question.
- Native macOS and browser forward-auth qualification remain unproved. The container's older glibc emitted a weak-version warning for TaskQ; the recorded real-board checks nevertheless completed. This trace establishes only the stated candidate/environment.
- A4: no FRA, production config, Access or Caddy changes were made. Production configuration was not read, so its current contents remain unknown. Production enablement still requires the full gate and explicit owner approval under D13.

R12 is applied by naming the exact artifacts, real receipts and remaining gaps; R13 is applied by preserving the production boundary and adding no PM mutation authority. The temporary environment/harness failures are not product fixes or passing live evidence.

## Publication check repair

The [first merge-result test run](https://github.com/taskqorg/dashq/actions/runs/39895526740) failed in `agent_work_agrees_across_data_report_and_page`: sequential responses crossed a second, so host source ages changed from 0 to 1. All other fields in that assertion matched. Query and changes windows also move with each response's timestamp.

On Windows, a temporary 1.1 s delay between the two reads reproduced the assertion failure with `cargo test --locked agent_work_agrees_across_data_report_and_page` (0 passed, 1 failed). The same delayed check passed after validating each source age and window against its response's `generated_at` before normalizing only those computed values for comparison (1 passed). The delay was then removed. Attempts, counts, states, reasons, producer times, query parameters, change records, model use and page presence retain their existing assertions; no product behavior or test gate changed (R12 / TaskQ § 7.4).

The Windows candidate gate also exposed CRLF conversion of `task-controls.js`, `task-controls.json` and `admission-codes.json`, which lacked the byte-preserving Git attributes already used for other pinned TaskQ assets. Before the fix, the admission hash, shared-board conformance and answer live-gate tests failed (109 passed, 3 failed, 2 ignored). Extending the existing attributes restored the upstream bytes without changing the published hashes; `cargo test --locked admission_codes`, `node tests/boards.cjs` and `node tests/answer-live-stub.cjs` then passed. Checksum and pre-write refusal protection remain required.
