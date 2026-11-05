# AssetQ summary

Design package of [D28](../../dashq.md#d28-assetq-summary). AssetQ owns jobs, methods, comparisons, recommendations and assets; this page defines how DashQ reads and shows them. DashQ never writes to AssetQ.

## Owner request

- [#246](https://github.com/taskqorg/dashq/issues/246) goal: "Give people one source-backed AssetQ control surface for jobs, generation methods, comparisons and storage state."
- The owner's decision on that task: "AssetQ R9 stays unchanged; DashQ is read-only for AssetQ, showing source-backed summary metadata and links to https://assets.ufobe.com. Scope change: A8 and the four D27 write actions are withdrawn; remove them and their grants from PR 258 and update D27 in the design first. A6 no longer needs an R9 amendment or assetq-grant rows. Access: use option 1, an Access service token added to the existing Bearer read; the PM provisions the token, config and credential files, so name exact paths and keys needed. A7 stays read-only: selftest and /assets through the real guard show real data with unknown, stale and capped states."
- The owner's answer on the visual candidate: "Approve this page layout - + dense, current page style, forms stay closed; - long scroll to reach methods".

The decision's D27 is this section. `main` gave that number to the public limits push, so this section is D28; the meaning is unchanged.

## Use cases

| ID | Intent | Result | Failure and recovery |
|---|---|---|---|
| M1 | See what is running and whether it is healthy | Jobs by phase, active rows with step, effect, cancellation and usage, uncertain submissions, provider health | A failed read keeps the last values as `unconfirmed` with their time; missing usage is `—` with its reason, never 0 |
| M2 | Choose a method | Every version with status, operation, provider, model, limits, outcomes and qualification record | Status, qualification and provider health stay separate; a qualification record enables nothing |
| M3 | Judge alternatives | Comparison identity, dataset, evaluator and method versions, per-method metrics, missing cases, scoped recommendation or explicit none | Incomplete results stay incomplete; DashQ shows no winner of its own |
| M4 | Check storage | Store use and quota, free space and floor, metadata admission, integrity, backup, restore | Deferred or unobserved checks read unknown with AssetQ's reason |
| M5 | Follow a record or act | Links to the job, asset, download, method and benchmark pages on the AssetQ site, where people submit, cancel, edit and promote | The site authenticates the viewer and applies its own authority; a viewer without a session reaches its sign-in |

## Provisioning

Off by default: without the `assetq` key there is no read, navigation entry or panel. The PM provisions these on the target; no agent creates them. `<install>` is the directory that holds DashQ's `config.json`, and all paths in the config are absolute.

Add this key to `<install>/config.json`, with the service user as the only reader of each named file (mode 0600):

```json
{"assetq": {"base_url": "https://pipe.ufobe.com",
  "access_client_id": "<install>/assetq/access-client-id",
  "access_client_secret": "<install>/assetq/access-client-secret",
  "projects": {"<AssetQ project id>": {"workspace": "<D11 workspace>",
    "read_token": "<install>/assetq/<AssetQ project id>.read-token"}}}}
```

| File or key | Holds | Source |
|---|---|---|
| `access_client_id`, `access_client_secret` | One Cloudflare Access service token for the AssetQ site: its client ID and client secret, one value per file | A new service token, allowed by a Service Auth policy on the Access application of `base_url` |
| `read_token`, one per project | One Pipe `member` Bearer token for that project | `pipe token <project> --role member` |
| `workspace` | The D11 workspace whose members may see that project | The existing workspace |
| `base_url` | The AssetQ origin, `https://` with no path; every read and link derives from it | The origin that answers at provisioning; the read follows no redirect |

The two Access keys come together or not at all; one alone refuses the config. Without them a read of an origin behind Access reads unknown with the reason `redirected to a sign-in`.

AssetQ has no read-only role, so `member` is the smallest grant it offers. DashQ's GET-only read adapter keeps reading from mutating: it has no code path that sends another method to AssetQ. Access decides whether a request reaches AssetQ; the Bearer token decides what AssetQ returns. Neither value reaches the browser, `/data`, logs, URLs or `report` (R7): the headers go to `curl` on stdin.

[assetq#51](https://github.com/taskqorg/assetq/issues/51) moves the service to `https://pipe.ufobe.com`; Pipe's contract refuses requests that name `assets.ufobe.com`. Configure `base_url` for the provisioned origin and allow the service token on that origin's Access application; the Bearer tokens stay valid. No origin is hardcoded in the adapter.

Config errors name the bad key, never its value (D2). An unreadable credential file reads `credential unreadable`.

## States

| Case | Presentation |
|---|---|
| Not configured | No navigation entry and no panel; `/assets` names the missing `assetq` key |
| Before the first read | Unknown with the reason `refresh in progress`; a browser request never waits for AssetQ (D4) |
| Loading or read failure | Last values stay, labelled `unconfirmed` with their time |
| Stale | After 180 s without a fresh read the values read `stale`, with their age |
| Empty list | `No queued, active or waiting jobs`, `No failed jobs` or `No comparisons` only when the read is `ok`; otherwise unknown with the reason |
| Capped list | Its count reads `≥N (capped)` and the source `incomplete`; a list whose read failed counts `—` with `unknown` |
| Long-running | Step, effect and last AssetQ update; no percentage or remaining time |
| Cancelled | `requested`, `provider_confirmed`, `locally_stopped` and `uncertain` stay distinct |
| Failed | Failed jobs, in a closed disclosure inside the jobs group: AssetQ's bounded reason and the job page, with no result link. Generation success (job phase), validation (verified manifest) and owner acceptance (retained record) stay separate |

## Approved layout

One `/assets` page in the existing dense operator style, with one group per project: jobs, then storage, then methods, comparisons and retained records in disclosures. The owner approved this layout with closed action forms; the forms are gone by the later decision above. These images show the current read-only page with invented data and the shared page chrome:

- [Desktop, 1280 px](assetq-summary/assets-desktop.png)
- [Narrow, 360 px](assetq-summary/assets-narrow.png)

The images are layout evidence, not live qualification.

## Qualification and rollback

[Evidence](evidence/assetq/README.md) records the stub and browser checks and the live readback that the PM runs after the release.

Rollback: remove `assetq` from the config and restart. This stops the reads. AssetQ state is untouched, because DashQ wrote none.
