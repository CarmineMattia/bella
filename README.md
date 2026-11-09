# Term Social — 0.1.0-alpha.3

A Linux terminal messenger for people and their personal AI agents. Use local mode on one computer or connect multiple computers through the optional authenticated hub. Profiles, pictures, shared groups, offline queues and owner-approved tools are available for alpha testing. [Two-computer setup guide](docs/NETWORK.md).

## Start

Extract the Linux x86_64 alpha package and run this from its folder:

```bash
./start
```

The first launch opens a setup screen for your local account ID, model endpoint, model name, and agent workspace. Tab changes fields, Ctrl+U clears a field, Enter saves and opens chat, and Escape exits. The launcher reuses a responding model server or starts an already-installed local GGUF through `llama-server`. It never downloads models or installs dependencies. If inference is unavailable, chat still opens with its connection status visible.

Requirements: Linux x86_64, Python 3.11+, a UTF-8 terminal (80×24 or larger recommended), and `sh`/GNU `timeout` for approved commands. The packaged app includes a static Rust executable and bundled SQLite; **Rust and a compiler are not needed to run it**. Agent inference additionally needs an installed `llama-server` and a compatible GGUF model, or an existing compatible HTTP endpoint.

```bash
./start --chat-only              # Skip model startup
./start --name alice             # Choose a local account ID for this session
./start --name bob               # A second terminal, same database
./start --setup                  # Change the saved profile
./start --doctor                 # JSON diagnostics; 0 only when chat and model checks pass
./start --stop-model             # Stop only a server started by this launcher
./start --help                   # Launcher and app options
```

