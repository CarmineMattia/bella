# Tool approval and denial

The local owner reviews every shell command or browser launch before it executes. **Y** approves that single action. **N** or **Esc** denies it.

A denial now ends the tool phase for the entire current request. The app enforces this in code, rather than relying only on the model's instructions:

- Other tool calls in the same model response are blocked after the denied call.
- Later model turns in the same request receive no tool definitions. Calls returned anyway are blocked before argument parsing, command preparation or approval.
- Changing a command or switching between shell and browser does not reopen approval.
- The bot can still finish with a chat reply. To ask for another action, send a new message; normal owner approval is required again.

This does not undo actions you already approved and that already ran. Their existing output remains saved. Denied actions execute nothing. The existing task cancellation behavior remains separate: stopping a running approved command waits for its bounded result rather than claiming to undo its effects.

The scope is one agent request. Denial is not a permanent bot setting and does not affect other app instances. Guest requests to lent bots stay chat-only unless the owner has set *Guest tools when lent* to `shell` for that bot. In that case only `shell` is offered, each command still needs the owner's approval (the dialog names the guest and the working directory), and `open_browser` is never available. A denial still ends the tool phase for that request. This is not a sandbox; see [known issues](KNOWN_ISSUES.md).

Browser cards show Denied, and their Details contain the reason. Shell denial messages explain that further actions for this request are stopped. Repeated blocked calls return a result to the model without filling the conversation with repeated log messages. The existing six-model-turn limit still bounds a model that refuses to finish.

## Non-interactive commands and diagnosis

Shell tools run in a separate session with no controlling terminal and closed standard input. The shell's `sudo` function adds `-n`: existing cached or NOPASSWD authorization can work, but an action needing a password fails promptly. Explicit sudo paths or nested shells also have no controlling terminal. The app never collects a sudo password in chat and does not start a password prompt through an inherited askpass helper.

Privilege/authentication failures and timeouts include explicit notices for the agent. A failed command is different from an owner denial: the bot may request further diagnostic commands, each requiring approval. For status/crash questions the agent is instructed to inspect installed package metadata, process state and relevant logs first, continue with useful non-privileged checks after an error, and avoid starting an upgrade just to check status. An unfamiliar package name is not evidence that it does not exist. Tasks requiring fresh administrator authentication still need the owner's own terminal; the app does not pretend to have completed those actions.

Captured output is capped at 16 KiB per stream. The truncation footer appears only if output was actually cut. The 20-second execution limit still applies to the owner's own commands; timeout results do not prove an update completed and must not trigger blind repeats of state-changing commands. Guest shell commands have no time limit; Stop sends SIGTERM then SIGKILL to the command's process group.

## Copying messages

Every user, bot and tool message has a compact **⧉** Copy icon in its header. Copy preserves the full original text, Unicode, code and line breaks rather than the wrapped screen layout. Browser action cards copy a readable report including their command and diagnostics. A streaming reply copies the text received so far.

Copying runs in a worker and does not block the chat editor. A successful native clipboard helper shows **✓**. The app tries available `wl-copy`, `xclip` and `xsel` helpers with bounded execution, passing message text through standard input rather than a shell. If no native helper works, it sends an encoded OSC 52 request to the terminal and shows **↗**, since terminal clipboard permissions and support cannot be confirmed by the app. Terminal fallback is limited to 64 KiB of source text; larger messages require a native helper. Copy never sends message text to the model or the network.

## Structured runtime results

New shell and browser results are stored as versioned action records. Chat, model tool responses, readable Copy text, `--history` and task details use the same outcome: Succeeded, Failed, Timed out, Cancelled, Denied or Blocked. Browser success means **Launch requested**, with window/page visibility still unverified. Older text output and browser cards remain readable.

Owner timeouts are measured by the parent process, rather than inferred from exit code 124 or output text. Started commands may have external effects even after failure, timeout or cancellation. Guest Stop retains its separate process-group behavior. Existing approval and request-wide denial rules remain enforced in code.

Records fit the hub's 64 KiB message limit by reducing captured output when necessary and explicitly marking that reduction. Commands are preserved exactly; shell proposals exceeding the 8 KiB review limit and browser URLs exceeding 4 KiB fail before execution. See [the result contract](TOOL_RESULT_CONTRACT.md) for scope, compatibility and remaining work.
