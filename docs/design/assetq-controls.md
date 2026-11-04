# AssetQ controls

Design package of [D27](../../dashq.md#d27-assetq-controls). AssetQ owns jobs, methods, comparisons, recommendations and assets; this page defines how DashQ reads them and transports the allowed actions. The candidate below is a proposal until the owner approves it.

## Owner request

- [#246](https://github.com/taskqorg/dashq/issues/246) goal: "Give people one source-backed AssetQ control surface for jobs, generation methods, comparisons and storage state."
- A3: "Support allowed submit/cancel and method add/edit/promotion actions with scoped grants, exact revisions and verified receipts; reading never grants a mutation."
- The owner's answer on that task selects keeping A3 in DashQ over the earlier summary-only note: "Keep full A3 in DashQ - + all actions in one page; - overrides note and AssetQ R9, duplicates site editor".
- Previews, full asset browsing and benchmark actions are not in A3 and stay on the AssetQ site, as that note assigns.

AssetQ R9 and its method-evaluation page still say DashQ receives summary metadata only and no generation authority. AssetQ must amend them in its own repository before production enablement; DashQ does not edit them.

## Use cases

| ID | Intent | Result | Failure and recovery |
|---|---|---|---|
| M1 | See what is running and whether it is healthy | Jobs by phase, active rows with step, effect, cancellation and usage, uncertain submissions, provider health | A failed read keeps the last values as `unconfirmed` with their time; missing usage is `—` with its reason, never 0 |
| M2 | Choose a method | Every version with status, operation, provider, model, limits, outcomes and qualification record | Status, qualification and provider health stay separate; a qualification record never enables an action |
| M3 | Judge alternatives | Comparison identity, dataset, evaluator and method versions, per-method metrics, missing cases, scoped recommendation or explicit none | Incomplete results stay incomplete; DashQ shows no winner of its own |
| M4 | Check storage | Store use and quota, free space and floor, metadata admission, integrity, backup, restore | Deferred or unobserved checks read unknown with AssetQ's reason |
| M5 | Act | Submit or cancel a job; add or edit a method version; promote a proposed recommendation | Stale revision, refusal and unknown delivery are distinct receipts; drafts stay |
| M6 | Follow a record | Links to the job, result, object and download records at the AssetQ origin | AssetQ authenticates; without a session the link reads AssetQ's own refusal |

## Authority

Off by default. Protected deployment configuration names credential files, not values:

```json
{"assetq": {"base_url": "https://assets.ufobe.com",
  "projects": {"invented-project": {"workspace": "invented-workspace",
    "read_token": "/absolute/read-token", "control_token": "/absolute/control-token"}}}}
```

Each file is mode 0600 and holds one AssetQ project grant. `read_token` is used only for GET requests. AssetQ has no read-only role, so DashQ's GET-only read adapter is the boundary that keeps reading from mutating; a `member` grant is the smallest one AssetQ offers. Without `control_token` every action is off. `control_token` is never used for reads, and its role still decides owner-only writes: a `member` grant cannot edit methods or promote.

The host administrator runs `dashq assetq-grant WORKSPACE SUBJECT jobs|methods on|off`. Each request needs the guard-verified `X-Dashq-Subject` to administer the project's workspace and hold the action's grant. Enrollment, observation, answer, task-control and PM grants carry none of this. Revocation applies on the next request.

## Routes

`GET /assetq/control?project=P` returns a CSRF token and the actor's capabilities for that project. `POST /assetq/control` takes one strict JSON object, with the same-origin HTTPS `Origin`, JSON content type, `X-Dashq-CSRF` and the existing write rate limit. Unknown fields, duplicate keys and other actions are refused.

| `action` | Grant | Fields | Exact revision check before dispatch | AssetQ call | Readback for the receipt |
|---|---|---|---|---|---|
| `job.submit` | `jobs` | `operation`, `method` (`id@version`), `text`, `attachments`, `constraints`, `max_credits` | `revisions.registry` from `GET /v1/status` | `POST /v1/jobs` with `idempotency_key` `dashq.assetq.<op_id>` | Returned job ID, then `GET /v1/jobs/{id}` |
| `job.cancel` | `jobs` | `job` | The job's `updated_at`; a terminal job is refused | `POST /v1/jobs/{id}/cancel` | `GET /v1/jobs/{id}` shows its `cancel` state |
| `method.save` | `methods` | `id`, `status`, `spec` | `revisions.registry` from `GET /v1/status` | `POST /v1/methods` | The new version from `GET /v1/methods` with an equal specification |
| `recommendation.promote` | `methods` | `recommendation`, `use_case`, `constraints` | `revisions.recommendations` from `GET /v1/status` | `POST /v1/recommendations/{id}/promote` | The record reads `promoted` in the scoped read |

All carry `project`, `action`, `op_id` and `revision`. A method's `spec` passes AssetQ's own validation; DashQ checks only that it is a JSON object. A submit carries no permission or configuration field, and `max_credits` is the per-job budget AssetQ enforces; DashQ adds no budget of its own.

The revision check narrows a race but cannot close it: AssetQ has no conditional write. DashQ serializes its own actions per project, and a receipt names the resulting version, so a concurrent change is visible. A conditional write belongs to AssetQ.

## Journal and delivery

The D11 store keeps `assetq_ops`: operation ID, actor digest, workspace, project, action, revision, payload digest, state, receipt and time. Text, specification and constraints are not stored; receipts hold IDs, versions, states and AssetQ error codes.

- A stored terminal receipt repeats without dispatch. The same ID with a different digest is refused.
- `pending` is recorded before dispatch. A lost reply or restart leaves it `unknown`.
- `Check delivery` resends the operation in the page session. It never dispatches a second effect: `job.submit` repeats the same idempotency key, which AssetQ resolves to the same job or refuses. `job.cancel`, `method.save` and `recommendation.promote` only read back; without visible evidence they stay `unknown`.

Receipt states are `accepted`, `stale`, `refused`, `unknown`. Only `accepted` follows a verified readback. AssetQ's `402`, `403`, `409`, `413`, `422`, `429`, `503` and `507` replies read `refused` with its error code; a timeout reads `unknown`.

## States

| Case | Presentation |
|---|---|
| Not configured | No navigation entry and no panel; `/assets` names the missing configuration |
| Loading or read failure | Last values stay, labelled `unconfirmed` with their time |
| Empty list | `No active jobs` or `No comparisons` only when the read is `ok`; otherwise unknown with the reason |
| Capped list | `≥N (capped)` and `incomplete` |
| Long-running | Step, effect and last AssetQ update; no percentage or remaining time |
| Cancelled | `requested`, `provider_confirmed`, `locally_stopped` and `uncertain` stay distinct |
| Failed | AssetQ's bounded reason; generation success, validation and owner acceptance stay separate |
| Denied | A disabled action names the missing grant, credential or role |
| Refused or stale | The draft stays; the receipt names the reason |

## Visual candidate

One `/assets` page in the existing dense operator style, with one group per project: jobs, then storage, then methods, comparisons and retained records in disclosures. Action forms stay closed until opened, and only offered actions appear. The candidate uses invented data and the shared page chrome:

- [Desktop, 1280 px](assetq-controls/assets-desktop.png)
- [Narrow, 360 px with actions open](assetq-controls/assets-narrow.png)

Owner approval is recorded on the task; the images are layout evidence, not live qualification.

## Qualification and rollback

Fixture and adapter checks prove the projection and gates, not a deployment. Production needs the published AssetQ service, owner-provisioned credentials, grants and config, and the amended AssetQ contract; none of these is implied by this package.

Rollback: remove `assetq` from the config and restart, or remove one grant. This stops further reads or dispatch; it does not undo AssetQ jobs, versions or promotions. Journal rows stay.
