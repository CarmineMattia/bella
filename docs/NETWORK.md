# Two-computer messaging — alpha.3

This version adds an optional shared hub. Keep using `./start` for local mode, or run `./start --network` for network chats. They use separate databases: existing local chats are never uploaded automatically.

## 1. Start the hub on one computer

The extracted package includes `server/hub.py`, which needs only Python 3.11+ and the standard library. The hub can run independently of the chat app or model.

From the extracted package folder:

```bash
python3 server/hub.py --data ./hub-data/hub.sqlite --bind 0.0.0.0 --port 8787
```

The startup line shows the registration-invite file location. Read that file locally and give the invite to the people who should create accounts on this hub:

```bash
cat ./hub-data/hub.invite
```

Use the host computer's LAN address, for example `http://192.168.1.20:8787`. The address is an example: replace it with the actual host's address. `127.0.0.1` works only on the host itself. Allow the chosen port through the host firewall if needed. Binding without `--bind` defaults to loopback for same-computer tests. Stop the hub with Ctrl+C; restart it with the same data file to preserve accounts and conversations.

**HTTP sends passwords, session tokens and messages in plain text. Use it only on a trusted LAN or through an encrypted tunnel.** For other networks use HTTPS with a certificate trusted by the clients, either through a reverse proxy or the built-in TLS options:

```bash
python3 server/hub.py --data ./hub-data/hub.sqlite --bind 0.0.0.0 --port 8787 --tls-cert /absolute/path/fullchain.pem --tls-key /absolute/path/privkey.pem
```

The client verifies HTTPS certificates and has no insecure-certificate bypass. There is no end-to-end encryption: the hub operator can inspect stored conversations, including personal agent rooms.

## 2. Connect each computer

Extract the Linux x86_64 package on each client computer. Run:

```bash
./start --network
# Skip local model startup if you only want messaging:
./start --chat-only --network
```

If prompted for the existing local/model setup first, complete it normally. Hub sign-in then asks for:

1. Hub URL, such as `http://192.168.1.20:8787`.
2. A permanent account ID: 1–32 lowercase letters, digits or underscores, starting with a letter.
3. Password: 8–256 bytes.
4. Registration invite: needed only when creating an account.

Tab changes fields and Ctrl+U clears a field. **Ctrl+R creates an account**. **Enter signs in** to an existing account. Escape cancels. Password and invite fields are masked and neither is saved. Accounts have stable random IDs; their account handles cannot be renamed. Use F7 to change the visible name, About text and picture.

A saved session lasts up to 30 days. The next `./start --network` reuses it and can open cached chats while the hub is offline. Use the same account to access its network chats on another device; each device signs in separately.

```bash
./start --network --network-setup  # Sign in again, or choose another hub/account
./start --network-logout          # Revoke the saved session, retaining cached chats
./start                           # Return to separate local chats
```

Sign-out needs the hub online to revoke its token. With an explicit `--data PATH`, pass the same base path to these commands. A session file named after that base path (for example `chat.network.json`) is created with owner-only permissions. It contains a bearer token: treat it as a login secret. It is not part of the SQLite backup or packaged application. Password reset, account deletion and administrator tools are not implemented yet.

## 3. Try messaging and groups

Both users start in `lobby`. Send a message from one device and confirm it appears on the other. Ctrl+N or `/join GROUP` creates/opens a shared group. Empty groups are synchronized too.

All non-agent groups on a hub are **public to its registered accounts**; this release has no private groups, group invitations or moderator roles. Read state, drafts, pins, archives and composer recipient choices remain local preferences. F6 shows participants found in local session/message history; it does not claim remote online presence.

`/agent` is your account's personal bot conversation. The hub filters those room messages to the same signed-in account. It is available on your other devices, but not to other accounts. The hub administrator still has access to its database.

## 4. Profiles and bots

User/bot names, descriptions and picture thumbnails sync. F8 shows your own bots plus other accounts' bots added to the current group. Another account's bot is read-only, labeled with its owner. Only the owner can edit its profile or change its membership. Reusing an account handle on a different hub does not reuse its local cache or identity.

Bots run on their owner's device. The owner's own `@handle` requests can propose shell/browser commands, each approved locally. A received mention never starts a shell command, and it starts a model request only through the opt-in lending below. Group participants see the request, bot reply and shared tool notes.

### Bot lending (chat only)

This is the first, partial step of owner/delegate policies. It covers chat answers only.

