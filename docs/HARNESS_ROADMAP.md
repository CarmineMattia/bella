# Harness improvement roadmap

Updated: 2026-11-10. Status: planned; the unchecked items below are not delivered features.

Carmine's direction: improve execution reliability first, then skills and future integrations with other bot/agent interfaces. Use the public OpenAI Codex harness as a reference while preserving local inference and application-owned permissions. This document supplements `PLAN.md`; it does not replace the networking, identity, Android or release milestones.

## Baseline and coordination

The original planning baseline was `380ed82`; implementation validation targets the integrated Bella base (`7ce243b`, subsequently merged with GitHub at `6e4f17d`). It already provides streamed replies, local shell and browser-launch tools, exact-action owner approval, request-wide denial enforcement, bounded shell execution, interruption records and task history. The model context takes the last 24 messages; a request has at most six model cycles. Browser launch acceptance is explicitly different from visual verification. See [tool actions](TOOL_ACTIONS.md) and [browser actions](BROWSER_ACTIONS.md).

Remo recorded the quota, Bella rename and opt-in guest shell tools as integrated into `main` at `7ce243b`. Keep owner and guest execution policies distinct: owners retain bounded commands, while the integrated guest runner has separate stop handling and no fixed time limit. Guest capabilities must always come from the effective application policy; a dedicated working directory alone is not a filesystem sandbox.

Read [coordination](COORDINATION.md) and reserve files before each implementation slice. Remo released the guest-tools reservation after integration; always recheck live reservations rather than treating this snapshot as a lock. This roadmap does not authorize editing reserved files, committing, pushing or creating remotes.

## P0 — Reliable execution and truthful outcomes

Runtime integration (2026-11-10): shell/browser executors now emit validated versioned action records with session/task/action IDs. Model tool responses, chat/history, Copy and task details use those records; old plain text and browser records remain readable. Real parent timers distinguish timeouts from arbitrary exit codes. See [the contract and remaining boundaries](TOOL_RESULT_CONTRACT.md). Explicit per-action timestamps/durations remain open, so P0 as a whole is not complete.

Completed slices: standalone contract, then runtime integration after Remo released the reserved files. Next harness slice: explicit timing metadata and P1 recovery/budgets.

- [ ] Define versioned request, action and result records with stable request/action IDs, tool name, workspace, requester, timestamps and policy scope. Keep approval decisions separate from execution outcomes.
- [x] Represent execution outcomes such as succeeded, failed, timed out, cancelled, denied and blocked. Track verification separately: a successful launcher can still have an unverified external effect.
- [x] Preserve exit code, diagnostic category, bounded stdout/stderr, actual truncation flags and whether execution started. A timeout must not imply that no side effects occurred.
- [x] Bind approval to the exact action, arguments, requester, workspace and request. Changed arguments require a fresh decision; skills and adapters cannot grant approval.
- [x] Use the same factual result for model context, chat cards, task details and persisted history. Keep older stored browser records readable.
- [x] Preserve the current denial rule: one owner denial blocks all remaining tools for that request. A later user request starts its own policy evaluation.

Acceptance: deterministic tests cover command success, nonzero exit, authentication required, timeout with uncertain effects, denied execution, unavailable tool and browser launch without visual confirmation. Assert both the displayed state and actual execution count. Denied/blocked calls execute nothing; approved outputs remain available. Test serialization and legacy-record reading before changing storage.

## P1 — Recovery, budgets and cancellation

- [ ] Replace fixed loop limits with explicit, bounded request budgets: model cycles, tool calls, elapsed time and output/context size. Keep conservative defaults and expose why a request stopped.
- [ ] Detect repeated equivalent failed actions and stop unproductive loops. Offer a useful diagnostic alternative when supported by evidence.
- [ ] Classify failures so the agent can continue relevant non-privileged checks after authentication failure without treating failure as owner denial.
- [ ] Never automatically replay state-changing actions after timeout, interruption or reconnect. Reconcile observed state before proposing another action.
- [ ] Define cancellation at each stage: model streaming, pending approval, tool execution and final response. Preserve the existing bounded handling of already-approved commands until an explicit replacement is implemented and tested.
- [ ] Record incomplete work and the reason it stopped; do not present budget exhaustion as successful completion.

Acceptance: a simulated sudo failure leads to an appropriate non-privileged diagnostic proposal; repeated failure ends within budget; denial produces no further execution; Stop and restart do not replay an approved action. Use fake adapters for side-effect scenarios, with an optional local-model check kept separate.

## P2 — Context continuity and scoped memory

- [ ] Budget context by tokens/size rather than only a message count, accounting for instructions, tool schemas, skill metadata and results.
- [ ] Preserve real message/tool roles and action provenance across turns instead of flattening execution records into ordinary chat text.
- [ ] Add a compact checkpoint containing the user's goal, constraints, observed results, pending work and unresolved questions, with references to original records.
- [ ] Preserve denial and approval state in application code; a summary must never recreate permission or authorize replay.
- [ ] Scope context to the chat, requester, bot and effective owner/delegate policy. A shared bot must not retrieve unrelated private conversations.
- [ ] Rebuild usable context after restart and allow inspection of the checkpoint. Store summaries as derived context, not as new user instructions.

Acceptance: a long conversation retains an early constraint and a relevant tool failure after compaction; restart retains pending work without executing it; a guest/group cannot retrieve another chat's content; stale or missing summaries fall back to source history safely.

