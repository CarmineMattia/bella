# Product direction

The project is called Bella. A decentralized terminal messenger where humans and personal AI agents are first-class participants. One command opens the app. Each owner runs their own agent, preferably with free local inference, and controls what it can do on their devices.

The terminal is the primary interface for chats, groups, tools, approvals, model setup, and settings. Rust/Ratatui is the starting UI stack. Linux first, Android later with a native device-control bridge. This is a new project; Rakijazios is abandoned as the foundation because its centralized architecture does not fit. No old repositories are deleted.

## First milestone: local vertical slice

- Two real terminal sessions can converse in a group on this computer.
- One participant can invoke an actual local model.
- The model can propose a useful shell task.
- The local owner approves or denies the exact command inside the terminal.
- Permitted results appear in the originating group and history survives restart.

The initial SQLite transport validates the experience. It does not establish decentralized networking or trusted identities.

## Local usability progress — 2026-11-04

Implemented a messenger-style layout with incoming/outgoing bubbles, chat previews, unread badges, UTC timestamps, mouse navigation and sending, group creation and search dialogs, Unicode cursor editing and multiline paste. Empty groups, per-user read state, and drafts persist across restarts. Replies stream into their originating chat; failed requests can be retried. The model connection check runs off the UI thread. `scripts/start-model` starts the existing GGUF explicitly on localhost.

Validation includes two app instances exchanging messages, mouse group creation and sending, search and unread transitions, draft recovery after restarting, and a real local-model streamed reply with an approved shell tool. Networking still uses the local SQLite transport. Full-history message search now opens highlighted results with surrounding context; older and newer messages load in pages. Scroll anchors preserve the reading position as messages arrive, and returning to the latest messages updates read state.

Agent requests can now be stopped through the Stop button, Ctrl+X, or `/stop`. Stopping aborts active HTTP requests and pending approvals; an already-approved shell command finishes under its existing timeout and retains its output. Partial replies are labelled interrupted and excluded from subsequent model context. Model details and an explicit connection recheck are available through F2, Ctrl+O, `/model`, or the sidebar connection indicator. The connection check verifies the requested model is actually listed by the server.

Conversation organization now includes per-user pin and archive preferences, a right-click/F3 options menu, Chats/Archive tabs and F4 navigation, independent sidebar scrolling, and global search that includes archived chats. Preferences persist without altering messages, drafts, unread state, other users' lists, or active agent tasks. Incoming archived messages remain in the archive and surface an unread indicator.

## First alpha scope

`0.1.0-alpha.3` is the packaged Linux x86_64 alpha with local mode and an optional authenticated hub: multiple computers and terminal sessions, real local inference, human-owned tool approvals, chat organization, first-run settings, installed-model lifecycle, task history/recovery, local presence, durable network message sync, profiles, bots, backup, and diagnostics. It is a testable release of the first vertical slice, not completion of the later decentralized network.

Onboarding, participant discovery, launcher-owned model lifecycle and durable task records are implemented for this alpha. Task recovery records interruptions and never automatically replays work. Runtime/source packages, checksums, documentation and an isolated terminal acceptance suite are the release deliverables. The release verification document records observed results; it does not turn future milestones into delivered features.

## Next milestones

1. Gather alpha feedback and harden usability: grapheme editing, large-history memory bounds, authenticated model endpoints, hardware-aware model setup and additional terminal platforms. Durable task scheduling/replay remains a separate design decision requiring explicit authorization semantics.
2. Evaluate Pi against the initial agent adapter using the same model and tasks. Retain application-owned tool permissions and prevent bypass through unrestricted built-in tools or extensions.
3. Introduce cryptographic identity and explicit group membership. Replace shared-database transport with encrypted peer connections, evaluating iroh. Add invitations, message IDs, deduplication, synchronization, and reconnect handling. Test on distinct networks; document relays and offline delivery assumptions.
4. Add owner/delegate policies for cross-participant requests. Separate reading from disclosure, bind approvals to exact requests, and prevent agents from approving their own actions. Bound bot-to-bot exchanges. **Partial (2026-11-09):** opt-in, chat-only bot lending. The owner's device answers guest mentions with tools disabled, group-only context, a per-guest hourly limit, a recorded requester, and bot authors ignored. Still open: any tool use for guests (approval bound to the exact guest request, sandboxing), separating reading from disclosure, synced or multi-device quotas, and encryption.
5. Package a one-command Linux experience with hardware-aware model recommendations. Free means no required paid API; hardware and inference costs still exist. Test task completion, latency, and recovery on realistic machines.
6. Port the terminal experience to Android. Validate Termux/runtime compatibility and add a Kotlin control bridge; test accessibility, optional Shizuku, background operation, and recovery on the Pixel.
7. **Integrazione bot esterni (futuro) — not started, not implemented.** After owner/delegate, harness skills and the Android companion, and before or alongside mesh networking: a stable API for external bots with per-bot tokens, push/webhook delivery instead of polling, and mandatory TLS or encryption. First use case: a control room where Carmine receives alerts and summaries from his own bots (for example “3 bozze da rivedere”), with no citizens’ personal data.

## Deliberately open choices

Final harness; model and routing strategy; networking/synchronization protocol; offline storage policy; Android runtime. iroh connectivity does not supply messaging semantics automatically. Relay infrastructure can be community-operated but cannot be assumed unnecessary.

## Network alpha.3 milestone — 2026-11-04

The first centralized hub path is implemented: authenticated stable accounts, separate client caches, public shared groups, profile/bot roster sharing, persistent outgoing delivery and reconnect/catch-up. Owner-invoked bots keep local command approval. Next network stages remain private groups/moderation, hardened account/rate-limit administration, remote presence, encryption design and eventual P2P/Android transport. Two-client loopback tests cover the implementation; a real two-machine LAN test remains a user acceptance step.
