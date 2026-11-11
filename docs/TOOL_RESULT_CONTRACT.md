# Tool result contract and runtime integration

Status: connected to the executable's owner/guest shell runner, browser launcher, model tool responses, stored chat history, readable Copy text and task details. Existing plain-text tool messages and `Browser action v1:` records remain readable. No database rewrite, automatic replay or new tool permission is introduced.

`src/lib.rs` exposes `tool_result` and `tool_action`. The initial contract describes process-backed tools. HTTP-only or other future adapters need explicit mapping or a schema extension; they must not invent a process exit code. The CLI `--history` also renders the new records as readable reports.

## Data and invariants

`ToolResult::new(scope, observation)` constructs a validated, immutable result. `scope()` and `observation()` expose shared references. Scope contains caller-supplied request ID, action ID, requester, tool and workspace. The app supplies a session/task request ID, an incrementing action ID, and the actual owner or guest requester. The working directory comes from the effective execution policy. For chat-only requests, blocked results omit the owner workspace. Standalone callers without an ID receive a process/time/counter ID. Metadata grants no permissions; the request-local approval channel still approves only its captured immutable command and workspace.

The observation keeps execution facts separate from verification of the intended effect:

- `succeeded` requires a started process with exit code zero. It does not imply that a browser page was opened or another intended external effect occurred.
- `failed` may describe a started process with a nonzero/unknown code, or a failure to start with no exit code.
- `timed_out` requires a started process. Keep the observed code if available, even if a signal handler exits zero. A parent-side timer records the stop reason; arbitrary output and exit codes 124/137 alone do not establish a timeout.
- `cancelled` can describe cancellation before or after execution started. The parent records whether it sent a stop signal. Owner commands retain their 20-second bound and finish under that bound after Stop; guest commands retain no fixed time limit and Stop signals their process group, escalating from TERM to KILL after two seconds.
- `denied` and `blocked` require no execution, exit code or captured output, and verification `not_applicable`.

`Verification` is `unverified`, `not_applicable`, or `verified` with nonempty evidence. Only successful outcomes may claim a verified intended effect. Evidence remains a report from the caller, not something the library independently checks. A successful browser launcher must use `unverified` until an independent observation actually exists.

`effects_may_have_occurred()` is true for any started process, including failures, timeouts and cancellations. It is not an instruction to retry and does not prove that an effect occurred. Authentication-required diagnostics remain failures rather than owner denials.

Diagnostics use an explicit category and bounded message. They do not parse stdout/stderr, decide policy or trigger retries. The adapter remains responsible for faithful classification.

## Captured output and stored records

`CapturedStream::capture(bytes, truncated_upstream)` retains at most 16 KiB of UTF-8 text per stream. It handles invalid UTF-8 lossily, preserves complete characters at the cap, and marks actual local/upstream truncation. The executor drains both pipes, retains a three-byte lookahead to preserve UTF-8 boundaries, and reports background pipes that remain open. The contract helper itself does not run processes. Control characters remain data for faithful copying; renderers must escape them before terminal display, as existing UI code does.

`to_json()` emits schema version 1. `from_json()` rejects oversized records before parsing, unsupported versions, unknown fields and inconsistent observations. Its 384 KiB whole-record bound allows JSON escaping of all bounded fields. Direct Serde deserialization performs the same field/invariant validation, but callers reading external history should use `from_json()` for the additional whole-record bound.

Deserializing a result never executes anything. Neither a stored `verified` claim nor a scope identifier is trusted approval. Remote records must remain conversation data under the existing policy.

## Stored action envelope and presentation

New actions use `Tool action v1: ` followed by JSON containing the exact command, optional browser target and validated result. `tool_action::Action::decode` accepts the record at the start of a tool message, optionally after one hub account prefix; it never searches inside arbitrary output. Readers call it only for tool messages when creating actionable UI targets.

Both stdout and stderr can be reduced further to keep the complete stored record within the hub's 64 KiB message limit. Reduction preserves UTF-8 and marks truncation; it never changes the command. Oversized metadata returns an error. Shell commands have an 8 KiB review limit and browser URLs a 4 KiB limit, checked before approval/execution. Full original output beyond capture/transport limits is not retained.

The model receives the same record saved as a tool message. Chat and Copy render readable reports; new browser records retain compact launch cards and clickable Details through the legacy browser reader. Process success remains unverified for intended external effects. Task details retrieve matching session/task results from the task's original chat, including after a chat switch; hub-prefixed remote reports are excluded from that local execution view. Legacy task history has no retroactively invented result records.

Denied actions save one report. Subsequent blocked calls return structured policy results to the model without producing repeated log messages or additional approvals. Invalid arguments and start failures produce unstarted failed reports. Stop before execution produces an unstarted cancellation; post-start observation failures retain `started` and uncertain effects.

## Remaining work

- Add explicit action start/end timestamps and durations; current chat/task timestamps remain the persisted timing evidence.
- Complete P1 request budgets, loop detection and richer recovery, as scoped in the roadmap. The six-model-cycle request bound remains.
- Add independent browser/page observation before ever producing verified effects; no screenshot, click or page-reading capability was added.
- Evaluate context-role preservation, skill loading and alternative backends separately.

Tests in `src/tool_result.rs` cover outcome round trips, launcher success versus verification, uncertain effects, authentication failure, denied/blocked invariants, spawn failure, cancellation, Unicode/invalid-byte caps, maximum escaped records, unsupported/malformed records and nested unknown fields. Those contract tests use no shell, browser, model, credentials or desktop clipboard. Executor/UI regressions additionally exercise temporary shell fixtures, actual timer/process-group handling, denied sentinels, requester isolation, legacy records, task association and exact Copy hitboxes. The terminal suite uses a fake model, isolated temporary databases and a fake browser launcher; it does not open a real browser.

See [the harness roadmap](HARNESS_ROADMAP.md) for P0 and later phases, and [coordination](COORDINATION.md) before touching reserved adapters.
