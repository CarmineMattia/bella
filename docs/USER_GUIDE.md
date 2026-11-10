# Bella alpha user guide

## Profiles and startup

Run `./start` from the extracted package. On first launch, set a local account ID, an OpenAI-compatible model endpoint ending in `/v1`, the model's served name, and an existing workspace directory. Enter saves the profile; Escape cancels. The configuration file is saved atomically with owner-only file permissions; changing it keeps a `settings.json.previous` copy.

`./start --setup` reopens setup. CLI `--name`, `--model`, `--endpoint`, and `--workspace` temporarily override saved values. They are saved only when you explicitly use setup. Distinct account IDs have distinct drafts, read state, pins, archives, and agent activity. Two processes using the same account ID share that profile's state; IDs are not passwords or verified identities. Changing the ID in setup selects another local profile; use F7 to change only its display name.

The source-tree launcher defaults to `.data/chat.sqlite`. The extracted package defaults to `$XDG_DATA_HOME/bella/chat.sqlite`, or `~/.local/share/bella/chat.sqlite` when XDG_DATA_HOME is unset. If the `bella` directory does not exist and a leftover `term-social` directory is present in the same parent, the launcher moves it once (it copies if a rename is not possible, and never deletes the original data if that fails). If `settings.json` stored a workspace inside that old directory, the path is updated; a workspace kept elsewhere is left unchanged. `--data PATH` or `BELLA_DATA` overrides the database path. `--config PATH` or `BELLA_CONFIG` selects another settings file. The older `TERM_SOCIAL_DATA` and `TERM_SOCIAL_CONFIG` names still work when the matching `BELLA_*` variable is unset; this compatibility is temporary. Keep settings and database paths different. To continue an existing source-tree conversation from the package, explicitly pass that source database's absolute path. `./term-social` and the packaged `bin/term-social` are temporary aliases for Bella and will be removed in a later release.

## Model startup

The launcher checks `/models` before starting anything. It reuses an endpoint that lists the requested model. For automatic startup, the endpoint must be `http://127.0.0.1:PORT/v1` or its localhost equivalent, an installed GGUF must exist, and `llama-server` must be on PATH or supplied with `--server FILE` / `LLAMA_SERVER`.

```bash
./start --model-path /absolute/path/model.gguf --model SERVED_NAME
./start --model-only --model-path /absolute/path/model.gguf --model SERVED_NAME
./start --chat-only
./start --stop-model
```

Custom `--model-path` and `--server` startup overrides are not saved in the chat profile. Pass them again on future cold starts, or export `BELLA_MODEL_PATH` (or the temporary `TERM_SOCIAL_MODEL_PATH`) and `LLAMA_SERVER`. The model alias and endpoint can be saved through setup.

Startup defaults: context 8192, GPU layers 99, flash attention on, a 90-second startup deadline. Override with `BELLA_MODEL_CONTEXT`, `BELLA_MODEL_GPU_LAYERS` (or the temporary `TERM_SOCIAL_MODEL_*` names), and `--startup-timeout SECONDS`. Different hardware may need lower GPU layers or a smaller GGUF. The alpha does not download weights or choose a model based on hardware.

One launcher-owned model is managed per configuration directory. A model started by the launcher has its own process group, a process-identity record, and a log beside the settings file: `model-process.json`, `model.lock`, and `model.log`. It stays running when a chat exits. `--stop-model` uses the saved PID **and Linux process-start identity**, executable, and process group; it does not stop a server it merely discovered. Model startup failures are reported before chat opens, and `--model-only` returns nonzero on failure.

F2 or `/model` shows the endpoint, selected model, workspace, and diagnostic result. R retries the connection check. A responding server with a missing requested model is distinct from a connection error. HTTP endpoints requiring API-key authentication are not supported in this alpha.

## Conversations and organization

