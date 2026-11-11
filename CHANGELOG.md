# Changelog

## Unreleased — 2026-11-11

- Skills, explicit selection only. Start a request to your own agent with `$name` to add one skill's instructions to that request. A chat note and the header status show the active skill. An unknown or disabled name sends nothing and keeps your draft. `/skills` shows roots, enabled and disabled skills, versions and diagnostics. `/skills disable|enable NAME` is saved as an optional `disabled_skills` list in `settings.json`; older files load unchanged. Owner requests carry a compact metadata block of at most 2,500 characters. Skills never change tools, approvals or denial. Guests of lent bots never get skills. Bella ships `linux-diagnostics` and `browser-check` (installed to `<data-dir>/bundled-skills/`, lowest priority, overridable in `<data-dir>/skills/`).

## Unreleased — 2026-11-10

- Optional guest shell for lent bots (level 3), off by default. A new per-bot, device-local *Guest tools when lent* setting (`off`/`shell`) is never synced. When enabled, a guest request may propose `shell` only — never `open_browser`. Every command still needs the owner's approval, which names the guest and the working directory. Commands run in `<data-dir>/guests/<guest>/` with a scrubbed environment, CPU/RAM caps and no time limit; Stop still kills a runaway command. This is not a sandbox: the process runs as the owner's Linux user and can read files outside that folder. Guest tool use counts against the existing per-guest hourly request quota. The owner's own tools are unchanged.
- Renamed the project to Bella, after the familiar greeting. The Cargo package and binary are now `bella`; `./start --version` prints `bella 0.1.0-alpha.3`.
- Installed data and settings now live in `$XDG_DATA_HOME/bella` or `~/.local/share/bella`. If that directory is missing and a leftover `term-social` directory is present, the launcher renames it into place, or copies it when a rename is not possible. A failed copy leaves the original directory untouched and keeps using it. Workspace paths inside `settings.json` that pointed inside the old directory are rewritten to the new location; a workspace outside that directory is left unchanged.
- Environment variables use the `BELLA_*` prefix, with a temporary fallback to the older `TERM_SOCIAL_*` names when the new variable is unset. `./term-social` and the packaged `bin/term-social` alias remain as a compatibility shim and will be removed in a later release.

- Redesigned first-time setup with a centered responsive form, field hints, click/paste support, visible Open chat and Exit actions, and focus-following layout on short terminals.
- Added Exit to welcome in chat and /welcome. Returning saves drafts, stops active replies safely, retains already-approved command results and reopens setup with the current profile values.

- Copy buttons on all message headers preserve original text and line breaks; asynchronous native clipboard helpers have an encoded terminal fallback and honest success feedback.
- Shell tools detach from the chat terminal and use non-interactive sudo. Privilege errors and timeouts get clear diagnostic notices; failure does not disable subsequent owner-approved diagnostic checks.
- Agent instructions favor inspecting status and crash evidence over starting upgrades or guessing whether an unfamiliar package exists. Output-cap footers now appear only for actual truncation.

## Earlier unreleased changes — 2026-11-09

- Denying a tool action disables all further tools in that request, enforced before argument parsing and approval. Batched calls, later model turns and tool switching cannot prompt again; a new user request has a fresh approval flow.

- Click Details on a browser card to inspect that specific action; selection stays attached to its message ID as history updates. Only visible buttons receive clicks, with scroll, resize and latest-message overlays respected.

- Browser actions now show compact launch-requested, failed or denied cards. F11 or /browser opens launcher diagnostics with Left/Right to browse the loaded chat's actions. Browser approval is explicitly labeled.
- Website launches use xdg-open; opening the default browser uses its registered launcher without passing about:blank as a file. Explicit X11 sessions propagate their display choice to the launcher.
- Browser results sent to the agent distinguish launcher acceptance from visual confirmation; nonzero exits and spawn failures are recorded as failures.