- **Opt-in per bot.** In F8 → **E**, set the last field, *Lend for chat to group members*, to `yes`. The default is `no`. The setting is stored only on that device and is never synced. The hub drops it, and sync cannot turn it on. If you enable it on two of your devices, both answer.
- **Who can ask.** Another account posts a message in a group (not `/agent`) that starts with `@handle`, while your bot is added to that group. A guest's client never runs your bot. It only shows "Sent to alice's @planner". Your running client sees the message arrive from the hub and starts the bot on your device, with your model and instructions. The reply is posted to the group under the bot's identity. Your client must be open and online with `--network`.
- **No tools for guests.** Delegated requests send the model no tool definitions and a separate system prompt that does not mention your computer. Any tool call the model returns anyway is refused without an approval prompt or execution. Shell and `open_browser` stay available only for your own requests.
- **Context.** Only the history of that group up to the request (the last 24 turns) is used. Your `/agent` room and other groups are never included. Instructions are sent to the model only as the system prompt, but a determined guest may still coax a model into paraphrasing them, so do not put secrets in instructions of lent bots.
- **Limits.** Each guest gets 5 started requests per bot per rolling hour (`DELEGATE_PER_HOUR`, a constant in `src/profiles.rs`). The bot answers one request at a time, and a mention arriving while it is busy is skipped, not queued. Requests over the limit or skipped as busy never reach the model. They post nothing to the group and show only in the owner's status line. Messages from bots, your own messages, messages older than about one minute before startup (offline backlog), and mentions where the requester has their own bot with the same handle in that group are ignored. Each message is decided once (dedup by message id, persisted in `delegate_requests`).
- **Audit.** F5 lists delegated tasks as "@bot for NAME (delegated)", and task details show "Requested by". A failed delegated request posts only a generic note to the group, and the error detail stays in your local task.
- **Not yet.** Guests cannot use tools under any approval scheme. There is no sandbox, per-guest allowlist, synced or multi-device quota, or end-to-end encryption. The hub administrator can read every group message, including guest requests and bot replies.

Bot instructions, model overrides, endpoints and workspaces stay on each device. Set them on every device where you want to invoke that bot. Sync does not overwrite an existing device's instructions or model. A shared bot's picture and public description do not grant permission to execute commands. Removing a named bot from a group before its reply uploads can reject that reply; the local record remains available.

## 5. Disconnection and delivery

The UI displays hub status in the chat header; **F9** or `/network` shows connection details and rejected events.

- **Queued**: committed to this device's outgoing queue, waiting for the hub.
- **On server**: the hub acknowledged storage. It is not a recipient read receipt.
- **Rejected**: the hub refused the event, for example because bot membership changed or a message exceeds 64 KiB. F9 shows the reason. Fix the cause, then press R there to retry rejected events, or resend a corrected message.

Disconnecting or closing the app preserves the queue. Reopening `--network` retries temporary failures, catches up missed events and prevents duplicate echoes. Profiles, bot membership, messages and task completion enter the queue in the same SQLite transaction as their local changes. Multiple local windows for the same account serialize network sync with a file lock.

## Storage and backups

The base `--data` path selects the directory and saved session. Network cache filenames include both hub ID and account ID: `network-HUBID-USERID.sqlite`. Local mode still uses the original base database. Network chat messages, receipts, cursor and outgoing queue live inside the account cache. The hub has its own separate SQLite database and registration-invite file.

To back up an account cache, close its clients and use the existing backup command with that cache's actual path (without `--network`):

```bash
./start --data /absolute/path/network-HUBID-USERID.sqlite --backup /absolute/path/new-cache-backup.sqlite
```

Restore with clients closed to the original cache filename for that same hub/account. Do not restore one account's cache under another identity. The hub database is authoritative for shared history. Stop the hub before copying its database and any remaining SQLite WAL/SHM files, or use a SQLite-aware backup. Keep its invite file private. Do not delete the hub database to reset a password: that creates a different hub identity and requires clients to sign in again.

## Verification and current boundaries

```bash
python3 tests/network_smoke.py --binary ./bin/term-social
```

The suite runs two real terminal clients with separate cache files against a loopback HTTP hub. It covers account sign-in/registration, masking and session persistence, owner-enforced bot identity, private-room filtering, public profile/bot sharing, pagination, same-account bootstrap, sign-out, and queue recovery after both hub and client restart. A controlled model verifies that received mentions do not invoke another user's bot without the owner's lending opt-in, and that a lent bot answers a guest with no tool definitions in the model request. These tests do not substitute for trying two physical computers on your LAN.

This is a first network alpha, not an internet-hardened service. It has bounded request sizes/concurrency and hashed passwords/session tokens, but lacks production rate limiting, password recovery, administrative moderation, end-to-end encryption and remote presence. There are no P2P transport, Android or ARM client packages yet. Only completed/interrupted bot output is shared; in-progress streamed tokens remain on the invoking device. The timeline currently follows local arrival order, so catch-up can place older received messages after optimistic local sends.