In local mode, terminal instances using the same database can exchange messages. For separate computers, use [hub mode](NETWORK.md). Ctrl+N creates a named group; `/join NAME` opens it from another terminal. Empty groups persist. `/agent` opens your personal agent context.

Click a chat to open it. Ctrl+K searches names and latest-message previews across Chats and Archive; Enter or a click opens the result. Tab and Shift+Tab cycle chats in the visible folder. Scroll the sidebar with the mouse wheel when there are more rooms than fit on screen.

Options / F3, or a right-click on a chat, opens actions for that exact chat. P toggles pinning; A archives or restores it. Arrow keys and Enter, or mouse clicks, also work. The open chat's archive action switches its sidebar to Archive; archiving a different chat does not open it or consume its unread messages. Pins and archive status are per display name, and survive restarts. Incoming messages remain in their archived folder; an unread dot appears on the Archive tab.

F4 or the Chats / Archive tabs switches folders. `/pin`, `/unpin`, `/archive`, `/restore`, `/chats`, and `/archived` are equivalent shortcuts. Archiving never deletes messages, drafts, or tasks.

## Writing, history, and search

Enter or Send posts the current message. Left/Right, Home/End, Backspace/Delete edit it; Ctrl+U clears it. Alt+Enter inserts a newline. Pasting multiline text does not send until Enter. Each chat's unsent draft is saved while you type and restored when switching or reopening the app.

Page Up/Page Down or the mouse wheel scroll message history. Reaching the top loads another 200 older messages. Ctrl+F or Find searches all saved messages in the current chat, including messages outside the loaded page. Choose a result to open its surrounding context and highlight it. Page Down in search fetches another 50 matches. Matching treats `%` and `_` literally. Search case-folding is ASCII; non-ASCII text matches its exact case.

New messages do not move your scrolled reading position. They remain unread until you return to the end. Ctrl+End or the clickable Latest messages strip jumps to the live tail. A historical search window loads newer pages as you scroll downward. Escape clears a message highlight.

All displayed message and task timestamps are UTC.

## Agent requests and approvals

Mention `@bot TASK` in a group, or write a message in `/agent`. One request per app instance runs at a time. Streaming replies stay bound to their originating conversation even if you switch rooms or archive it.

The agent can propose a shell command. The approval panel shows the exact command, working directory, and room receiving output. Y approves once; N or Escape denies. PgUp/PgDn reviews a long command. The decision is recorded before execution. Every command needs a fresh decision; pending approvals are never restored as approvals after a restart.

Commands run under `sh` as your ordinary Linux user. They have a 20-second timeout and a two-second forced-termination grace period. Captured output is capped at 16 KiB for each of stdout and stderr. Background children can outlive the shell; open output pipes do not hold the request open indefinitely. Such output is labelled to make this visible. The agent stops after six model turns; send a new request to continue.

Stop / Ctrl+X / `/stop` aborts the HTTP request and drops unanswered approvals. An already-approved command is allowed to finish under its existing timeout, and its result is kept. The task is not reported stopped until that execution worker has finished. Stopping does not undo completed effects or stop deliberately launched background jobs. Ctrl+R or `/retry` retries an ordinary failed request in its original chat.

Ctrl+C or `/quit` stops active work before closing the interface. If a command is still finishing, Ctrl+C again forces exit and leaves an interruption record. A forced exit may leave an already-approved command running under its timeout.

## Activity and recovery

F5 or `/tasks` opens your latest 50 task records. Enter or a click shows a task's state, request, last proposed command and approval, received text, and error. Enter in the detail view opens its conversation. PgUp/PgDn and the mouse wheel scroll details.

Task records survive process crashes. Startup checks the saved process-start identity, so another live app instance's tasks are not incorrectly interrupted. Work from a dead session is marked interrupted, received text is recovered once, and a note is posted to the original chat. A command already approved before the crash may have run or may still be running. Check its effects before issuing a fresh request. **No crashed request is automatically replayed.**

