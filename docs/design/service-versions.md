# Service versions and diagnostics

Accepted under D18. Coreq owns installation, selection, update and rollback mechanics; each consumer owns activation and health semantics. This view is observation-only, not a new updater or remote shell.

## V1. Inventory

One row per installation and workspace: product, host/platform, observed running version, selected version, release available from the configured release source, update state and observation age. Never join installations by hostname; Windows and WSL are distinct. Coreq is a library, not a service row; display its version only when reported. Unknown versions remain unknown. Release-check time and host observation time are separate. A verified version comparison alone permits an update-available label.

## V2. Lifecycle

Availability is not selection; selection is not a running process; a running process is not verified health. Show waiting for active work, starting, healthy, checksum rejection and rollback from source evidence. Do not invent progress percentages. Failed validation preserves the previous selection. Report rollback as recovered only after fresh consumer-defined health evidence. Preserve historical running/selected values when stale, visibly marked as last known; current health is unknown. Source recovery does not prove workload or integration recovery.

AgentQ retains its connection-drain semantics, TaskQ its child-preserving platform semantics, ServerQ its completed-check health marker and DashQ its own health contract. No UI action may bypass these rules. This acceptance adds no update, restart or rollback authority.

## V3. Diagnostics

Select a row to inspect one contextual detail: installation identity, observation times, safe reason codes, health receipt, update history, queue age and engine versions where supported. Missing fields have explicit unknown/not-reported states. Use allowlisted metadata from existing adapters; never expose raw status files, command output, paths, credentials or logs. Read-only diagnostic command hints are product-owned and never execute in the dashboard. Failure of one service preserves other rows and selection.

## V4. Reference and qualification

The accepted [reference](service-versions-reference.html), [preview](service-versions-preview.html) and [manifest](service-versions-reference.sources.json) are synthetic. Demo state controls are not production controls. Match 1024px/16px light and 360px/20px dark, including waiting, startup, checksum failure, verified rollback and stale observations. Preserve selection and focus on refresh. Qualify fields against existing producer contracts and fresh source evidence; unsupported fields remain unknown. Do not deploy candidates or exercise real upgrades merely to populate this view.
