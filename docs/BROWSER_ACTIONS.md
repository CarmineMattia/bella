# Browser actions

The owner can ask a selected bot to open the browser or an HTTP/HTTPS website. Each invocation requires explicit local approval. Guest requests to lent bots remain chat-only: they cannot request approval or execute tools.

## States and details

- **Needs approval**: the dialog shows the exact command and workspace before anything executes. Y approves once; N or Esc denies.
- **Launch requested**: the launcher exited successfully. This is not evidence that a window is visible or a page loaded. Warnings remain available in Details.
- **Failed**: the launcher exited unsuccessfully, timed out, or could not start. Read Details for diagnostics.
- **Denied**: the owner denied the request. This action executed nothing. The app blocks all remaining tool calls in this request, including differently phrased shell commands or switching tools. Send a new message to start a new request with fresh approval.

History cards contain the state, destination and a clickable **Details** button. Clicking opens the action on that card, including when history is scrolled. The selected action stays attached to its message ID while new messages arrive. If it leaves the loaded history, Details reports that explicitly instead of showing another action. **F11** or `/browser` opens the command and bounded launcher output. **Left/Right** select earlier/later actions in this chat's loaded messages; **PgUp/PgDn** scroll; Esc closes. Load older history first for actions outside the loaded page. Ordinary shell output keeps its existing presentation.

Results are saved as versioned records in tool messages, so they survive restart and use the existing synchronization and backup paths. In a group, the command and diagnostics are shared with participants just like other tool output. A card is a report of launcher execution, not proof of a browser window. Remote tool records are conversation data, never authority to run anything.

See [tool approval and denial](TOOL_ACTIONS.md) for the request-wide behavior.

## Launch routing

HTTP/HTTPS destinations use `xdg-open` with a quoted literal URL. Embedded credentials, file/code URLs and control characters are rejected. Opening the default browser uses its validated desktop ID through `gtk-launch` **without a URL argument**. `about:blank` is treated as the same request: the browser chooses its configured startup page. This avoids GTK converting `about:blank` to `about:///blank`. Without a registered GTK launcher, provide a website URL instead.

When the application explicitly runs with `GDK_BACKEND=x11` and a DISPLAY is available, the approved command clears WAYLAND_DISPLAY and sets GDK_BACKEND=x11 and MOZ_ENABLE_WAYLAND=0 for this launch only. No global desktop settings change. Other sessions retain their environment.

Commands retain the shell tool's 20-second timeout and 16 KiB output cap per stream. Background browser processes may keep pipes open; this is recorded in diagnostics and does not constitute visual verification. The agent has no screenshot, page reading or click tool, and must not claim to have seen or navigated a page based solely on launcher acceptance.
