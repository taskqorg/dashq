# Agent session archive

Linked detail of [dashq.md D31](../../dashq.md#d31-agent-session-archive). Its narrow R1/R7/R9 exception applies only here. Owner decisions and the source request: [dashq#283](https://github.com/taskqorg/dashq/issues/283). Other surfaces, including `/data`, local report, export, logs and telemetry, carry no archive content.

## Producer wire

Use the existing device, approval, token, ack and revoke routes with `schemas: ["agent-history/1"]`, or add this schema to an existing AgentQ connection by approval. Observation grants do not imply it. The approval names the archive schema and its content effect. Manual capture additionally requires the separately unticked `agent-history.manual` capability on that host's AgentQ installation; no flag inside content grants it.

`POST /machine/archive/1` uses the enrollment Bearer credential and JSON below. The grant supplies workspace and installation; payload ownership fields never do. The existing DashQ HTTP body limit and device ingestion rate apply; no archive timeout, cap, budget, cache, grace period, retry limit, queue bound, device quota or free-space floor is added.

```json
{
  "protocol": 1,
  "schema": "agent-history/1",
  "manifest": {
    "project": "example/project", "task": 283, "attempt": "example-attempt",
    "actor": "worker", "entry": "taskq", "runtime": "opaque-runtime",
    "cli_version": "example-version", "model": null, "effort": null,
    "policy": null, "account_label": "example-account", "host": "example-host",
    "session": "example-session", "parent_session": null, "instructions_hash": null
  },
  "manifest_sha256": "CoreQ canonical digest of manifest",
  "turns": [{"event_id": "opaque-agent.turn-event-id", "start": 0}],
  "capture": "launched",
  "source": "opaque-source-id",
  "format": "adapter-history-format",
  "start": 0,
  "end": 12,
  "sha256": "64 lowercase hex digits of decoded bytes",
  "bytes": "standard padded base64 of the exact scrubbed native bytes",
  "scrubbed": 0,
  "observed_at": 1800000000
}
```

This is an invented example, not a complete valid payload. All manifest keys are required; unknown conditions are null. Project, actor, entry, runtime, host and session are nonempty strings; task and attempt are strings, unsigned integers or null; policy is arbitrary JSON or null. `instructions_hash` is null or a lowercase SHA-256 hex digest. Other conditions are strings or null. `capture` is `launched` (workers and managers) or `manual`. `scrubbed` is the producer's unsigned replacement count; scrubbing is best effort, not a guarantee.

Session identity is `(workspace, installation, manifest.session)`. A source is opaque, chosen by AgentQ, with the history `format` that its adapter reports. Ranges are `[start,end)` of the scrubbed native byte stream; `end-start` equals decoded byte length. Upload contiguous ranges starting at zero; an exact session/source/range/digest replay answers accepted without another object. A different digest, overlapping range, changed source format or a changed manifest hash on an already accepted range refuses `replay_mismatch`; a gap refuses `range_gap`. Complete native records can cross upload ranges. Replaced/truncated sources use a new source ID and preserve their provenance, never overwrite accepted bytes. One session may have multiple native/subagent/background/log sources; a child session has its own manifest and parent identity.

Each upload names `manifest_sha256`, computed by `coreq::enroll::digest`: recursively sort object keys, preserve array order, compact-serialize UTF-8 JSON and SHA-256 those bytes. A session keeps every manifest version, keyed by that digest, and every activation's first `(source,start)` byte position. Versions are append-only; a resumed session may change project, task, attempt, model, effort, policy or other conditions. A condition change starts a new upload range at its first affected byte. The native session identity itself never changes.

Optional `turns` binds a durable `agent.turn` observation `event_id` to its first byte `start` in this source. These are metadata join receipts, not native content. An upload may add a binding for an earlier accepted byte, including on an exact range replay; changing a recorded binding refuses `replay_mismatch`. A binding cannot name an unshipped byte or a media source. Comparison selects the greatest manifest activation at or before that turn's first byte. Missing or conflicting bindings with multiple possible manifests stay unknown; no model, timestamp or task-name guess substitutes for a binding. A session with exactly one manifest has an unambiguous version even without bindings.

Media uses the same contiguous range chunks with `media: true`, `source` equal to the **whole object's** SHA-256, `total` equal to its full byte size, and `format: "media"`. Each chunk's `sha256` still hashes just `[start,end)`; `total` and the whole-object hash stay fixed. Every range is durable/idempotent, but the object is unavailable to readers until `end == total` and SHA-256 of all assembled bytes matches `source`. A failed final verification acknowledges no final range; previously accepted ranges stay pending. Receipts include `complete: false|true`. Identical chunks are stored once by hash; references never fetch a URL or local producer path. Unknown media is a download, never rendered inline. A native reference without an uploaded object remains unavailable. All fields above, including the manifest, remain required.

Success after object fsync and SQLite FULL commit: `{protocol:1,schema:"agent-history/1",state:"accepted",id:"server-owned-session-id",source,start,end,sha256,manifest_sha256,complete}`. Retry the same range after an uncertain response. No archive content or receipt is inserted in the telemetry, snapshot or observation tables. Refusals are `{protocol:1,error:"code"}`: `invalid_request` (400), `invalid_credential`/`revoked` (401), `not_active`/`scope`/`manual_disabled` (403), `replay_mismatch`/`range_gap`/`deleted`/`expired` (409), `incomplete` (409, pending media), and `unavailable` (503, including filesystem exhaustion). Nothing advances the sender checkpoint on refusal.

## Storage and recovery

Config `archive.root` is an absolute dedicated private filesystem root, outside the web root and unrelated service roots; absent means archive unavailable. Object keys are server-owned digests, never paths from the manifest. Objects contain a random nonce and AES-256-GCM authenticated gzip bytes, authenticated with the object hash and encrypted with a domain-separated key derived from `dashq.key`. Reading verifies authentication, decompression and plaintext SHA-256. A separate `archive.db` under the dedicated archive root contains manifests, ranges, hashes, names, tombstones and metadata-only audit rows, never native content or media. It shares no database file or retention with telemetry; enrollment grants and archive roles remain in D11. Neither archive index nor objects adds a quota or free-space floor.

Object creation is atomic and durable before index commit; only committed ranges appear to readers. A failed index commit leaves an unacknowledged orphan, reconciled by the retention pass. Object deletion follows removal of its final index reference and never removes another session's referenced object. Retention runs in the existing hourly maintenance pass and is also checked before reads: content older than 365 days from receipt is unavailable, index remains. Explicit deletion tombstones the session permanently, removes all content references and rejects every replay. Project deletion tombstones every session that names that project in any manifest version in that workspace, across installations. It never rewrites native bytes to remove only one project.

For backup, stop the candidate service, copy the private config and `dashq.key`, take a consistent existing Store backup for enrollment grants/roles, checkpoint the stopped archive index, and copy the complete archive root, including `archive.db` and its WAL if present. Restore into a new isolated directory, preserving 0600 files and 0700 directories; never overwrite an active store. Verify every referenced object with the restored key before activation. Local tests qualify encrypted readback, shared-object deletion, retention, corruption refusal and index/key/object backup/restore; production backup/restore remains a deployment qualification item, not a claim about pds18.

The supplied sizing observation is about 9 GB raw per 14 days across Mac and Windows, gzip about 2.3x, with 555 GB free on pds18. At unchanged volume, 365 days is about 235 GB raw / 102 GB compressed before index and filesystem overhead. These are planning observations, not new quotas, admission floors or verified production capacity.

## Readers and mutations

`dashq admin WORKSPACE SUBJECT` binds the workspace archive owner if none exists; existing installations require an explicit host-administrator `dashq archive owner WORKSPACE SUBJECT`. `dashq archive reader WORKSPACE SUBJECT on|off` grants/revokes the separate read role. Workspace membership and the generic administrator role alone never imply content access. The verified proxy subject is mandatory for archive reads, even on loopback; there is no local-report bypass. Archive role commands require the existing private config and enrollment key. Ownership replacement is an explicit host-administrator action.

Guarded `GET /archive` lists/filter-compares each session’s manifest versions or opens `?id=ID`. `GET /archive/step?id=ID&source=SOURCE&step=STEP` derives and shows a complete selected step; `GET /archive/raw?id=ID&source=SOURCE` returns exact uploaded bytes as a download. `POST /archive/search` searches all retained source bytes, decoded step text and names/conditions in authorized sessions, never puts search text in a URL. Search decrypts content only after audit. It yields sessions, not invented snippets or rankings. `POST /archive/name` and `/archive/delete` are owner-only. Every POST checks same-origin and a subject-bound CSRF token; deletion accepts an ID or a workspace/project pair. All content is escaped text, no Markdown execution, active HTML, remote assets or inline media.

Every source read, including search, normalization, raw and media, commits an audit row first with subject digest, workspace, session ID, source, action, time and outcome; no search text or content. A durable pending audit precedes decryption; successful/failed readback commits the outcome before response, while an interrupted read remains pending/unknown. Audit failure refuses the read. Index reads do not need a content-read audit. Comparison attributes each `agent.turn` by its bound first byte to the manifest then in force and joins only observations of the same workspace/installation, exact session identity and matching project/task/attempt; missing or ambiguous values remain unknown. Summed tokens and duration are reported quantities, outcome comes from observed turns, never guessed from text. Conditions, reported usage, source positions, receipt time/age and expired/deleted state remain distinct.

## Runtime mapping

Read-time normalization uses `agentq-runtime` history at [agentq PR #305](https://github.com/taskqorg/agentq/pull/305), source `15baf4ee4fb43ec83f6fa7a571b11ad6bb7daf20`, and its [RUNTIME.md](https://github.com/taskqorg/agentq/blob/15baf4ee4fb43ec83f6fa7a571b11ad6bb7daf20/docs/spec/RUNTIME.md#universal-step-history). It maps native records into stable step IDs and revisions; DashQ upserts by greatest revision. Raw uploaded bytes remain authoritative. An unsupported format or failed parse remains explicitly unknown, with raw source available. Source reconstruction is private and temporary, deleted after the read; no native file layouts or runtime-name branches exist in DashQ.
