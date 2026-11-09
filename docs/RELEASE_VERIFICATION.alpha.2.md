# Release verification — 0.1.0-alpha.2

Verified on 2026-11-04 on Linux x86_64.

## Identity and scope

- Executable: `term-social 0.1.0-alpha.2`, static PIE x86_64 Linux/musl, stripped.
- Executable SHA-256: `d78a76fde5dcb869376231692266a064760018c940729cc24928bf2a68027b2d`.
- Local terminal messaging plus editable user/bot profiles, picture thumbnails, named bot mentions and per-account/chat bot enablement.
- No model weights, real profiles, databases, logs or toolchains are included in the package.
- The earlier alpha.1 verification is retained in [the historical report](RELEASE_VERIFICATION.alpha.1.md).

## Automated gates

- Rust suite: **37 passed**, zero failures; two opt-in tests excluded from the normal run.
- Clippy with `-D warnings`: passed.
- Optimized build with `--release --locked`: passed.
- Terminal/launcher acceptance suite: **12 passed**, including actual profile editing and picture-path paste, bot creation, model/personality routing, restart persistence, second-account profile display, and removing a bot from a chat. Existing messaging, presence, approvals, cancellation, recovery, backup and launcher ownership checks remain covered.
- Real-model terminal check: passed with the installed `Qwen3.6-35B-A3B-UD-Q4_K_XL` endpoint. The harness approved only `printf alpha-release-ok`, observed saved output and completion, inspected activity and exited normally. The existing server was reused.
- Profile-specific Rust checks cover stable identity after display-name edits, cancelled edits, duplicate handles, room/owner isolation, PNG/JPEG imports, file/dimension limits, thumbnails after source-file removal and backup/restore, bot-specific assistant roles, and named-bot identity after a simulated crashed session.
- Actual Ratatui sample-buffer previews were inspected for profile and bot-manager layouts. Narrow layouts remain covered down to 1 × 1 cells.

## Limits of this verification

This is a local alpha. Images are stored at 24 × 24 pixels and terminal truecolor support affects their appearance. Bots use the configured endpoint/workspace; model overrides must exist on that endpoint. Bot membership is a per-account preference, and names are not authenticated identities. Networking, Telegram integration, automatic bot-to-bot conversations and graphical image/file uploads are outside this release. See [known limitations](KNOWN_ISSUES.md).

The real model check exercises the original personal bot; named-bot model selection, instructions and role isolation use a controlled HTTP fixture for deterministic assertions. No claim is made that every model, terminal or Linux distribution was tested.
