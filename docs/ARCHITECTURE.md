# Alpha architecture

## Runtime

`start` invokes `scripts/launch.py`. The standard-library launcher resolves profile/data paths, runs first-time setup when needed, checks or starts installed local inference, then execs the packaged Rust app. It never builds, downloads a model, or installs software.

The Ratatui/Crossterm UI in `main.rs` and `ui.rs` runs on the foreground thread. User input and storage updates happen there. `editor.rs` handles UTF-8 insertion, cursor motion and a horizontal viewport. A connection-check thread performs short model-list requests. `settings.rs` handles validated profile configuration, atomic writes and first-run setup.

`agent.rs` creates a dedicated Tokio runtime per task. Async HTTP permits cancelling a stalled connection as well as a streaming response. SSE bytes are buffered across chunks before UTF-8 parsing and tool-call fragment assembly. Approval uses an application-owned channel. The shell executes on a blocking worker only after a decision is persisted and the approval signal is sent.

Stop aborts inference and pending approval. The runtime waits for an already-approved execution worker before reporting cancellation, which preserves its tool result. An approved command uses GNU timeout; output drains are capped, and pipes held open by background children do not cause an indefinite join. Normal UI exit follows the same cancellation path; a second interrupt forces exit and records interruption.

## Storage

`store.rs` opens SQLite with WAL and a three-second busy timeout. Existing message-only databases are migrated additively. No release migration deletes conversations or overwrites preferences.

- `messages`: ordered conversation events, author, body, kind, UTC timestamp.
- `conversations`: persistent room names, including empty groups.
- `chat_state`: per-name/room read cursor and draft.
- `conversation_preferences`: per-name/room pin and archive flags.
- `app_sessions`: process PID and Linux process-start identity, with a unique session ID.
- `local_presence`: the profile and room currently viewed by each local session.
- `agent_tasks`: request, state, current received text, last shell command/approval, timestamps, error, room and owning session.

Read cursors advance only to the loaded tail when it is being viewed. A historical search window does not mark newer events read. A message/line anchor keeps the viewport stable when new events arrive. Active and archived folders filter the same conversation records, and pin order is per owner.

Task approval decisions are committed before signalling the execution worker. Task completion and its final chat event are committed together. Streaming checkpoints are written once per UI event batch. On startup, recovery compares the saved PID's start identity so PID reuse cannot make a dead session appear live. It records each dead session's active tasks as interrupted and recovers received text once, transactionally. It does not replay a task, infer command success, or restore a pending approval as authorization.

`VACUUM INTO` produces a coherent backup including committed WAL state, and refuses an existing destination. Profile JSON and model files are outside SQLite.

## Model process ownership

A file lock serializes model startup for a profile. A launcher-created server has a new process group and a registry containing its PID, start identity, executable and endpoint. Stop validates that identity before signalling the group. A discovered external server is reusable but has no owned registry and is not stopped. Startup failure cleans up a child created by that startup attempt; a second session waiting for an existing owned startup does not kill it.

## Boundaries

Local mode uses a database shared by processes on one computer. The optional hub transport is described below. Names, room prefixes, and profile settings are not trust boundaries. The process user controls approvals; shell access is unrestricted outside the initial directory. `PLAN.md` retains the later identity, network, delegation and Android work.

## Editable identities and pictures (alpha.2)

`profiles.rs` stores human and bot presentation separately from stable authorship. Human keys are startup account IDs; bot keys combine that owner with an immutable handle. Existing messages retain their authors. `profile_ui.rs` provides the editors, chat bot manager and truecolor half-block avatar renderer. Imported PNG/JPEG files are size/dimension bounded, center-cropped and decoded into a 24 × 24 RGB thumbnail. Only thumbnail pixels are serialized in SQLite, so backup does not depend on external image files.

Additive tables: `profiles` contains profile JSON, `room_bots` contains per-owner/chat enablement, and `task_bots` binds durable task IDs to their bot author. The original personal bot keeps its old author key. New tasks snapshot model, author and instructions. Recovery uses `task_bots` and falls back to the legacy personal-bot key for old tasks. Completion and interrupted fragments preserve the originating bot. Only the selected bot's complete messages enter its assistant role; other agents are labeled conversation input. The HTTP endpoint, workspace and tool-approval path remain shared with the owner's default configuration.

## Hub transport (alpha.3)

`network.rs` adds an opt-in client without changing local-mode storage. Authentication happens before chat in a masked terminal screen; the saved bearer token lives in a mode-0600 sidecar file. Cache filenames contain stable hub/account IDs. `server/hub.py` provides the stdlib HTTP(S)/SQLite hub, invite registration, PBKDF2-SHA256 passwords, hashed expiring session tokens and account-bound event validation.

Network cache triggers enqueue message/profile/room/bot-membership changes inside the transaction that writes them, including task completion. A background blocking HTTP worker sends bounded batches, records acknowledgments, fetches ordered events and advances a persistent cursor atomically. Imported changes suppress outbox triggers; account-bound sent IDs avoid self echoes. File locking serializes workers sharing one cache. Failed temporary requests remain queued; permanent rejections stay visible and can be retried manually. Local approval channels/task execution are never reconstructed from incoming network messages.

The hub's ordinary groups and public profile presentations are visible to registered accounts. `agent:ACCOUNT` room events are filtered to that account, while the hub administrator can still read its database. Instructions/model/workspace stay on the invoking device. Network membership enforces bot ownership; it is not private group access control. Profile defaults originate at registration, so a new client cannot overwrite an existing remote profile while history is still catching up.

Entity-version receipts protect newer acknowledged profile/membership edits from older catch-up events; pending local edits keep their optimistic public presentation until sent. Message IDs still follow local cache insertion order, so authoritative timeline reconciliation after offline optimistic sends remains future work.

## Composer recipient selection (development update)

`recipient.rs` handles explicit Group/Bot selection and previews. `composer_recipients(owner,room,handle)` stores a local preference without adding a network event type. Group messages sent through a selected bot are serialized with their `@handle`, so existing agent routing, durable task requests, ownership validation and shared history retain the intended recipient. Human message insertion and mention-driven recipient persistence commit atomically. Private agent rooms default to the personal bot. The control never runs a job on an incoming message; disabled named bots reject the send while retaining the draft.
