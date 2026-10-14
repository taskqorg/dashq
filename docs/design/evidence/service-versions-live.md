# Service versions live qualification

Task #73 qualifies the D18 projection (#70, #72) against real source evidence from
specification revision `c9f83071ab6a9bfe4534a93a50f7e2de6d182757` (V1–V4). Read-only: no candidate
deployment, update, restart, rollback, raw status/log export or production access was performed.

## Run

`dashq report` from this branch on a development Mac, with a private 0600 config outside the
repository: GitHub configured with a read-only-used token for the six D4.2 projects, Langfuse
absent, the serverq report path missing. Generated at epoch `1791962554` (2026-10-14 07:22 UTC).
Receipts were read separately at the same time: every open `taskq-host` issue (`version`,
`service`, `at` lines) in the six repositories and `releases/latest` of each repository.

## A1. Fields against source receipts

The live GitHub source yields 23 rows, all `taskq-heartbeat` provenance (product taskq, one row per
`service` id and project workspace; legacy heartbeats without `service` key by host).
A script matched each heartbeat issue to exactly one row and compared six fields per row:
installation (`service`), running (`version`), release (`taskqorg/taskq` latest `v4.1.11`),
selected, health and lifecycle. Result: 138 of 138 equal.

- Running: the heartbeat's exact version, e.g. `4.2.0-dev.1791961334`; seven rows past the heartbeat
  `ttl` are `stale` and their versions are marked last known.
- Selected: unknown (`not reported`); heartbeats carry no selection.
- Available: `4.1.11` with its own check time, separate from host observation time.
- Update: `current` on the 16 fresh rows (verified comparison: `4.2.0-dev.*` sorts above `4.1.11` by
  coreq `update::newer`), `unknown` (`observation not current`) on the 7 stale rows.
- Health and lifecycle: unknown with the producer gap as reason; no row reads healthy.
- `mixed` lists the six distinct running taskq versions, including last-known ones.

Defect found and fixed: `data::semver` accepted only `MAJOR.MINOR.PATCH`, so every current
heartbeat, whose producer reports a pre-release dev build, read running `not reported` and update
`unknown`. It now also accepts a SemVer pre-release (`-` then dot-separated `[0-9A-Za-z-]`
identifiers, at most 64 bytes); build metadata and other forms still read unknown. The chain
`cargo test --locked cached_heartbeat_time_and_project_identity` now uses the real producer form;
it failed before the fix (`left: Null`) and passes after it. The same parser feeds the Hosts
taskq column and AgentQ version fields.

## A2. Unavailable and partial sources

- Partial (run above): Langfuse `unknown` (`not configured`), serverq `unknown`
  (`read failed (report missing)`), GitHub `ok`. No AgentQ, serverq or DashQ row is invented.
- Unavailable: the same config with an invalid GitHub token. Every GitHub read reports
  `read failed (HTTP 401)`, all three sources are `unknown` with reasons, `versions` is
  `{"rows": [], "mixed": []}` and no board or version value reads ok, zero or current.
- Last-known values across a failed refresh in a running service remain covered by the chain
  `cached_outages_preserve_last_known_versions`; this one-shot report has no earlier cache.

## A3. Rendered evidence and adapter coverage

The live `/data` document rendered through the production `page::render` and stylesheet, Services
section only (temporary local render hook, not committed). Captured with Chromium headless shell
1243 at a real 360 px viewport (desktop Chrome headless enforces a 500 px minimum).

- [1024 px / 16 px, light](service-versions-live-1024-16-light.png)
- [360 px / 20 px, dark](service-versions-live-360-20-dark.png)

No horizontal overflow at 360 px; long dev versions wrap inside the Running cell. Hostnames and
installation IDs shown are already public in the heartbeat issues; no path, credential or raw
output appears.

| Adapter | Live qualification |
|---|---|
| taskq heartbeats (GitHub) | Qualified here: identity, running, stale/last known, release, update |
| GitHub `releases/latest` | Qualified here for taskq; other products have no row from this source |
| Enrolled `agentq-snapshot/1` | Not qualified: needs the production enrollment store (separate authorization) |
| Langfuse `agentq.installation` | Not qualified: needs production Langfuse keys (separate authorization) |
| Legacy updater snapshots (Langfuse) | Not qualified: same as above |

Producer gaps from #70 remain unknown in every row: selected version, health receipt, lifecycle
stage, update history, safe reason code and coreq version.
