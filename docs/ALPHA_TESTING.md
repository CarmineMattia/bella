# Alpha acceptance checklist

Test the extracted `0.1.0-alpha.3` package in a UTF-8 terminal, at least 80×24. Use a fresh data path for experiments; do not mix test requests with important existing chats. No paid API or online account is required.

## 1. Package and setup

- Run `./start --version`: expect `bella 0.1.0-alpha.3`.
- Run `./start --help`: launcher and app options are visible.
- Run `./start --chat-only --data /absolute/path/alpha-test/chat.sqlite` with no existing settings: setup appears. Choose a name and save; reopening the same data path reuses it.
- Run `./start --setup --data /absolute/path/alpha-test/chat.sqlite` to edit the profile. Cancel and confirm that existing settings remain unchanged.
- The package must run without Rust or its source toolchain. It needs Python, a terminal, and ordinary Linux shell helpers.

## 2. Chat and organization

- Open two sessions on the same data path with different names. Exchange messages in lobby; create a group and join it from the other terminal.
- Paste Unicode/multiline text, edit it with arrows, then send. It must arrive once with its line breaks.
- Switch chats with an unfinished draft, then restart. The draft must be restored.
- Pin a chat with F3; newer traffic must not move an unpinned chat above it.
- Archive and restore a chat. Messages, drafts and pins remain; the other name's preferences are unchanged.
- Open another room while an archived chat receives traffic. It remains archived and unread, with an archive-tab indicator.
- Scroll a long sidebar and right-click a visible chat. The menu must affect that chat, not a hidden one at the same list index.

## 3. History and presence

- Ctrl+F searches text older than the initially loaded 200 messages. Open a result and confirm its highlight and surrounding context.
- New messages must not move the historical viewport or be marked read. Click Latest messages or press Ctrl+End and confirm read state catches up.
- In F6, a second live session viewing the same room is listed. Moving or closing it changes its local presence; a historical author can remain without a live session.

## 4. Model and owner tools

- Start the installed local model with `./start`. A second session reuses it. F2 should list the selected model as available.
- Ask for a short reply in `/agent`; text streams into the originating conversation.
- In a group, ask `@bot Use the shell tool to run exactly: printf alpha-ok`. Inspect and approve the proposed command. The decision and `alpha-ok` output appear in that group.
- Repeat with a command you deny. It must not execute.
- Stop a long reply. Received text is labelled interrupted; another request can start after Stop completes.
- While an already-approved short command runs, quit. The app waits for its result and records it. A second interrupt is the explicit force-exit option.

## 5. Crash recovery and activity

- Use disposable data. Interrupt the process unexpectedly during a streaming reply. Reopen it: F5 shows interruption, received text is recovered once, and no model request is automatically sent.
- Repeat while a command awaits approval. On reopening, the approval is cancelled and the command is not executed.
- A second live instance must not have its tasks marked interrupted by another app startup.
- Check F5 task details for state, last command/approval, received text and error. Enter opens the correct conversation.

## 6. Backup, diagnostics and model cleanup

- Create a backup with `./start --backup /absolute/path/new-snapshot.sqlite`. Reusing the destination must fail without changing it.
- Close source app sessions, copy the snapshot to a new filename, then open that copy with `--data`. Verify history and preferences.
- Run `./start --doctor`: model available means a successful full check. With an offline endpoint it must return nonzero and distinguish `chat_ready` from `model_ready` in JSON.
- `./start --stop-model` stops a server created by the launcher. An existing external model server must remain running.

## Automated evidence

`tests/alpha_smoke.py` executes these terminal/launcher paths with temporary databases and controlled local HTTP model fixtures. It does not load a GGUF or prove model quality. The separate Rust live-model test covers actual local streaming, approval and tool output. See [developer checks](DEVELOPMENT.md). Release verification results are recorded separately from this checklist; unchecked manual exploration remains tester feedback, not a promised feature.

## Profiles and bots

- F7: change your display name/About and import a PNG/JPEG from a path containing spaces. Save, restart, and confirm the picture/name remain. Verify drafts/history still belong to your original account ID.
- Cancel an edit; try an invalid picture; confirm the last saved profile survives. Blank keeps a picture; `-` removes it.
- F8: create `planner`, set instructions and a picture, and send `@planner help plan a test`. Confirm the reply's name and activity record.
- Edit the original @bot name/picture. Add/remove the new bot in another chat. A removed bot's request stays in your draft and makes no model call.
- Open a second local account: its own bots/preferences remain separate, while public replies and profile pictures are visible.
- Remove the original image file, back up SQLite, and restore into a new database. The imported thumbnail must remain available.

## Two-computer network test

Follow [NETWORK.md](NETWORK.md). Start one hub and connect two client computers with different accounts. Exchange lobby/group messages, update pictures, inspect shared bot ownership, disconnect one client, send while offline and verify a single delivery after reconnecting. Confirm another account cannot see your `/agent` history, that received mentions never run local tools, and that normal `./start` still opens your separate local chats. Run the packaged `tests/network_smoke.py` for automated loopback coverage.

## Composer recipient switch (development build)

Click Bot and send two messages without tags; both must ask the selected bot. Click Group with an unfinished draft and confirm the body is retained, then send a human-only message. F10 should include only your enabled bots, and named bots must retain their model/instructions. Switch chats and restart to verify the per-chat choice. Send an explicit @mention from Group mode and confirm that follow-ups go to that bot. Disable the selected named bot: sending must keep the draft until another recipient is chosen. Incoming mentions and other accounts' choices must not change your target or run your agent.
