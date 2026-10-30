# Process detail

Linked detail of [dashq.md](../../dashq.md) D2 (Process), D4.2 (snapshot adapter run), D12 (workflow permissions) and D20 (Profiling). The contract rules R1–R9 apply; this page adds lower-priority detail and never weakens them.

## Commands

- `dashq profile [--window=S]` prints the profile report (D20); the default window is 86400 s.
- `dashq doctor` checks the [host requirements](#host-requirements); `dashq --snapshot DIR` writes the fixture page for review ([snapshot command](#snapshot-command)).

## Config

- Config holds the Langfuse base URL, project id and keys, a read-only GitHub token, the project list (D4.2), the ServerQ report path and the host id its row joins, optional `taskq_logs` `{host: absolute taskq.log path}` (D21), optional `prices` (D24) and the public `dash.` host name.
- Other optional keys belong to their sections: `taskq.binary` (D4.2), `serverq` (D4.3), `langfuse.forward` and `telemetry` (D11), `answer`, `task_controls` and `pm_requests` (D13), `resources` (D14), `agentq_configure` (D16).

## Snapshot adapter run

- With config `taskq.binary`, each `taskq status --json` run (D4.2) uses a fresh scratch folder holding only `taskq.json` and a scratch install whose `token` is the read-only token (`TASKQ_INSTALL`; taskq reads no token variable).
- The run has no HOME and incident reports off; the folder is removed after the run.

## Host requirements

Source rule: [specq host requirements](https://github.com/taskqorg/specq/blob/main/engineering/release-contract.md#host-requirements).

- `curl` ≥ 7.20.2 (`--proto`): Langfuse, GitHub, forwarding and coreq update downloads; always required.
- `taskq` ≥ 4.6.32: D4.2 snapshots, D13 answers and controls, D23 admission; required only with `taskq.binary`. Its version comes from its coreq install layout (`releases/<v>/` or `current` beside it).
- Chrome ≥ 59: `--snapshot` only; skipped when absent; the version is unchecked on Windows.
- OS service tools (`systemctl`, `launchctl`, `schtasks`, `powershell`, `whoami`, `id`, `icacls`) ship with the OS: no minimum. No system library (static builds).
- `dashq doctor` prints one line per requirement and exits 1 naming each missing or older tool with the found version, the minimum and the fix.
- `serve` and `service run` at start, `service install` and `update` (the new release's doctor) log the same failures as `requirement:` lines.
- dashq installs no OS packages: the fix line is the step.
- A failed curl, taskq or Chrome run logs its command (never stdin or a `--text` value) and its first stderr line (at most 200 characters).

## Snapshot command

- `dashq --snapshot DIR` writes the fixture page as `DIR/page.html` (stylesheet inlined, no script) and `DIR/page.png`, so a reviewer sees the rendering without a browser login.
- PNG renderer: headless Chrome (`--headless --screenshot`, 1280 px wide). The binary comes from env `DASHQ_CHROME`, else `google-chrome`, `chromium`, `chromium-browser` on `PATH`, else the macOS Google Chrome app.
- No Chrome: `page.html` is written and the command fails naming the missing renderer.
- No config and no source reads.

## Update and rollback

- An `update` that makes a new release current restarts an installed service unit (unload, then load) so it runs that release. `--check`, an unchanged or a failed update leaves the service running. A failed restart exits non-zero and names the error.
- Automatic update: an installed `service run` (the launcher runs `<install>/releases/<ver>/dashq`) checks for a newer release 900 s after start, then every 3600 s. Env `DASHQ_UPDATE_EVERY` (seconds) overrides the interval; the first check waits at most 900 s.
- A newer release is applied by the coreq update (download, checksum, offline selftest, `current`). The service logs it and exits 75, so the unit (systemd `Restart=always`, launchd `KeepAlive`) or the Windows launcher starts the new `current`.
- Each check and apply prints one line to the service log: `update check: <v> available; applying`, `update: <v> applied; restarting`, `update check: up to date (<v>)`, or the failure with its reason.
- A failed check or apply keeps the running release until the next interval. Profile ops are `update.check` and `update` (D20).
- Rollback on failed start: the release becomes healthy after 60 s; the coreq launcher restores `previous` at the third start without `healthy`.
- The running service then writes that version to `<install>/rolled-back` and never applies it automatically again. A newer release proceeds; a manual `dashq update` still installs it.
- `serve`, and `service run` of a binary outside the install, never update themselves.

## Release

- CI (`.github/workflows/release.yml`) on a `v<ver>` tag equal to the `Cargo.toml` version builds static `dashq-<ver>-linux-x86_64` and `dashq-<ver>-linux-aarch64`, `SHA256SUMS` and `latest.json` into a GitHub release of `taskqorg/dashq`.
- Each artifact passes `selftest` with `DASHQ_SELFTEST_OFFLINE=1` before publishing; a failure publishes nothing.
- Every pull request runs the same two builds and selftests without publishing; `main` requires them with `tests`.
- Only owners push tags. Installed services take a release on their own (above), other hosts with `dashq update`; rollback is coreq `previous`.
- Every workflow declares top-level `permissions: contents: read`; a publishing job adds only what it writes (`release.yml` `publish`: `contents: write`).

## Profiling

Applies [built-in profiling](https://github.com/taskqorg/specq/blob/main/engineering/profiling.md).

- Counters are always on in `serve`, `service run`, `report` and `update`: one record per finished operation `{at, op, ms, outcome, error}`. `selftest` and tests write nothing.
- Measured operations, p95 budget and error budget (initial values; revise from the first production report):
  - `http.page` and `http.data` 3 s, 1 %; `http.enroll` (D11 routes) 500 ms, 1 %; `http.answer` and `http.control` 15 s, 5 %.
  - `http.resources` (D14 routes), `http.configure` (D16) and `http.pm` (D13 PM routes) 500 ms, 1 %; `http.link` (D25 routes, receive waits up to 25 s) 30 s, 1 %.
  - `read.langfuse` and `read.taskq` 10 s, 5 %; `read.board` and `read.release` 3 s, 5 %; `read.serverq` 50 ms, 1 %; `read.tasklog` (D21) 500 ms, 1 %.
  - `update.check` 15 s, 5 %; `update` 120 s, 5 %; `restart` (service reload after update) 30 s, 5 %.
- Times include response writing; cached source hits are not reads. A 5xx is a failure classed by its code; a 4xx is the caller's. Assets and 404 are not measured.
- The error class is the source reason before any `:` detail, at most 40 plain characters; update and restart record only `failed`.
- Store: `profile.jsonl` (0600, kept on prune) next to the config. Past 4 MiB the writer keeps the newest half under a first line `{"pruned":true}`.
- Detailed traces: env `DASHQ_TRACE=1` for the process run prints one line per operation to the process log (stderr). The process logs `profile: trace on`; the report shows `trace`.
- Each HTTP call is a span `trace #ID`: its source reads and the `enrollment` and `build` steps print as `step NAME MS ms at +OFFSET ms OUTCOME`, then the call total.
- Report: JSON `{schema 1, window_s, from, to, trace, store, samples, operations, unknown}`.
- Operations rank worst first by the larger of p95/budget and failure rate/error budget, then by variation (`cv`). Each has count, p50/p95/max ms, outcome counts and error classes.
- A named operation without samples is listed under `unknown` with its reason. A missing store is `unknown`. A prune that dropped records inside the window makes the store `unknown` with the covered start. Skipped corrupt records make it `incomplete` with their count. Never zero or ok.
