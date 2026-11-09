# Release verification — 0.1.0-alpha.1

Verified on 2026-11-04 on Linux x86_64, using Python 3.14.7 for terminal/launcher tests.

## Release identity

- Version observed from the executable: `term-social 0.1.0-alpha.1`.
- Target: `x86_64-unknown-linux-musl`.
- `file` inspection: ELF 64-bit x86-64, static PIE, stripped.
- Verified executable SHA-256: `b358e61bd691d8225cd7efbe56f9db42b8f1cda2874eb0666399718a86f3c558`.
- The final runtime manifest and file checksums bind the delivered executable and support files. No model weights, live settings, databases, logs or toolchain directories are included.

## Source gates

- `./scripts/check test`: **32 passed, 0 failed; 2 opt-in tests excluded from the normal suite**.
- `./scripts/check clippy -- -D warnings`: passed without warnings.
- `./scripts/check build --release --locked`: passed; optimized static executable produced.
- The separate real-model Rust test passed with `Qwen3.6-35B-A3B-UD-Q4_K_XL`, an owner-approved `printf term-social-tool-ok`, streamed output and saved conversation results.
- Existing message-only databases, WAL backups, per-user preferences, long/Unicode profile labels, task completion rollback, stale-session recovery and live-session exclusion have direct regression coverage.

## Extracted-package gates

The runtime archive was extracted outside the source tree into a path containing spaces. Its **11 terminal/launcher acceptance tests passed** using the packaged binary and launcher:

1. First-run setup saves and reuses a profile; cancelling reconfiguration preserves the existing settings.
2. Two terminal instances exchange group messages, paste Unicode/multiline text, preserve drafts across switching/restarting and update local participants.
3. Full-history search reaches beyond the newest 200 events, opens the matching context and protects unread state while reading older messages.
4. Pin/archive/restore preferences survive restart, do not change other names' preferences, and target the correct chat after sidebar scrolling.
5. Stop closes an in-flight model connection, saves received text as interrupted, and allows a subsequent request. Model diagnostics and manual recheck work.
6. SIGKILL recovery records streaming and pending-approval interruptions once, never sends an automatic replacement request and never executes the pending command.
7. Graceful quit waits for an already-approved short command and preserves its output and approval record.
8. A SQLite backup restores into a new filename, refuses overwrite and remains coherent. Offline diagnostics distinguish chat readiness from model readiness.
9. Launcher-owned model process startup/reuse/stop works, including executable/model paths with spaces; a discovered external model server remains available. The normal launcher enters chat using the running model, and exiting chat retains that model for another session.
10. Version/help and clean noninteractive error paths work with Cargo/Rustup paths deliberately unavailable.
11. A portable runtime without a source tree saves data/settings outside the runtime folder. An explicit XDG data directory is honored, and an empty XDG_DATA_HOME falls back to the isolated home directory's `.local/share/term-social`.

The controlled HTTP servers in this suite prove application and lifecycle behavior; they are not model-quality tests.

## Real inference and process lifecycle

- The **extracted release's normal launcher** reused the real installed Qwen model, entered the terminal UI, streamed a reply, requested and durably recorded approval for `printf alpha-release-ok`, stored the tool output and completion, exposed activity details, and exited cleanly. The opt-in packaged `tests/live_alpha.py` test passed.
- An additional real `llama-server` process was started by the launcher with an installed Gemma GGUF on loopback port 18181. Diagnostic JSON reported database integrity `ok`, `chat_ready: true` and `model_ready: true`.
- `--stop-model` terminated that owned model process, removed its registry and closed its port. The pre-existing Qwen server on port 8080 remained healthy. The extra test model was cleaned up.

## Scope audit

- **Testable first alpha:** executable runtime directory, runtime archive, source archive, manifest and checksums are supplied. Running the alpha does not invoke a compiler.
- **Usable local app:** onboarding, chats, local groups, editing, drafts, search/history, organization, inference, owner approvals, cancellation, activity/recovery, presence, diagnostics and backups have current code and runtime evidence.
- **Documentation:** README, Italian quick start, user guide, alpha checklist, known issues, changelog, architecture, development instructions and the longer-term plan reflect this release. Relative document links are checked during final artifact verification.
- **No claim of a completed network product:** cross-computer/P2P transport, authenticated identities, private membership, Android, attachments, automatic durable replay, hardware selection and API-key authentication are explicitly outside this local alpha. The remaining milestones stay in PLAN.md.
- **Limits remain visible:** a force-exited approved command or a background job may continue; small unflushed streaming fragments can be lost on a crash; Unicode editing is character based and search case folding is ASCII. See KNOWN_ISSUES.md.

This report records observed release gates. It does not replace the tester's exploratory feedback or imply that every possible model, terminal, Linux distribution or command has been validated.