Launcher-started models stay available after chat exits so another session can reuse them. Stop them explicitly when finished. Existing servers started elsewhere are reused but never stopped by the launcher. The default installed-model path is `~/models/Qwen3.6-35B-A3B-UD-Q4_K_XL/Qwen3.6-35B-A3B-UD-Q4_K_XL.gguf`; override it with `--model-path FILE` or `TERM_SOCIAL_MODEL_PATH`. See [model setup](docs/USER_GUIDE.md#model-startup).

## Try the alpha

1. Open two terminals with different names and exchange messages in `lobby`.
2. Create a group with Ctrl+N. In the second terminal, use `/join GROUP`.
3. Open `/agent`, click **Bot** above the composer, or mention `@bot` in a group. Review any proposed command before pressing `y`.
4. Try Stop, search, chat pinning, and draft recovery after a restart.
5. Open F5 to inspect agent activity, and F6 to see local sessions in the room.

[Guida rapida in italiano](docs/QUICKSTART.it.md) · [User guide](docs/USER_GUIDE.md) · [Alpha acceptance checklist](docs/ALPHA_TESTING.md) · [Known limitations](docs/KNOWN_ISSUES.md) · [Changelog](CHANGELOG.md) · [Release verification](docs/RELEASE_VERIFICATION.md)

## Main controls

- **Group / Bot** above the message input: choose who receives your message. F10 cycles recipients; `/to group` and `/to @handle` are keyboard alternatives. The selected bot receives plain follow-ups without tags. Explicit mentions select that bot for subsequent messages. Group replies remain shared.

- Enter or **Send**: send. Alt+Enter: newline. Paste supports Unicode and multiline text.
- Ctrl+K: find any chat, including archived chats. Ctrl+F: search the open chat's full saved history.
- Ctrl+N: create a group. Tab / Shift+Tab: next / previous chat in the visible folder.
- Options / F3 or right-click: pin/unpin and archive/restore. F4: Chats / Archive.
- Mouse wheel over the chat list scrolls conversations; over messages it scrolls history and loads earlier pages.
- Ctrl+End or **Latest messages**: return to the end without losing unread state while reading older messages.
- F2 / Ctrl+O: model diagnostics; R rechecks the connection. F5: durable agent activity. F6: participants and live local sessions.
- **Stop** / Ctrl+X: cancel inference and pending approvals. Already-approved commands finish under their timeout and keep their output; completed effects and background jobs are not undone.
- F9 / `/network`: hub connection, queued/rejected events and retry.
- F1: help. Ctrl+C or `/quit`: close; an active request is stopped first. Ctrl+C again forces exit while an approved command is finishing.

## Connect two computers

On the host: `python3 server/hub.py --data ./hub-data/hub.sqlite --bind 0.0.0.0`. On each client: `./start --network` (or `./start --chat-only --network` without local inference). Enter the host's LAN URL and sign in. Ctrl+R registers a new account using the invite in the host's `hub-data/hub.invite` file. Use a trusted LAN or HTTPS: HTTP sends credentials and messages in plain text.

F9 shows hub status and the persistent queue. Messages show **queued**, **on server**, or **rejected**; the server receipt does not mean the recipient has read them. Clients with the same account share network history, while existing local chats remain separate. All ordinary hub groups are public to registered accounts. [Full setup, permissions and recovery](docs/NETWORK.md).

## Profiles and bots

**F7** or `/profile` edits your display name, About text and picture. Your account ID stays fixed, so renaming preserves chats, drafts and task history. Paste a local PNG/JPEG path in Picture and save with Ctrl+S. Blank keeps the current image; `-` removes it. Pictures are cropped to square thumbnails and rendered in terminal colors.

**F8** or `/bots` opens the bot manager for this chat. Press **N** to create a bot, **E** to edit the selected bot, or **Enter** to add/remove it here. Choose a unique handle such as `planner`, a name, About text, instructions/personality, optional model override and picture. A new bot is added to the current chat automatically. Ask it with `@planner your request`. The original `@bot` remains available and its profile is editable too.

Each bot uses the configured endpoint and workspace. An empty model field inherits the default; an override must exist on that same server. Instructions and model edits affect future requests. These are local AI bots; Telegram accounts and external bot integrations are not connected. Adding a bot is a preference for your local account in that chat. Another account configures its own bots. In network mode, other accounts see shared bots with their owner and cannot edit them. **Bot lending (chat only, opt-in):** set *Lend for chat* to `yes` in a named bot's F8 form and other members of a group where that bot is added can ask it with `@handle`. Your client answers on your device with your model and instructions; guest requests never receive tools (no shell, no browser, no approval prompts), use only that group's history, are limited to 5 per guest per bot per hour, and appear in F5 with the requester's name. Lending is off by default and stays on the device where you enabled it. The hub is not end-to-end encrypted: its administrator can read group messages, including guest requests and bot replies. Public profile pictures and descriptions sync; model instructions stay on each device. Your own requests keep shell commands behind your approval. [Details](docs/NETWORK.md#bot-lending-chat-only).

## Data and backups

The extracted package uses `${XDG_DATA_HOME:-~/.local/share}/term-social/chat.sqlite` and a `settings.json` beside it. Source-tree launchers use `.data/chat.sqlite`. Override with `--data PATH` / `TERM_SOCIAL_DATA` and `--config PATH` / `TERM_SOCIAL_CONFIG`. The default new-profile agent workspace is a separate `workspace/` directory beside the database. Existing source data is not silently moved into the installed-data location.

```bash
./start --backup /absolute/path/new-backup.sqlite
```

Backups include committed WAL messages, drafts, read state, chat preferences, composer recipient choices, task records, profiles, imported pictures, bot settings/memberships, and local session records. Existing destinations are never overwritten. Copy `settings.json` separately if you also want to preserve the profile configuration. Restore into a **new database filename with all original app sessions closed**; see [backup and restore](docs/USER_GUIDE.md#backup-and-restore).

## Alpha boundaries

In local mode, display names are not authenticated accounts. Everyone with access to its database can inspect conversations, change records, or impersonate a name. Personal agent rooms separate context; they do not enforce privacy against local users. Commands run as your Linux user, without a filesystem sandbox. The workspace is only their initial working directory. Results appear in the invoking conversation.

This alpha does not implement P2P transport, end-to-end encryption, private group membership, Android, file attachments, or automatic durable task replay. Network groups are public within an authenticated hub; personal agent rooms are filtered by account. Network caches and the hub database need their own backups, described in the network guide. It records interrupted work and **never automatically replays a crashed task or an approved command**. Inspect command effects before explicitly retrying an interrupted task.

## Build and verification

The source tree on this computer has its development toolchain under `.dev/`:

```bash
./scripts/check test
./scripts/check clippy -- -D warnings
./scripts/check build --release --locked
python3 tests/alpha_smoke.py --binary target/x86_64-unknown-linux-musl/release/term-social --launcher ./start
./scripts/build-alpha
```

`./run` remains a development launcher that builds a debug executable. `./start` uses an existing executable and does not build. See [developer testing and release checks](docs/DEVELOPMENT.md), [architecture](docs/ARCHITECTURE.md), and [longer-term plan](PLAN.md).
