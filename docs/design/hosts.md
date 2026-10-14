# Host identity

Owner decision for the Hosts view ([#138](https://github.com/taskqorg/dashq/issues/138), option A). Status: approved rule; implementation is a follow-up task.

## Observed problem

Live `/data` on FRA showed 12 host rows for three machines. Each source named hosts differently: taskq heartbeat labels (`mac`, `macbook-m2.local`, `win`, `winq`, `wsl`, `hermes-fra1`), agentq `host.name` (`macbook-m2.local`, `DESKTOP`, `hermes-fra1`) and legacy health tags (`<host>/native`). No row joined taskq and agentq, and every row said `no snapshot in 24 h`.

## Terms

- **Machine**: one operating system instance. Its canonical name is the agentq `host.name`, compared case-insensitively and shown as reported by the newest agentq event.
- **Installation kind**: `native` or `wsl`. A WSL instance shares the Windows computer name, so it is a separate kind on the same machine name, never a separate machine.
- **Host row**: one machine and kind, id `<machine>/<kind>`. A Hosts row exists for each kind that reports.

## Rules

- H1. agentq: an event's kind is `wsl` when `agentq.runtime` is `wsl`, or when it is `linux` and the same machine name also has a `windows` installation. Otherwise the kind is `native`.
- H2. taskq: a heartbeat joins the row named by its `machine <host.name> [wsl]` body line (taskq follow-up; the label stays free text). Without that line, a heartbeat whose label equals a machine name joins its `native` row; any other heartbeat keeps its own row with the reason `no machine line: update taskq`. No hand-written alias map exists.
- H3. Several heartbeats on one row (more than one taskq root on a machine): the row shows the newest version and freshest state; the Boards view keeps per-label attempts.
- H4. serverq: the configured serverq host id and enrolled serverq installations use the same `<machine>/<kind>` id.
- H5. Freshness: the row's freshness is the newest agentq `agentq.installation` heartbeat. The legacy snapshot column and its `no snapshot in 24 h` reason appear only while a legacy health row exists for that machine.
- H6. agentq columns: running version, latest release and `behind` come from the freshest installation of the row (D4.1 fleet). A row with a fresh fleet entry never shows empty agentq columns.
- H7. Superseded installations: an installation of the same product on the same row whose newest observation is older than another fresh installation's, or whose version is lower while both are fresh, is marked `superseded by <installation> <version>` in `versions` and leaves the Hosts row. It is never deleted silently: it still reports, so it stays visible with the reason.

## Follow-ups

1. taskq: write the `machine <host.name> [wsl]` heartbeat line (taskq repository).
2. dashq: implement H1–H7, pin them in a hosts fixture, release and update FRA, then verify live `/data` has one row per machine and kind.