- Visible Group/Bot recipient switch beside the composer, F10 cycling and `/to` commands. Plain follow-ups address the selected bot without repeated tags.
- Recipient selection persists per local owner/chat; explicit bot mentions select that bot for subsequent messages. Named bot routing remains explicit in saved/shared message text.
- Personal chats always address a bot; disabled or unavailable named bots preserve the draft and require choosing another recipient. Incoming messages never change your recipient or invoke an agent.
- Bot lending, chat only, as a first partial step of owner/delegate policies. It is opt-in per bot through *Lend for chat* in the F8 form and off by default. The setting stays on the device and never syncs. When another account in a shared group starts a message with `@handle`, the owner's online client answers with the owner's model and instructions.
- Guest requests carry no tool definitions and get a separate system prompt. Any returned tool call is refused without approval or execution. Context is limited to that group's history up to the request.
- Each guest gets a per-bot number of started requests per hour, set in the F8 form (*Guest requests per person per hour*, 1–60, default 5, device-local and never synced). Guest requests are handled one at a time, with no queue. Bot authors, own messages, stale backlog and duplicate messages are ignored.
- Tasks record the requester through an additive `agent_tasks.requester` migration. F5 shows delegated requests and who asked. Failures post only a generic note to the group.
- No end-to-end encryption: the hub administrator can read guest requests and replies.

## Earlier unreleased changes — 2026-11-07

- Add an explicit `open_browser` tool using the registered desktop browser and the existing owner-approval flow.
- Detect local desktop/launcher capabilities and correct generic claims that terminal agents cannot launch GUI apps.
- Validate browser URLs, quote launcher arguments, and cover prior-refusal correction with the real local model; denied launches never execute.

## 0.1.0-alpha.3 — 2026-11-04

- Optional `--network` hub mode: sign-in/registration, stable account IDs, masked credential fields and revocable saved sessions. Local chats stay in a separate database.
- Python standard-library hub with SQLite, invite-gated registration, password/session hashing, request/concurrency bounds, account-filtered personal agent rooms and optional TLS.
- Transactional outgoing queue, idempotent server receipts, reconnect/catch-up, isolated hub/account caches and serialized sync between local windows.
- Shared public groups, user/bot profile pictures and owner-controlled named bot membership. Remote mentions never invoke local agents or approve shell commands.
- F9 connection/rejected-event view, queue/server receipt labels and retry; new protocol/real-terminal network tests and setup documentation.

## 0.1.0-alpha.2 — 2026-11-04

- In-app user profiles (F7): editable display name, About text and PNG/JPEG picture, with stable account IDs preserving history and drafts.
- Bot manager (F8): create/edit named bots, give them pictures and instructions, optionally choose another model on the configured server, and add/remove them per chat.
- Mention routing via `@handle`, per-bot assistant context, and durable bot identity across completion, cancellation and crash recovery. Existing @bot remains available and editable.
- Terminal avatar thumbnails in messages, chat cards, profiles and participant/bot lists; SQLite backups include imported pictures and bot settings.
- New unit/integration and real-terminal coverage for profile validation, imports, backups, bot configuration, persistence and routing.

## 0.1.0-alpha.1 — 2026-11-04

First packaged Linux alpha for local testing.

- Messenger-style terminal interface with incoming/outgoing bubbles, chat previews, UTC timestamps, unread state, mouse controls, and responsive narrow layouts.
- Named local groups, personal agent contexts, persistent drafts, pins, archives, and full-history search with highlighted results and paged history.
- Streaming local-model replies, exact-command owner approval, denial, bounded shell execution/output, Stop, retry, and graceful shutdown.
- First-run configuration, saved profiles, model diagnostics, automatic reuse/startup of installed local inference, and identity-checked shutdown of launcher-owned servers.
- Durable task records, atomic completion plus chat delivery, interruption recovery after crashes, recorded approval decisions, and activity inspection. Crashed work is never automatically replayed.
- PID-verified local room presence and historical participants.
- Consistent SQLite backup, diagnostic JSON, release/source archives, checksums, and an isolated terminal/launcher acceptance suite.

The alpha uses local SQLite transport. Networking, authenticated identities, encrypted membership, Android, and automatic durable task replay remain outside this release.
