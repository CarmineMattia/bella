# Welcome setup

First launch opens a centered setup screen for your account name, model server, model name and agent workspace. Wide terminals show an introduction alongside the form; smaller terminals use a compact form that keeps the selected field visible.

- **Tab / Shift+Tab** or **Down / Up** move between fields. You can also click a field or scroll with the mouse.
- **Ctrl+U** clears the selected field. Normal cursor editing and bracketed paste work in each field. Pasted line endings do not submit the form.
- **Enter** or **Open chat** validates and saves the profile, then opens chat. Errors stay visible without discarding the other fields.
- **Esc**, **Ctrl+C** or **Exit** closes setup without saving changes.

Changing the account name selects a different local profile; it does not rename the author of old messages. Use Profile inside chat to edit a display name. The model server can be unavailable during setup: chat remains usable while offline. The workspace must be an existing directory. Existing saved profiles are reused on ordinary launches.

## Return from chat

Click **Exit to welcome** at the bottom right of chat, or send `/welcome`. The button is hidden only when the terminal is too narrow to fit it; the command remains available. To open setup directly from the terminal, use `./start --setup`.

Returning saves the current draft and ends the current local UI session. An active reply is stopped; a command already approved is allowed to finish through its bounded execution path and its output is retained before setup opens. No command is replayed. Ctrl+C during this wait forces a normal exit rather than returning to setup.

Setup starts with the current account and model values, including command-line overrides. Open chat saves the form and opens the lobby; drafts and message history remain in their original chats. Exit closes the app without saving the form. Model servers started by the launcher keep their existing lifetime and are not stopped by this navigation.

Network mode keeps the original hub configuration and account cache routing. Returning to setup is not a hub sign-out; the saved session is reused after setup. Use the network sign-out command when you want to revoke that session.