Live streaming text is checkpointed by the UI about every refresh; the last small unflushed portion can be lost in a sudden crash. A graceful stop stores all text the UI has received. These records support diagnosis and manual recovery, not an automatic durable queue.

## Participants

F6 or `/people` shows humans with message history in the open room and live local sessions currently viewing it. Presence uses the Linux PID and process-start identity, not a guessed timer. Switching rooms changes presence; closing or killing a process removes it from the live count. Display names remain unverified, and the list does not establish private membership or remote online status.

## Backup and restore

Run this while the app is open or closed:

```bash
./start --backup /absolute/path/new-snapshot.sqlite
```

The SQLite snapshot includes committed messages in the WAL, drafts, read state, preferences, activity, and session records. It refuses to overwrite an existing destination. Profile configuration and model weights are separate; copy `settings.json` separately when needed.

For a restore:

1. Close all app instances using the original database. Let approved commands finish, or inspect any work left by a forced exit.
2. Keep the backup unchanged. Copy it to a **new** filename, for example `/absolute/path/restored-chat.sqlite`.
3. Open that copy with `./start --chat-only --data /absolute/path/restored-chat.sqlite`. Pass `--config /absolute/path/settings.json` to reuse an existing profile if desired.
4. Verify message history, drafts, and activity. A task active in the snapshot is not replayed.

Do not replace a live database file or copy only its main file while WAL writers are active. Opening a fresh restored filename avoids stale `-wal` and `-shm` files.

## Diagnostics and feedback

`./start --doctor` prints JSON containing the version, data/workspace paths, database integrity, shell-helper availability, endpoint, and model status. It returns zero only if the chat dependencies, workspace, database, and selected model checks pass. A nonzero status with `chat_ready: true` and `model_ready: false` means chat can run but inference needs attention. `--chat-only` does not make an offline model check pass.

For an alpha bug report, include `./start --version`, terminal dimensions, reproducible steps, expected/actual behavior, and the relevant diagnostic error. Logs and databases may contain your messages or command output; inspect them before sharing. The app does not send reports automatically.

## Profiles, pictures and named bots

Open **F7** or `/profile` to edit your display name, About text and picture. The startup `--name` is a stable local account ID; the in-app display name is separate. Renaming your display name preserves your agent room, message authorship, drafts, unread state and tasks. Other local sessions see your updated profile, including on earlier messages.

Picture accepts a local **PNG or JPEG** path, pasted as plain text without shell quotes. The limit is 8 MiB and 4096 × 4096 pixels. Pictures are center-cropped to a 24 × 24 thumbnail, stored inside SQLite, and rendered with colored Unicode half-blocks. The original is not copied or retained. You can remove the original file afterward; a database backup includes the thumbnail. A truecolor UTF-8 terminal gives the best result. Blank keeps the current picture and `-` removes it. Initials appear when no picture is set.

Use Tab/Shift+Tab or Up/Down to choose a field, Ctrl+U to clear it, **Ctrl+S** to save, and Esc to cancel. Enter moves to the next field; Enter on the last field saves. You can also click a field or the Save button. Invalid input leaves the editor open and preserves the previous profile.

Open **F8** or `/bots` from the chat where you want a bot:

