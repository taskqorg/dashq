# Host-first runtime review (#170)

The runtime implements the [approved #169 composition](first-screen.md) under D8–D10/D22 and R13. The owner [approved the five runtime screens](https://github.com/taskqorg/dashq/issues/170#issuecomment-6319532742) at `eaabcc879ff51505d5ce29027e00c7558a80e034` under TaskQ § 7, assigning the two named TaskQ handoffs to the PM and keeping A1 live qualification and CI acceptance open. This is not a deployment receipt.

## Behavior and preservation

- Hosts fill the overview; each row groups identity, observation age, product versions/update state, sign-in gaps, CPU, RAM/filesystem bars and observed attempts. Host links open resource detail; counts open host-scoped TaskQ tasks. Unknown online/sign-in/assigned-queue values stay unknown. Existing canonical host identities remain authoritative; no friendly-label mapping or new host authority is added.
- Attention contains TaskQ questions/failures and ServerQ diagnostics links. Model-call errors, routing/coverage observations and history remain in detail. Empty Attention says `No actions requested`, as the approved design requires; it does not claim all sources healthy. Source status is collapsed, with every non-ok reason.
- Tasks uses unchanged TaskQ v4.4.0 project/answer assets and the existing D13 adapter. Host scope includes only fresh, canonical heartbeat attempts; shared Ready is never multiplied into host queues. All hosts/unassigned work remains available. D14/D16 controls retain their existing components, grants, revisions and receipts.
- The old side-by-side composer and duplicate Hosts table are deleted. Resource history/policy components, boards, agent use/optimization/work, versions, checks/findings, connections and integrations retain their [ledger destinations](first-screen.md#preservation-ledger). Stale/revoked connection records require the explicit filter or a direct anchor; history is retained.
- Automatic 60 s refresh retains local disclosures, focus, reading position and drafts. Failed refresh keeps evidence, advances ages and marks retained values unconfirmed. Stable view URLs and direct question links use the same scoped page and single answer card.

## Review set

Five runtime captures, compared with the same invented host/task/resource scenario from `host-first-fixture.json`. Login, online and assigned-queue inputs in that design fixture have no qualified adapter, so the runtime deliberately renders them unknown. GB conversion uses the existing renderer's decimal units; the design fixture is in GiB. No invented sign-in failure, host-health receipt, queue assignment or policy threshold is promoted into runtime capability.

| Runtime capture | Comparison / remaining gap |
|---|---|
| [Hosts, 1280 light](evidence/host-first-runtime/hosts-1280-light.png) | Same host groups, products, resources and task entry points; independent source times remain accessible. |
| [Hosts, 1280 dark](evidence/host-first-runtime/hosts-1280-dark.png) | Shared grouping and palette; warning/unknown text remains visible. |
| [Hosts, 360 dark](evidence/host-first-runtime/hosts-360-dark.png) | Labelled groups reflow without smaller main text or horizontal overflow. |
| [Host detail](evidence/host-first-runtime/host-detail.png) | Real fixture sample history and existing evidence; no history from a single reading. |
| [Tasks](evidence/host-first-runtime/tasks.png) | Pinned shared rows, counters and answer card; full tray controls remain unavailable. |

[Manifest](evidence/host-first-runtime/manifest.json) identifies renderer/asset hashes, Chromium, platform, matrix cells and capture-time untested claims. The manifest predates the owner answer; its images and measured source hashes remain unchanged. Captures are Windows/Chromium fixture evidence, not Linux production or native-tray qualification. The linked owner answer records runtime visual acceptance (A5, TaskQ § 7).

## Checks and gaps

Before: `cargo test --locked host_first_actions_and_unknowns` failed with `hosts must precede Attention`. After: the same regression passes. It retains unknown-data protection and rejects the obsolete composition and telemetry-only action rows. Updated detail checks retain resource gaps/scales, filesystem bars, TaskQ component reuse, enrollment receipts and source-failure protection (Testing § Before adding or changing a test).

`tests/host-first-browser.cjs` checks 24 width/text/theme/view cells and navigation/direct-answer/host-scope behavior. Existing startup, resource, findings and limits browser chains run against their new destinations; their draft, outage, authority and receipt protections remain. CI runs the new check using the existing browser setup; publication acceptance remains merge-result CI, or private exact-SHA fallback (R12/§ 7).

Frozen review found three implementation defects, corrected within this task:

| Defect before | Retained regression / after |
|---|---|
| Linux/WSL resources became `desktop/native` instead of `DESKTOP/wsl`; case variants could add rows. | `review_resource_runtime_identity` failed on that value, then passed for unfiltered and host-filtered entry. Data normalizes through H1's shared runtime/installation mapping before filtering; grid identity is case-insensitive. |
| A superseded version row supplied the selected installation's update state/time. | `review_version_owner_evidence` failed on `wrong installation evidence`, then passed. Exact owner/version-compatible evidence wins; incompatible receipts cannot qualify the displayed version. |
| Persistent Attention header returned to its initial period/host filter. | Frozen `6d95e5c` header reproduced `period=1800` after selecting 86400; the runtime browser check now retains period/host/agent/session. |

The Tasks capture also exposed an initially collapsed first board. First entry now shows its shared rows; explicit disclosures survive refresh, and selecting counters opens the requested rows. Existing draft/outage checks remain.

TaskQ-owned gaps have these concrete handoff task names:

1. **Publish shared browser task workspace and controls for DashQ** — versioned tray-compatible detail/menu API, capabilities, expected revisions, operation IDs, receipts, fixtures and release hashes. Start, Stop, Later, Close and Retry cannot be imitated here; ordinary answers and released resource controls keep their existing gates.
2. **Publish scoped host online, client-login and assigned-queue observations for DashQ** — explicit producer identity/scope, observation/probe time, validity and completeness; no inference from activity, silence or global board counts.

These are **unfiled handoffs**, not completed or linked TaskQ tasks. Filing the first through the worker failed with `TASKQ_ROOT ... is not the project of the current directory`; the worker did not bypass the project guard. The owner assigned filing/readback and supplying their links to the PM. A3's handoff filing therefore remains open. #169 already approved retaining the capability gaps; no replacement control contract is adopted here.

Production deployment and representative live-source readback at `dash.ufobe.com` remain unverified (A1/R12). The read-only site probe returned HTTP 302 to the existing access guard. Held [#173](https://github.com/taskqorg/dashq/issues/173), read back after creation, records the deployment/live-readback blocker and requires an owner grant before deployment. A5 runtime screen approval is recorded above. No release or deployment was performed.

## Linux CI repair (attempt 6319734132)

Before: [CI run 39624898786](https://github.com/taskqorg/dashq/actions/runs/39624898786) failed on `994abf4826b321810202661b033a3353a3dd486e` at `tests/host-first-browser.cjs:45`, `hosts start first`. The unchanged check reproduced in Linux Chromium 141 / Playwright 1.56.1 (Ubuntu WSL). At 360 px / 20 px, Linux font metrics wrap the header and place Hosts at y=259; Windows places it at y=193. Hosts is the first visible section in both, before Attention and inside the 900 px viewport. The 220 px ceiling has no basis in D8/D10 or the approved composition.

After: the same `node tests/host-first-browser.cjs` passes all 24 cells in Linux Chromium. The check now requires Hosts to be the first visible section, inside the first viewport and before Attention, and explicitly verifies the requested body font size. Overflow, themes, widths, navigation, filters, direct questions, unknown values and no-write checks remain. This corrects the assertion under Testing § When evidence disagrees and TaskQ § 7.4; it changes no product rule or accepted visual. The reviewed renderer, assets, fixture and five screenshots are unchanged. Linux Rust/adapter qualification and R12 publication CI remain separate from this browser evidence.

The existing `node tests/resources.cjs` also exposed Windows CRLF conversion of the pinned `fixture/taskq-resources/resource-policy-v1.json`: SHA-256 `92a61801…` instead of published `edfccf01…`. The missing `-text` entry now joins the existing vendor-byte protections in `.gitattributes`. Restoring the committed bytes makes the unchanged resource scenarios and immutable-hash assertion pass; the fixture's Git blob remains `fb6cec452341fcd7d361c5fed15232a8ba7d7f6c`. No pin, fixture content or authority test was relaxed.
