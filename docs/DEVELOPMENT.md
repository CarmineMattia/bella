# Development and release checks

## Local source tree

The source tree on this computer has project-local Rust and Zig tooling under `.dev/`. These directories are ignored and are not included in runtime/source archives. Source dependencies are locked in Cargo.lock.

```bash
./scripts/check test
./scripts/check clippy -- -D warnings
./scripts/check build --release --locked
python3 tests/alpha_smoke.py --binary target/x86_64-unknown-linux-musl/release/bella --launcher ./start
```

On a new development machine, Rust stable and a target C/linker toolchain are required; ordinary `cargo test` / `cargo build --release --locked` can build for its host target. The project wrappers target Linux x86_64 musl using the project-local toolchain. `scripts/bootstrap` is the existing network installer for that development setup, not a runtime dependency and not run by the alpha launcher. The alpha release is validated on the current Linux host; fresh toolchain installation is not part of its runtime acceptance claim.

## Real inference test

With the selected installed model served on localhost port 8080:

```bash
./scripts/check test live_model_stream_and_approved_shell -- --ignored
```

It asks the actual model for a shell invocation of `printf bella-tool-ok`, checks the exact requested command, approves it, and verifies streamed text and persisted output. The normal test suite also checks denied tools, stalled HTTP cancellation before headers and during streaming, pending-approval cancellation, approved-command output on Stop, output-pipe bounds, migrations, consistent backup, profile configuration, task recovery, atomic completion, presence, draft/read/pin/archive behavior and narrow layouts.

The extracted package also provides an opt-in real-model terminal test, without Rust:

```bash
python3 tests/live_alpha.py --launcher ./start --endpoint http://127.0.0.1:8080/v1 --model Qwen3.6-35B-A3B-UD-Q4_K_XL
```

It uses a disposable profile/database, launches the normal alpha entry point, and approves only a command equivalent to `printf alpha-release-ok`. Any different proposed command is denied and the test fails. It checks durable approval/completion, chat output and the activity view.

## Terminal acceptance suite

```bash
python3 tests/alpha_smoke.py --binary /absolute/path/to/bin/bella --launcher /absolute/path/to/start
```

This Linux standard-library Python harness creates PTYs, temporary databases and controlled HTTP model servers. It covers first-run setup, saved profiles, two-session messaging, Unicode/multiline paste, drafts after restart, history search, unread protection, pin/archive/restore, sidebar context targets, local presence, Stop, activity views, process-crash recovery, graceful command completion, backup, diagnostics and launcher process ownership. Fixture shell commands write only inside a temporary test directory. Model fixture weights are a placeholder for lifecycle tests, not actual inference.

The suite is intended to run against both the source release binary and the extracted package, including from paths containing spaces. Each test uses isolated data and closes its app/model processes. It never uses the production database.

## Build an alpha package

```bash
./scripts/build-alpha
# Optional explicit output directory:
./scripts/build-alpha /absolute/path/to/releases
```

The script builds the optimized static binary, then creates a runtime directory, runtime tar.gz, source tar.gz and SHA-256 checksums. It does not publish or upload them. Runtime contents include the executable, standard-library launcher, model helper, documentation, manifest/checksums and the terminal acceptance harness. Model files, live settings, databases, logs, `.dev/`, `target/` and Git internals are excluded.

Before distribution:

1. Confirm the unit tests, clippy and real-model test pass.
2. Build the archive and check its checksums.
3. Extract it to a different path containing spaces; run version/help and the acceptance suite against that extracted copy.
4. Verify `file` / ELF headers show the intended Linux x86_64 static executable.
5. Compare the documented alpha scope and known limitations to actual behavior. Record the results in release verification notes.

## UI snapshots

An ignored Rust test exports an actual Ratatui cell buffer with sample messages from an in-memory database:

```bash
BELLA_PREVIEW=/absolute/path/preview.json ./scripts/check test ui_preview -- --ignored
BELLA_PREVIEW=/absolute/path/options.json BELLA_PREVIEW_MODE=options ./scripts/check test ui_preview -- --ignored
```

The `search` mode exports message search. `profile`, `bots`, and `portraits` exercise the new editors and avatar layouts. `BELLA_AVATAR=/local/file.png` optionally supplies a sample image for those previews. Sample content is never written to real chats. Terminal screenshots used for release review should be labelled as sample data.

Alpha.2 also covers profile IDs versus display names, image decode limits and backup survival, duplicate bot handles, per-account/chat enablement, named-model/personality routing and bot identity after crashes. The terminal suite creates a temporary PNG with Python standard-library tools and exercises the editors through actual key and paste events.

## Hub tests

Run `python3 tests/network_smoke.py --binary /absolute/path/to/bella`. It creates a disposable in-process HTTP hub, authenticates accounts and launches real terminals backed by independent network caches. It also tests protocol rejection/ownership/private-room rules, offline delivery after hub/client restart, multi-page bootstrap and token revocation. It never starts an externally accessible hub or uses production profiles. See [NETWORK.md](NETWORK.md) for manual two-machine testing.
