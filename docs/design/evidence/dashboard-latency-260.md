# Dashboard latency (#260)

[Owner request and acceptance](https://github.com/taskqorg/dashq/issues/260):
"Dashboard page must answer within seconds."

## Failure and cause

The task reports `http.page` up to **287929 ms** and `http.data` up to
**88515 ms** in the production 0.18.1 `profile.jsonl`. These are task-supplied
observations; this worker has not read that production file.

At base `bfe39ebfdab6559d0d0922d6e8545c93bf883138`, both HTML and `/data`
call `App::doc`, which synchronously calls `enroll::fleet_records`.
`Cache::get` holds the Fleet slot mutex throughout the read. On expiry, Fleet
calls `Store::telemetry_since(..., |_| true)`: it lists every retained block ID
and fetches every payload separately while holding the connection used by
ingestion and projection. Fleet then parses and decompresses every block before
keeping the newest 10,000 relevant rows. The row cap does not cap that work.
Other page requests queue behind the same slot; their earlier projection can
also queue behind the shared database connection. Fleet's two CLI release reads
are also synchronous and can consume two sequential 10 s HTTP timeouts.

This establishes a blocking mechanism, not historical attribution of those two
maxima. Production profile/trace correlation for **A1 remains open**.

An authenticated `GET https://dash.ufobe.com/` from the Windows worker timed out
after **15.015078 s**, curl exit **28**, with no HTTP status. Credentials stayed
in curl stdin and were not recorded. SSH to the production name failed with
`Host key verification failed.` No host key or authentication setting changed
(R5, R7).

## Change and regression evidence

- **D4 / A2:** browser HTML and `/data` use a single background Fleet refresh per
  installation/workspace. Fleet CLI release reads use the same background cache
  mechanism. `dashq report` retains synchronous source reads.
- **D6 / R2:** pending/failed refreshes retain the prior value, incompleteness
  and time. A pending first read has null data/time and a reason. Existing Fleet
  rendering shows producer age and an unconfirmed/unknown state.
- **D11 / R9:** Fleet opens a read-only connection to the same WAL database; it
  neither holds the ingestion/projection connection nor creates another store.
  Its query retrieves payloads in one ordered scan rather than one query per ID.
- **D11:** actor-scoped rows are checked before each cache lookup. Tests retain
  denied, anonymous and revoked-cache protections. No raw telemetry is added to
  browser output (R1).

Before the implementation, `cargo test --locked fleet_refresh_does_not_hold_callers -- --nocapture`
failed: `Fleet held the caller behind its read: Timeout`. The reproduction used
the original synchronous cache through a forwarding `refresh` entry point.
After the implementation, `cargo test --locked fleet_ -- --nocapture` passed
**9 checks**. The retained checks cover blocked and failing reads, shared
single-flight reads, cold unknown-to-success, last-good time, the independent
SQLite reader, all ten HTML view routes and `/data`, producer age, and scope.
The route check releases its deliberately blocked reader only after receiving
all eleven responses. This is deterministic regression evidence, not production
performance qualification.

## Remaining acceptance and rollback

**A2 is locally verified. A1 production correlation and A3 are unverified.**
No candidate was installed by this worker and no post-install ten-request result is
claimed. Other source reads remain synchronous; their contribution needs the
production profile. Windows checks do not establish Linux or proxy behavior.

The PM handoff below inspects the same profile report
before and after installing the exact checked candidate. Record its SHA/artifact
digest and the running executable; run ten authenticated public `GET /` probes,
requiring HTTP 200 and less than 5 s each (A3). Retain source states and producer
age while Fleet refresh is slow or fails. Preserve the previous executable for
rollback. This change has no schema migration, configuration or grant change;
rollback restores the prior executable against the same database.

## PM handoff after the accepted access answer

The accepted answer on #260 assigns the pds18 work to the PM. The worker must
not change SSH key permissions or work around access. A1 and A3 remain pending
the PM's receipts; returning this PR for review does not waive either item.
The accepted answer reports that the host key is now trusted and private-key
permissions block the worker. No SSH attempt was made in this continuation.

The worker ran ten more authenticated public `GET /` probes on 2026-11-05,
10:59:19-11:00:04 UTC, before any candidate installation. **0/10 passed**:
every call reached its 5 s client deadline with curl exit 28 and HTTP status
`000` (no response), taking 5.002271-5.017402 s. This proves that the current
public path still misses the target from this worker. It does not identify
which proxy/backend stage timed out or establish post-install A3 performance.
Per-try metadata is in [dashboard-latency-260-probes.json](dashboard-latency-260-probes.json).

The commands below are for the PM, not executed host changes. Run the pds18
blocks as the existing service owner, from a trusted DashQ checkout, only after
the candidate's required merge-result CI and deployment authorization. Use the
existing `dashq-dashq.service` user unit (README D3). Privately set/export
`DASHQ_CONFIG` to that service's existing config file; do not print or copy its
contents. The same source code remains at immutable implementation commit
`e012543112ef8899d7a875e28a8c66f9fc014345`; later handoff edits change evidence only.

### A1: preserve and inspect the production profile before activation

```bash
set -euo pipefail
umask 077
: "${DASHQ_CONFIG:?Use the existing private service config}"
test -r "$DASHQ_CONFIG"
repo=$(git rev-parse --show-toplevel)
receipts="$repo/.taskq/acceptance-260"
mkdir -p "$receipts"
unit=dashq-dashq.service
pid=$(systemctl --user show "$unit" -p MainPID --value)
test "$pid" -gt 0
running=$(readlink -f "/proc/$pid/exe")
printf 'before binary sha256: '
sha256sum < "$running"
test ! -e "$receipts/profile.before.json"
test ! -e "$receipts/profile.before.jsonl"
"$running" profile --window=86400 > "$receipts/profile.before.json"
cp -- "$(dirname -- "$DASHQ_CONFIG")/profile.jsonl" "$receipts/profile.before.jsonl"
python3 - "$receipts/profile.before.jsonl" <<'PY'
import json, sys
rows = []
for line in open(sys.argv[1], encoding="utf-8"):
    try:
        row = json.loads(line)
    except ValueError:
        continue
    if isinstance(row, dict) and isinstance(row.get("at"), int) and isinstance(row.get("ms"), int):
        rows.append(row)
for op in ("http.page", "http.data", "read.fleet", "read.langfuse", "read.board", "read.taskq"):
    selected = [r for r in rows if r.get("op") == op]
    if not selected:
        print(json.dumps({"op": op, "state": "unknown", "reason": "no retained samples"}))
        continue
    slow = sorted(selected, key=lambda r: r["ms"], reverse=True)[:5]
    print(json.dumps({"op": op, "count": len(selected), "slowest": [
        {k: r.get(k) for k in ("at", "ms", "outcome", "error")} for r in slow]}))
PY
```

Return the operation counts, maxima, UTC completion times and the profile
report's retention/unknown states. Relate `read.fleet` to the blocking code
path described above and name any other material slow source. The records
contain completion time but no request ID: overlapping intervals alone do not
prove which source belongs to a particular request. If the historical maxima
were pruned or the evidence cannot distinguish causes, keep that part of A1
unknown rather than inventing attribution. Preserve the original receipts;
publish only metadata summaries, never config, environment or raw service logs.

### A3: build and activate the checked implementation on the existing service

This is a scoped development installation, not a new release or a replacement
of immutable 0.18.1 files. The runtime override below keeps the existing unit's
config, database, user, working directory and authentication. A binary outside
the coreq release layout does not auto-update (Process detail); the PM must
later remove this override for the normal released-service update path.
Continue in the same pds18 shell after A1:

```bash
candidate=e012543112ef8899d7a875e28a8c66f9fc014345
git fetch origin "$candidate"
test "$(git rev-parse FETCH_HEAD)" = "$candidate"
stage="$receipts/$candidate"
mkdir "$stage"
git archive "$candidate" | tar -x -C "$stage"
(
    cd "$stage"
    cargo test --locked fleet_
    cargo build --locked --release
)
bin="$stage/target/release/dashq"
DASHQ_SELFTEST_OFFLINE=1 "$bin" selftest
digest=$(sha256sum < "$bin" | cut -d ' ' -f 1)
printf 'candidate source: %s\ncandidate sha256: %s\n' "$candidate" "$digest"
: "${XDG_RUNTIME_DIR:?Use the existing service owner runtime}"
dropdir="$XDG_RUNTIME_DIR/systemd/user/$unit.d"
override="$dropdir/90-task260.conf"
test ! -e "$override"
mkdir -p "$dropdir"
printf '[Service]\nExecStart=\nExecStart="%s" service run\n' "$bin" > "$override"
rollback260() {
    rm -f -- "$override"
    systemctl --user daemon-reload
    systemctl --user restart "$unit"
}
trap rollback260 ERR
systemctl --user daemon-reload
systemctl --user restart "$unit"
systemctl --user is-active --quiet "$unit"
pid=$(systemctl --user show "$unit" -p MainPID --value)
test "$pid" -gt 0
test "$(readlink -f "/proc/$pid/exe")" = "$bin"
test "$(sha256sum < "/proc/$pid/exe" | cut -d ' ' -f 1)" = "$digest"
printf 'running source: %s\nrunning sha256: %s\n' "$candidate" "$digest"
activated_at=$(date -u +%s)
printf 'activation UTC epoch: %s\n' "$activated_at"
trap - ERR
```

Return the source SHA, built/running digest match, native focused test and
offline selftest results, activation time and service active state. These prove
process identity, not A3 latency. No native build, restart, override, database
or configuration command above has been executed by this worker. On a failed
post-install check, run `rollback260` in this same shell and verify that the
running executable again equals `$running`; do not touch existing drop-ins.

### A3: ten authenticated public probes after that install

Run from an authorized client, with its own site credentials loaded privately
following the site's agent-access README. Export the two existing credential
variables for this command; do not copy credentials between hosts. This sends
headers through curl stdin, discards response bodies and prints metadata only
(R1/R5/R7). It adds no authentication route or exception. The same recipe can
also record a baseline; only a run after the matched activation proves A3.

```bash
python3 - <<'PY'
import json, os, subprocess, sys
fields = (("CF-Access-Client-Id", "CF_ACCESS_CLIENT_ID"),
          ("CF-Access-Client-Secret", "CF_ACCESS_CLIENT_SECRET"))
cfg = 'url = "https://dash.ufobe.com/"\n'
for header, key in fields:
    value = os.environ[key]
    if not value or any(c in value for c in '\r\n"\\'):
        raise SystemExit("missing or invalid site credential field")
    cfg += f'header = "{header}: {value}"\n'
passed = 0
for attempt in range(1, 11):
    r = subprocess.run(["curl", "-sS", "-o", os.devnull, "--max-time", "5",
        "-w", "%{http_code} %{time_total}", "-K", "-"],
        input=cfg, text=True, capture_output=True)
    status, seconds = r.stdout.split()
    ok = r.returncode == 0 and status == "200" and float(seconds) < 5
    passed += ok
    print(json.dumps({"try": attempt, "http_status": status,
        "seconds": float(seconds), "curl_exit": r.returncode, "pass": ok}), flush=True)
print(json.dumps({"passed": passed, "tries": 10}), flush=True)
sys.exit(0 if passed == 10 else 1)
PY
```

The PM's A3 receipt must pair all ten HTTP 200 results below 5 s with the
source/digest activation receipt. A login redirect, error status, timeout,
pre-install success or average below 5 s is not a pass. After these requests,
run `"$bin" profile --window=86400 > "$receipts/profile.after.json"` on pds18
and compare the same operations with `profile.before.json`, separating samples
before/after activation and any pruned window. D6 last-good/age behavior remains
covered by the regression checks; native/production evidence stays separate.
