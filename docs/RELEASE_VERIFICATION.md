# Release verification — 0.1.0-alpha.3

Verified on 2026-11-04 on Linux x86_64.

## Identity

- Static PIE Linux x86_64/musl executable, stripped: `term-social 0.1.0-alpha.3`.
- Executable SHA-256: `b67bb8021108602b7f67b8757267579d3038d7de1b0dd9a629f7b55c510c4c4c`.
- Runtime includes the Python standard-library hub, local/network test harnesses and [network setup guide](NETWORK.md).
- No live databases, sessions, registration invites, model weights, logs or toolchains are packaged. Earlier [alpha.1](RELEASE_VERIFICATION.alpha.1.md) and [alpha.2](RELEASE_VERIFICATION.alpha.2.md) reports are retained.

## Source gates

- Rust tests: **43 passed**, zero failures; two opt-in tests excluded from the normal run.
- Clippy `-D warnings`: passed.
- Locked optimized release build: passed.
- Existing terminal/launcher suite: **12 passed**, preserving local messaging, profiles/pictures, bots, approvals, cancellation, crash recovery, backups, setup and model ownership.
- New network suite: **4 passed**. It runs actual terminal clients with independent cache files against a disposable loopback hub. Tests cover invite registration, masked fields, saved mode-0600 bearer sessions, stable account identity, login rejection and token revocation; enforced human/bot authorship; private-room filtering; shared group/profile/bot sync; received mentions causing no model invocation; more than one page of history; fresh-device profile preservation; and queued delivery exactly once after both client and hub restart.
- Rust network checks cover transactional remote apply/rollback, cursor preservation, suppressed echo/outbox loops, account-bound event identity, other-device receipts, preserved private bot configuration, URL validation, read-only ownership controls and protection of pending/newer profile edits during catch-up. Narrow UI layouts include the connection dialog.
- Python syntax checks and actual Ratatui connection-dialog sample-buffer inspection passed.

## Scope of evidence

The independent clients were tested on one host over loopback. A second physical computer, LAN firewall/routing, reverse-proxy deployment and TLS certificate provisioning were not exercised. The first practical acceptance step is following NETWORK.md on two computers. Controlled HTTP model fixtures exercise network bot routing; the existing real-model test separately checks the normal launcher/approved shell path.

This is a first network alpha. Ordinary hub groups are public to registered accounts, personal agent room feeds are restricted by account, and the hub administrator can read stored history. HTTP is plaintext; use a trusted LAN, encrypted tunnel or correctly configured HTTPS. Production rate limiting, password recovery, private group roles, end-to-end encryption, remote presence, P2P and Android are future work. Local mode and its older databases remain separate from network caches.