## P3 — Layered prompts and capability-aware tools

- [ ] Separate global behavior, runtime capability descriptions, project instructions, bot persona and selected workflow instructions.
- [ ] Build tool availability from the effective policy and adapter capabilities. Enforce policy before execution even when a model invents an unavailable tool call.
- [ ] Keep output, remote messages, retrieved documents and skill content distinguishable from trusted runtime policy.
- [ ] Describe limitations accurately: opening a browser does not provide page reading, clicking or screenshots.
- [ ] Test prompt size and behavior with the actual local model; do not copy a large Codex prompt wholesale or assume identical instruction-following.

Acceptance: a chat-only policy advertises no execution tools and rejects fabricated calls; a tool-capable policy describes only implemented tools; persona/project text cannot bypass runtime checks. Public prompt examples are references, not claims about every Codex deployment's complete runtime instructions.

## P4 — Small, inspectable skill system

- [ ] Define a `SKILL.md` loader with name, concise description, instructions and optional local references/scripts. Declare compatibility with any chosen public skill specification only after testing it.
- [ ] Load names/descriptions first; read full instructions only when selected. Support explicit skill selection and show which skill is active.
- [ ] Start with Linux diagnostics, crash investigation and browser-action verification. Add skills in response to recurring tasks, with clear trigger and non-trigger examples.
- [ ] Make discovery roots, disabled skills, provenance and versions inspectable. Handle duplicate names, malformed front matter and missing resources predictably.
- [ ] Bound discovered metadata, instruction size and resource traversal. Review skill scripts before enabling them; running a script uses normal tool policy and approval.
- [ ] Keep installation explicit. Do not silently download Codex skills, install dependencies or give skills new permissions.

Acceptance: explicit and implicit selection tests, negative trigger cases, malformed/duplicate skills, bounded loading and denial enforcement. A browser skill cannot claim page control when only a launcher exists. Instructions alone do not create a tool capability.

## P5 — Backend adapters and future bot interfaces

- [ ] Define a provider-neutral contract for start, continue/resume, streamed events, approval requests, cancellation and completion, with stable IDs.
- [ ] Separate model transport, execution runtime and chat UI. Retain the local inference backend while evaluating alternatives.
- [ ] Publish a capability description per backend: tools, context/compaction, structured output, resume and cancellation semantics. Unsupported features must be visible.
- [ ] Evaluate Codex app-server for persistent conversations/events/approvals and Pi using the same representative tasks. Evaluate other bot interfaces only through documented integration surfaces.
- [ ] Keep final authorization in the application; backend-built-in tools or extensions must not bypass owner/delegate restrictions. Document any backend that cannot satisfy this requirement as unsuitable for that mode.
- [ ] Decide separately how to route models, store provider sessions, handle credentials and recover connections. Do not migrate existing chats or credentials as a side effect of adding an adapter.

Acceptance: the same fake-backend scenarios pass for every adapter, including denial, unavailable capability, disconnect/reconnect, duplicated events and cancellation. A reconnect cannot duplicate execution; permissions remain bound to the original requester. Real-service tests are opt-in and report cost/access prerequisites.

## Cross-cutting — Regressions and observable evidence

- [ ] Establish a small, versioned task suite from actual failures: status versus upgrade, sudo authentication, launch versus page verification, denial, loop exhaustion, long context and restart.
- [ ] Record the harness/prompt/skill/model configuration with each evaluation. Distinguish deterministic protocol tests from nondeterministic model evaluations.
- [ ] Measure task completion against observed evidence, tool errors/repeats, latency and context consumption. Use repeated local-model runs before claiming improvement.
- [ ] Make execution timelines inspectable using existing task/message records; avoid a separate unrestricted log of private prompt/output content.

Completion rule for every slice: targeted tests and relevant existing checks pass, user-visible behavior is verified, limitations are recorded, and the coordination entry moves to Done with actual results. Checklists become checked only after reviewed implementation and validation, not after writing a prompt or plan.

## Public references

Code references are pinned to the reviewed Codex revision `37103d7f01e82162472c6fba7b4088d5a1303321`; upstream may change. These are inspiration and integration references, not dependencies installed by this roadmap.

- [Codex as a platform: harness and integration choices](https://developers.openai.com/blog/codex-as-a-platform).
- [Public base instructions](https://github.com/openai/codex/blob/37103d7f01e82162472c6fba7b4088d5a1303321/codex-rs/protocol/src/prompts/base_instructions/default.md).
- [Public model-specific prompt example](https://github.com/openai/codex/blob/37103d7f01e82162472c6fba7b4088d5a1303321/codex-rs/core/gpt-5.2-codex_prompt.md).
- [Compaction/checkpoint prompt](https://github.com/openai/codex/blob/37103d7f01e82162472c6fba7b4088d5a1303321/codex-rs/prompts/templates/compact/prompt.md).
- [Embedded skill installation](https://github.com/openai/codex/blob/37103d7f01e82162472c6fba7b4088d5a1303321/codex-rs/skills/src/lib.rs) and [skill authoring guidance](https://learn.chatgpt.com/docs/build-skills).
- [App-server implementation notes](https://github.com/openai/codex/blob/37103d7f01e82162472c6fba7b4088d5a1303321/codex-rs/app-server/README.md).