1. Press **N**. Enter a handle such as `planner` (1–24 lowercase letters, digits or underscores; start with a letter).
2. Set its display name, About text, instructions/personality, optional model and picture. Names allow 32 characters, About allows 160, and instructions allow 2000; fields are single-line.
3. Optionally set *Lend for chat to group members* to `yes` (default `no`), *Guest requests per person per hour* (1–60, default 5), and *Guest tools when lent* to `off` or `shell` (default `off`). See [bot lending](NETWORK.md#bot-lending); these only have an effect in network mode.
4. Save with Ctrl+S. The bot is added to this chat immediately.
5. Close the manager and send `@planner help plan this task`.

In the manager, Up/Down selects, **E** edits, and **Enter** or Space adds/removes the selected bot from this chat. Mouse buttons at the bottom offer the same actions. Removal preserves the bot's profile and old messages. Handles are fixed after creation; display names, pictures and instructions remain editable. The original **@bot** can be renamed and given a picture, and is always available. Unmentioned messages in `/agent` still go to this original bot.

Bots use the endpoint and workspace in your startup settings. Blank Model inherits the default; a custom model must be served by the same endpoint. Changing settings does not alter an active request. Each local account owns its bot definitions and chat-enablement preferences. Other accounts can see replies and pictures. They can ask your bot only if you lend it: the answer runs on your device, is limited per guest per hour by the bot's own setting (default 5), and is recorded in F5 with the requester's name. Guest tools stay off unless you set *Guest tools when lent* to `shell`; even then every command needs your approval, runs in a per-guest folder, and is not a sandbox (the process still runs as your Linux user). There is one active request per app instance. Other bots' replies are labeled as conversation input rather than being passed as this bot's own assistant history. Shell tools still require local approval, and crashes never replay them automatically.

These are local AI bot profiles, not Telegram Bot API accounts. There is no bot marketplace, networking, upload service, animated image support or automatic bot-to-bot conversation.

## Network mode

See [the two-computer setup and recovery guide](NETWORK.md). `./start --network` opens an isolated, authenticated account cache; `./start` keeps using local chats. F9 displays connection/queue status. Public groups, profile pictures and named bot membership sync through the hub. Bots run on their owner's device: when the owner invokes them, or, for bots the owner has explicitly lent, when another member mentions them in a shared group. Lent bots answer in chat; guest shell is off unless you enable it, and `open_browser` is never available to guests. The hub is not end-to-end encrypted.

## Opening your browser (development update)

From `/agent`, ask “open my browser” or “open https://example.com”. The bot has an explicit `open_browser` tool and detects whether a graphical desktop session is available. It proposes a command for your registered default browser; review it and press Y to approve or N to deny. In groups, select Bot in the composer or address the bot with `@bot`/its handle. All launches use the same recorded approval path as shell commands. URLs support HTTP/HTTPS; omitting the URL (or using `about:blank`) opens the browser’s configured startup page; no web-page clicking or screen-reading tool is included. A successful launcher exit means the launch was requested, not that the agent visually inspected a window. Browser cards show **Launch requested**, **Failed**, or **Denied**. Click **Details** on a browser card, or press **F11** (or send `/browser` if the terminal captures F11) for the command and diagnostics, **Left/Right** for other browser actions in the loaded chat history, and **PgUp/PgDn** to scroll. The approval dialog says **Browser · Needs approval**. See [browser actions](BROWSER_ACTIONS.md).

## Choosing Group or Bot in the composer

The row directly above the message input shows **Group** and **Bot**. The hint underneath states who will receive the message, and the action button changes between **Send** and **Ask**.

- Click **Bot** to address your personal `@bot`. Type ordinary messages and follow-ups without a tag. Click the Bot area again to cycle through your other bots enabled in this chat.
- Click **Group** to return to ordinary group messages.
- **F10** cycles through Group and your available bots. Alternatively, use `/to group`, `/to bot`, or `/to @planner`.
- An explicit leading `@bot` or `@planner` overrides the current selection. After that message is sent, the selected bot remains active for follow-ups. Changing the switch on a draft removes only its leading bot address and preserves the message body.
- The choice is remembered per account and chat, including after restart. Other accounts' selections remain separate. A disabled/missing selected bot preserves your draft instead of silently choosing another bot.
- **My agent** always addresses a bot. Use a group chat when you want Group mode.

Selecting Bot in a group does not create a private conversation: your message, reply and approved tool output remain visible in that group. Saved and synchronized group messages include the selected `@handle` automatically so participants can see the intended recipient. Incoming messages never select a recipient or invoke your bot. Changing recipients affects your next message, not an active job or its approvals.
