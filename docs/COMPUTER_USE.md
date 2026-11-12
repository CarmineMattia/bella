# Computer use

Updated: 2026-11-12. Status: planned design; nothing in this document is a delivered feature or an implementation authorisation.

Bella today can only run `shell` and `open_browser`. The latter launches a browser or an HTTP/HTTPS URL after owner approval. A successful launch is not evidence that a window is visible or that a page loaded, and the agent has no screenshot, page-reading or click tool. See [browser actions](BROWSER_ACTIONS.md), [tool approval](TOOL_ACTIONS.md) and [the result contract](TOOL_RESULT_CONTRACT.md). Guest requests to lent bots never receive `open_browser`; an optional guest `shell` is not a sandbox.

This document adds computer use to the plan as harness phase P6 in [HARNESS_ROADMAP.md](HARNESS_ROADMAP.md), with a short pointer from [PLAN.md](../PLAN.md). The goal is a real Linux-integrated computer-use capability: a full action catalogue, perception that is not only pixels, an isolated agent desktop with its own cursor, and only later an optional live takeover and Android. Codex and the public computer-use APIs are references, not dependencies to install, and not a promise to copy their cloud or desktop-app products.

## Recommendation

A Linux computer can present a second, independent cursor, but not in the way that matters on Carmine's current session.

- **X11** can do it, through Multi-Pointer X (MPX): a second master pointer/keyboard pair really has its own cursor and its own focus. That is a property of the X server, not of a drawn overlay.
- **Wayland**, including GNOME, does not give applications a second system cursor on the owner's seat. Protocols and portals can inject pointer and keyboard events, but they drive the **same** seat cursor the owner is using. `ydotool` via `/dev/uinput` does the same. An overlay "ghost" cursor can *visualise* a bot pointer; it is not a second input device, and on GNOME it is awkward because Mutter does not implement the wlroots layer-shell protocol.
- Carmine's machine (observed 2026-11-12, hostname `Host-002`) is **Fedora 44, GNOME Shell 50.6, Wayland**, with XWayland on `DISPLAY=:0`. The right default for Bella is therefore **not** "a new cursor fighting for the live desktop", and **not** "browser-only as the end state".

Recommended default:

1. Build **CU1** first: a controlled, isolated-profile browser (Playwright). This needs no second cursor, works without a vision model, and is the smallest safe slice.
2. Then build **CU2**: the bot works in **its own nested or headless session** (own compositor, own cursor, no fight with the owner). Show that session to the owner live in a window or through a tightly scoped viewer. This is how Bella gets a real second cursor on Wayland.
3. Keep a **takeover / handoff** mode on the owner's live desktop for later, and only with explicit consent. On GNOME that path is the RemoteDesktop portal plus libei; it will move the owner's cursor.
4. **CU3** (smartphone) waits on the Android companion in PLAN milestone 6. Termux alone cannot control other apps.

Do not start by wiring `ydotool` to the live GNOME session. It is already installed and permitted here, but it would grab the one system cursor.

## Host snapshot (read-only, 2026-11-12, Europe/Rome)

Observed on the development host, without installing or starting new services:

| Item | Value |
| --- | --- |
| Hostname | `Host-002` (Fedora Linux 44 Workstation, kernel 7.2.8-200.fc44.x86_64) |
| Session | Wayland, `XDG_CURRENT_DESKTOP=GNOME`, `WAYLAND_DISPLAY=wayland-0`, `DISPLAY=:0` (XWayland running, started with `-enable-ei-portal`) |
| loginctl | User session 6 on `seat0`, `Type=wayland`, class `user`, active |
| Shell / compositor | GNOME Shell 50.6, mutter 50.6. Both advertise `--wayland`, `--headless`, `--virtual-monitor`, and `--display-server` ("full display server, rather than nested") |
| CPU / RAM / GPU | AMD Ryzen AI MAX+ 395, 16 cores / 32 threads; 62 GiB RAM (about 19 GiB used, 43 GiB available); AMD Radeon 8060S (Strix Halo, `c5:00.0`) |
| Input injection already present | `ydotool` 1.0.4 (`/usr/bin/ydotool`); `ydotoold` running as `cr1m3`; socket `/run/user/1000/.ydotool_socket`. `uinput` module loaded. `/dev/uinput` is `crw-rw----+` with an ACL granting `cr1m3` `rw` (not world-writable) |
| Official GNOME path present | `xdg-desktop-portal` 1.22.1, `xdg-desktop-portal-gnome` 50.0; session bus exposes `Screenshot`, `ScreenCast`, `RemoteDesktop` and `InputCapture`. `org.gnome.Mutter.RemoteDesktop` and `org.gnome.Mutter.ScreenCast` are on the bus. `libei` / `libeis` 1.6.0 installed. `gnome-remote-desktop` 50.2 and `grdctl` installed; `gnome-connections` is the viewer |
| Accessibility | `at-spi2-core` 2.60.7, `python3-pyatspi` 2.58.2, `org.a11y.Bus` and `at-spi2-registryd` running; Orca is installed |
| Isolation tools present | `podman` 5.8.9, Docker 29.9.2, `bwrap` 0.12.0, `systemd-nspawn`, `qemu-system-x86_64` / `qemu-kvm` 10.2.2 |
| Useful helpers present | `wl-copy` / `wl-paste` (package `wl-clipboard`) |
| Not installed | `xdotool`, `grim`, `scrot`, `xwd`, `Xvfb`, `Xephyr`, `weston`, `cage`, `sway`, `wmctrl`, `slurp`, `wtype`, `kwin_wayland`, Firecracker, TigerVNC, wayvnc, virt-viewer, Accerciser |
| Local model | `llama-server` on `127.0.0.1:8080`, alias `Qwen3.6-35B-A3B-UD-Q4_K_XL`, context 8192, health `ok`. `/v1/models` lists capability `completion` only. `/props` reports `"modalities": {"vision": false, "video": false, "audio": false}`. A projector file `mmproj-F16.gguf` (858 MiB) sits next to the GGUF but is **not** in the running command line, so this instance cannot accept images until it is restarted with the projector explicitly loaded. That restart is not part of this documentation slice |

Ollama remains on port 11434 and was not queried.

## Public references

Retrieved 2026-11-12. These are design references. Pin or re-check them before implementation; upstream pages change.

### What Codex actually offers

Be precise: "Codex" is several products, and they do not all have computer use.

- **Codex CLI and the Codex IDE extension do not include the built-in browser.** Official docs say to open the ChatGPT desktop app instead. Source: [Browser](https://learn.chatgpt.com/docs/browser).
- **ChatGPT desktop app (ChatGPT Work and Codex), macOS and Windows, in supported regions:** Computer Use can see and operate graphical apps. Setup is Settings > Computer use; the app asks permission before using an application; approved apps can be saved under Always-allowed apps. macOS needs Screen Recording and Accessibility. On Windows, Computer Use runs in the foreground of the active desktop and is expected to take over the pointer and keyboard; OpenAI itself suggests a VM if that desktop must stay independent. Linux is not listed. Source: [Computer Use](https://learn.chatgpt.com/docs/computer-use).
- **Built-in browser in that same desktop app:** ChatGPT Work or Codex can open pages, click, type, inspect rendered state, take screenshots and check the result. The profile is separate from the user's regular browser and does not automatically share tabs, cookies or saved passwords. Source: the same Browser page.
- **Codex Cloud:** a cloud task can be given browser access to inspect a local web app inside the task environment. Also on the Browser page.
- **Enterprise controls** distinguish the in-app browser, Browser Use (in-app browser or Chrome extension), and native Computer Use (desktop apps on macOS and Windows), including site allow/block lists and saved-approval policy. Source: [Manage browser and computer use in your Enterprise workspace](https://help.openai.com/en/articles/20001510-manage-browser-and-computer-use-in-your-enterprise-workspace).

Codex is a useful product reference for approval prompts, scoped tasks, a separate browser profile, and "don't take over the user's live desktop if you can use a VM". It is **not** a Linux computer-use implementation Bella can reuse, and it is **not** present in Codex CLI.

### OpenAI computer-use tool (Responses API)

The API-level reference for action shape and safety, independent of the Codex app:

- Guide: [Computer use](https://developers.openai.com/api/docs/guides/tools-computer-use).
- Drivers and approvals: [Browser and desktop drivers](https://developers.openai.com/api/docs/guides/tools-computer-use-sdk-drivers).

The current guide presents two integration styles. The first is **code execution** in a sandboxed browser or desktop (for example Playwright in JavaScript, or PyAutoGUI in Python). The second is the structured **`computer` tool**: `tools: [{ "type": "computer" }]`. The model returns a `computer_call` with an ordered `actions` array. Documented action types are `click`, `double_click`, `drag`, `move`, `scroll`, `keypress`, `type`, `wait` and `screenshot`. The application executes permitted actions in order and returns a `computer_call_output` screenshot. A call status of `completed` means the model finished generating the call; the application still has to execute it.

Safety in the API and SDK, which Bella should mirror in application code rather than trusting the model:

- If `pending_safety_checks` is non-empty, they must be acknowledged before any action runs. In the SDK, `approve_safety_checks` / `approveSafetyChecks` must return exactly true; a missing or declined callback stops the run. An optional `before_action` / `beforeAction` hook can refuse a single action, including the final screenshot. A model message cannot grant approval.
- Restrict the environment (isolated browser or VM, allow list of sites and actions). Treat screen content as untrusted. Confirm consequential actions (purchases, data transmission, destructive changes; typing sensitive information counts as transmission). Bound the run with step, time or cost limits, support cancellation, and verify the actual outcome.
- The SDK desktop example uses an isolated Linux guest: a virtual machine on Apple silicon, Docker on x86-64 Linux.

Older integrations may still mention `computer-use-preview`; the current tool type is `computer`. Follow OpenAI's migration notes if that comes up.

### Anthropic computer use

- Tool reference: [Computer use tool](https://platform.claude.com/docs/en/agents-and-tools/tool-use/computer-use-tool).
- Demo and caution text: [computer-use-demo](https://github.com/anthropics/anthropic-quickstarts/blob/main/computer-use-demo/README.md).

The current toolset (`computer_toolset_20260801`) exposes 17 member tools that the application executes in an environment it controls: `screenshot`, `zoom`, `left_click`, `right_click`, `middle_click`, `double_click`, `triple_click`, `left_click_drag`, `mouse_move`, `left_mouse_down`, `left_mouse_up`, `cursor_position`, `scroll`, `type`, `key`, `hold_key`, `wait`. Coordinates are in screenshot pixels. For tasks that stay inside web pages, Anthropic recommends the separate browser-use tool instead of a full desktop.

Documented precautions: a dedicated VM or container with minimal privileges; no account login secrets in the environment; a domain allowlist; a human confirms consequential actions (cookies, payments, terms of service). The reference environment is a sandboxed Linux desktop (Xvfb, a lightweight window manager, browsers and office apps) inside Docker. Screen and image content is untrusted because of prompt injection.

### Browser-only alternatives

- [Playwright MCP](https://playwright.dev/docs/getting-started-mcp) and [microsoft/playwright-mcp](https://github.com/microsoft/playwright-mcp): a Model Context Protocol server over Playwright. The model operates on the page **accessibility tree**, not pixels, so a vision model is not required. Tools cover navigation, click, type, fill, select, screenshots, keyboard and mouse, dialogs and tabs. Profile modes include persistent, **isolated** (`--isolated`, fresh session) and an extension that attaches to an existing browser. Bella should take the isolated-profile idea and the accessibility-tree preference, but keep the agent loop and approvals inside the app rather than depending on MCP as the authorisation boundary (consistent with P5).
- [browser-use](https://docs.browser-use.com/open-source/mcp): a Python agent framework that owns the perceive-decide-act loop. Useful as a comparison; less aligned with Bella, which already has an application-owned loop and owner approval.

### Linux input, isolation and perception

- X11 MPX and XI2: [MPX documentation](https://www.x.org/Development/Documentation/MPX/), [xinput(1)](https://xorg.freedesktop.org/archive/current/doc/man/man1/xinput.1.xhtml), [ArchWiki: Multi-pointer X](https://wiki.archlinux.org/title/Multi-pointer_X). xdotool does not currently target a chosen MPX master cleanly; see [xdotool#330](https://github.com/jordansissel/xdotool/issues/330).
- Wayland virtual pointer: [`zwlr_virtual_pointer_v1`](https://wayland.app/protocols/wlr-virtual-pointer-unstable-v1). Compositor table retrieved 2026-11-12: supported on Sway, Hyprland, niri, river, Labwc and other wlroots compositors; **not** on Mutter or KWin.
- Wayland virtual keyboard: [`zwp_virtual_keyboard_v1`](https://wayland.app/protocols/virtual-keyboard-unstable-v1). Same split: common on wlroots, not the GNOME/KDE path.
- Portals and libei: [xdg-desktop-portal RemoteDesktop](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.RemoteDesktop) (`ConnectToEIS()` is the recommended input path). libei: [gitlab.freedesktop.org/libinput/libei](https://gitlab.freedesktop.org/libinput/libei).
- GNOME headless / remote sessions: [GNOME Remote Desktop README](https://github.com/GNOME/gnome-remote-desktop/blob/master/README.md); mutter headless virtual monitors ([mutter!1698](https://gitlab.gnome.org/GNOME/mutter/-/merge_requests/1698)).
- AT-SPI: [AT-SPI2](https://www.freedesktop.org/wiki/Accessibility/AT-SPI2/), [pyatspi example](https://www.freedesktop.org/wiki/Accessibility/PyAtSpi2Example/), [stack architecture](https://gnome.pages.gitlab.gnome.org/at-spi2-core/devel-docs/architecture.html).
- Set-of-marks grounding: [Yang et al., "Set-of-Mark Prompting", arXiv:2310.11441](https://arxiv.org/abs/2310.11441).

## Second cursor and input injection

The question is whether the bot should have its own new cursor on the computer, whether a computer can support that, or whether computer use should stay browser-only.

### X11 (MPX)

X Input 2 Multi-Pointer X is the one mainstream Linux mechanism that actually gives a second cursor. `xinput create-master Name` creates `Name pointer` and `Name keyboard`, each with an XTEST slave. Each master pointer has its own sprite; each master keyboard has its own focus. Core requests such as `XQueryPointer` and `XGetInputFocus` use the client's **ClientPointer** and its paired keyboard, so a traditional toolkit app still behaves as if there were one pointer unless the window manager or the injecting client sets the ClientPointer. Popup grabs are a known caveat: a core grab can lock out the other pointer.

To inject through the second master, the client must target that device (XI2 / the master's XTEST slave), not the Virtual core pointer. Stock `xdotool` talks to the core pointer via XTEST and is not a complete MPX tool.

This path is relevant for a dedicated X11 agent session (Xvfb, Xephyr, or a nested X window manager). It is a poor primary path on Carmine's live session: that session is GNOME Wayland. XWayland is present, but an MPX pointer inside XWayland does not control native Wayland apps and is easy to get wrong.

### Wayland, by compositor family

Wayland has no global "create a second pointer" request comparable to MPX. Input is mediated by the compositor and, on GNOME and KDE, by portals.

| Path | What it is | GNOME (Mutter) | KDE (KWin) | wlroots (Sway, Hyprland, niri, …) | Independent cursor? |
| --- | --- | --- | --- | --- | --- |
| `zwlr_virtual_pointer_v1` / `zwp_virtual_keyboard_v1` | Compositor protocols that emulate a pointer or keyboard, optionally suggested onto a seat | No (Mutter 51 listed as unsupported on wayland.app, 2026-11-12) | No for these wlr protocols | Yes, this is the usual path | Usually **no**: the device is attached to a seat and moves that seat's cursor. A separate seat would be required for a true second cursor, and compositors rarely expose one to ordinary clients |
| libei via `xdg-desktop-portal` RemoteDesktop (`ConnectToEIS`, or the older `Notify*` D-Bus methods) | User-consented input injection, with optional ScreenCast | Yes: `xdg-desktop-portal-gnome` plus Mutter RemoteDesktop. Installed and on the bus here | Yes: `xdg-desktop-portal-kde` | Only if a portal backend implements RemoteDesktop (xdg-desktop-portal-wlr historically focused on ScreenCast; some compositors need a shim) | **No** on the live session: GNOME treats this like a remote-desktop client. Relative motion moves the hardware cursor; absolute devices target regions of the same seat |
| `uinput` / `ydotool` | Kernel virtual input device | Works if `/dev/uinput` is accessible. Here the module is loaded, `cr1m3` has an ACL, and `ydotoold` is already running | Same | Same | **No**. Events enter the same seat as a physical mouse. The owner's cursor moves. Needs careful permissions; world-writable `/dev/uinput` would be unacceptable |
| Overlay / "ghost" cursor | Draw a marker where the bot intends to point (`zwlr_layer_shell_v1`, or a always-on-top window) | No wlr-layer-shell; a GNOME Shell extension or a regular window would be a hack | KWin implements layer-shell | Yes, layer-shell is standard | Visual only. It does not create a second input device |
| Separate logind seat | Attach devices to `seat1` | Possible in principle, not practical for a bot sharing the same monitors | Same | Same | A real second cursor at the cost of a second seat and display assignment; not the product default |

Honest summary for GNOME Wayland, which is the development host: **there is no supported way to give the bot a second system cursor on the owner's live desktop**. The supported injection path (portal + libei) is the right one for an explicit takeover, and it will fight the owner for the one cursor. `ydotool` is worse as a default (no portal consent dialog, same shared cursor). A ghost overlay can be a later visualisation aid in a takeover UI, not the mechanism.

### What this means for Bella

- **Own cursor:** yes, by giving the bot its own session (CU2). Inside that session the bot's pointer is the only pointer. The owner watches from outside.
- **Live desktop:** optional takeover later (CU2-live), never the default, always explicit, always stoppable.
- **Browser-only:** a first slice, not the ceiling.

## Isolated agent desktop

The recommended CU2 runtime is a dedicated graphical session that is not the owner's live desktop.

Options, in preferred order for this host:

1. **Nested or headless GNOME/Mutter session.** mutter and gnome-shell 50.6 already support `--wayland`, `--headless`, `--virtual-monitor WxH`, and distinguish nested from `--display-server`. A nested session can appear as a window on the owner's desktop (live view with no extra VNC stack). A headless session with a virtual monitor can be reached through GNOME Remote Desktop (`grdctl`, `gnome-remote-desktop-headless.service`) or by attaching a ScreenCast portal / PipeWire viewer. Use a **separate D-Bus session** (`dbus-run-session`) so the agent session does not share the owner's AT-SPI bus, secrets, or portal grants by accident. This is the best fit for "looks like GNOME, own cursor, show it to the owner".
2. **Smaller nested compositor** (Weston, Cage, or a Sway instance) if a full GNOME session is too heavy. These are **not installed** here today. They would make wlroots virtual-pointer protocols available inside the nest, which simplifies injection, at the cost of a less familiar desktop.
3. **Classic X nest:** Xvfb or Xephyr plus a window manager, with MPX if a visible second pointer inside that X server is wanted. Also **not installed** here. Still valid for tests and for a tightly locked container.
4. **Container or VM:** Podman or Docker (both present), `bwrap` for a lighter sandbox around the session, `systemd-nspawn`, or QEMU/KVM (present) when stronger isolation is required (network allowlists, no host credentials, no host mounts). Firecracker is not installed. OpenAI's and Anthropic's public guidance both start from an isolated Linux guest; Bella should follow that when the agent may reach a browser or untrusted content.

Live view: prefer a nested window first (the owner just sees a window). A TUI preview (low-FPS screenshot in the terminal) is a later convenience, not a substitute for a real viewer when clicking. VNC/RDP (GNOME Remote Desktop + Connections, already on this host) is the fallback for headless sessions. Bind the viewer to localhost unless the owner explicitly shares it.

**Takeover / handoff** (CU2-live): the owner grants a RemoteDesktop + ScreenCast portal session on the live GNOME desktop, watches a live view, and can stop it immediately. This shares the seat cursor; the UI must say so plainly. Do not auto-start it.

## Perception

Do not make screenshots-plus-vision the only path. The local model instance today cannot see images.

1. **Structured trees first.**
   - Browser: Playwright / CDP accessibility tree and DOM (text, roles, refs). This is CU1 and needs no vision.
   - Native apps: AT-SPI over D-Bus (`org.a11y.Bus`, pyatspi or a Rust `atspi` client). Walk applications, windows, roles, names, states, geometry; use `Action.DoAction`, `EditableText`, `Component.GrabFocus` when the app implements them. Accessibility may need `org.a11y.Status.IsEnabled`; some toolkits only publish a tree after that flag is set, and some apps (parts of Chromium, games, custom widgets) are incomplete or reject direct activation. When AT-SPI cannot act, fall back to coordinates in the *agent* session.
2. **Screenshots** for verification, for apps with a poor accessibility tree, and for visual questions. Capture via the portal `Screenshot` / `ScreenCast` APIs, or via `grim`/`slurp` on wlroots, or via X11 (`xwd`, `scrot`) inside an X nest. `grim` is not installed on the host today; on GNOME the portal is the native path.
3. **Set-of-marks (SoM)** when a vision-capable model is actually loaded: overlay numbered boxes on interactable regions, let the model choose an id, map the id back to coordinates or an AT-SPI node. This reduces raw-pixel pointing errors. It requires vision.
4. **OCR** as a last fallback for text that is not in AT-SPI or the DOM.

**Local Qwen and vision.** Official Qwen3.6-35B-A3B is documented as a vision-language model. The copy on disk includes `mmproj-F16.gguf`. The running `llama-server` on port 8080 was started without that projector, and `/props` reports `vision: false`. Until someone restarts the server with the projector and a trivial image probe succeeds, CU1 and CU2 must work with accessibility trees and must report "vision unavailable" honestly rather than pretending to have seen a screenshot. Hardware (64 GiB, 8060S) is likely able to load the projector; that is an inference, not a measurement from this slice.

## Action catalogue

Authorisation stays in the app. Skills cannot grant approval. Every state-changing action is either approved individually or covered by an explicit bounded plan (see Safety). Results use the existing structured tool-result contract, including `unverified` when a click was sent but the effect was not independently observed.

### Session and pointer (CU2)

| Action | Notes |
| --- | --- |
| `screenshot` | Full session or monitor. Preferred observation after a batch. |
| `zoom` / region capture | Crop or higher-resolution region, in screenshot pixels, as in Anthropic's `zoom`. |
| `cursor_position` | In the agent session, this is well-defined. On a live Wayland seat, the compositor may not expose a global pointer; cache the last injected position and say so. |
| `move` | Move the agent-session pointer. |
| `click` | Left / right / middle; optional modifiers. |
| `double_click`, `triple_click` | |
| `drag` | Press, move, release; refuse if the endpoint leaves the permitted region. |
| `scroll` | Direction and amount, optional coordinates. |
| `type` | Literal text into the current focus. Never used for passwords or other secrets (the app must block this class of input). |
| `key` / hotkey | Key or combination (`Return`, `ctrl+s`, `alt+Tab`), optional repeat. |
| `hold_key` | Bounded duration. |
| `wait` | Bounded pause; refuse unbounded waits. |
| `open_app` | Launch a permitted application inside the agent session (desktop id or an allowlisted command), not an arbitrary host binary. |
| `window_list`, `window_focus`, `window_resize` | Prefer compositor or session IPC; `wmctrl`/`xdotool` only inside an X nest (neither is installed on the live host). |
| `clipboard_read`, `clipboard_write` | Always approved separately. On the live session use `wl-copy` / `wl-paste`; treat clipboard contents as untrusted. |
| File pick dialogs | Prefer a structured "upload this path" tool that the app performs, rather than asking the model to drive a picker with clicks. |

### Browser-specific (CU1, also available during CU2)

`navigate` (allowlisted schemes and hosts), `back` / `forward` / `reload`, `read_page` (accessibility tree and visible text), `find_element`, `click` / `type` / `fill` / `select` by element ref, `screenshot`, tab create / close / switch, dialog accept / dismiss, `upload` of an owner-approved local file. No attachment to the owner's everyday browser profile by default. HTTP/HTTPS only, consistent with the existing URL policy in [BROWSER_ACTIONS.md](BROWSER_ACTIONS.md).

### Phone (CU3)

Tap, long-press, swipe, type, back, home, app switch, screenshot, and a structured dump of the current accessibility tree, through the Android companion (`AccessibilityService`) or, where the owner has enabled it, ADB / Shizuku. This depends on PLAN milestone 6. Termux cannot control other apps by itself.

## Comparison

| | Codex (product) | OpenAI `computer` tool / CUA | Anthropic computer use | Playwright MCP | browser-use | Bella (planned) |
| --- | --- | --- | --- | --- | --- | --- |
| Where it runs | ChatGPT desktop app (macOS/Windows) and Codex Cloud browser; **not** Codex CLI | Your browser or OS driver against the Responses API | Your VM/container against the Messages API | Local (or attached) browser via MCP | Python agent loop, local or cloud browser | Local-first, in the Bella app on Linux, later Android |
| Desktop GUI | Yes, on macOS/Windows, with app-level permission | If you provide a desktop driver | Yes, designed for a sandboxed Linux desktop | No | No | Yes, defaulting to an **isolated session with its own cursor** |
| Browser control | Built-in isolated profile; optional Chrome extension | Driver or Playwright code execution | Possible, but they recommend a dedicated browser tool | Yes, accessibility-tree first | Yes | CU1: Playwright, isolated profile, no user cookies by default |
| Action shape | Product UI, not a public Linux driver | `click`, `double_click`, `drag`, `move`, `scroll`, `keypress`, `type`, `wait`, `screenshot` | 17 members including `zoom`, `hold_key`, `cursor_position`, button variants | Page tools (navigate, click, type, snapshot, tabs, …) | Library-defined browser actions | The union of the API catalogues plus window management, clipboard (approved), open-app, and phone actions |
| Perception | Screenshots / visual models (e.g. GPT-6 Astra when offered) | Screenshots for the `computer` tool; page text if you use a browser driver | Screenshots; zoom for small UI | Accessibility tree; screenshots optional | Mixed | AT-SPI + DOM/a11y first; screenshots and SoM when vision is actually available; OCR fallback |
| Approval | Permission per app; prompts for sensitive actions; admin policy in Enterprise | `pending_safety_checks` and `before_action`; app must confirm | Human confirm for consequential actions | Whatever the MCP host implements | Varies | **Application-owned**, like today's shell: per action or a bounded plan; skills and backends cannot grant it (P5) |
| Isolation | Separate browser profile; Windows guidance includes a VM | Isolated browser or VM recommended | Dedicated VM/container, no host credentials, domain allowlist | Isolated or persistent profile | Local or cloud browser | Isolated browser profile, then isolated desktop session; guests excluded until sandbox + quotas |
| Second cursor | No (it uses the apps you allow, on macOS/Windows) | Only if your environment provides one | One cursor inside the sandbox desktop | n/a | n/a | Yes, **inside the agent session**; not on the owner's live GNOME seat |
| Lent bots / guests | n/a | n/a | n/a | n/a | n/a | No computer use for guests until isolation and quotas exist |
| Android | Not this feature | Not this feature | Not this feature | No | No | CU3, after the companion app |

## Safety and UX

- **Approval granularity.** Default: owner approves each state-changing action (or a short batched group shown as one plan: for example "click Search, then type X") with a visible summary, target and screenshot or accessibility excerpt. A later option is a bounded plan: a step budget, allowed domains or apps, a time limit, and a visible live view. Changing the plan requires a new decision. One denial still ends the tool phase for that request, consistent with current denial rules. A later user message starts a new request.
- **Kill switch.** Stop (existing Stop / Ctrl+X / `/stop`) cancels a pending computer-use approval, stops further actions, and leaves already-executed effects in place rather than inventing an undo. In live-view and takeover modes a dedicated hotkey or click must abort injection immediately.
- **No secrets typed by the bot.** The app refuses to type into password fields and refuses action arguments that look like credentials. Website sign-in, if ever offered, must use a human-filled path (the ChatGPT desktop browser documents a similar idea: credentials go to the browser, not to the model). The existing rule remains: the app never collects a sudo password in chat.
- **Allowlists.** Browser navigation is restricted to an owner-configurable host allowlist (default conservative). The agent session's network should be equally restrictable. `open_app` is allowlisted.
- **Untrusted screen content.** Text and images from pages, AT-SPI names, clipboard data and screenshots cannot grant permission or override owner instructions. This is the same stance as OpenAI and Anthropic on prompt injection.
- **Screenshots.** Keep them only as long as the task and any explicit history setting require. Default short retention; redaction hooks for the owner's own screen if takeover is ever enabled (notifications, other windows). Do not ship raw desktop screenshots to a remote log by default.
- **Action log.** Every injected action, approval, denial, cancellation and observation is recorded with the existing task/action identifiers and is inspectable in the app.
- **Guests of lent bots.** No computer use, no browser control, no clipboard, no screenshots of the owner's session, until a real sandbox and quotas exist. The current optional guest `shell` does not imply any of this.
- **Final authorisation stays in Bella**, not in a backend, MCP server, skill or model (P5).

## Milestones, effort and tests

Efforts are order-of-magnitude estimates for a focused implementation slice after Carmine schedules it, not commitments. Checkboxes move only after reviewed implementation and validation.

### CU1 — Controlled browser (build first)

Playwright-backed tools in an isolated profile, no import of the owner's cookies, HTTP/HTTPS only, accessibility-tree `read_page`, screenshots as evidence, navigate / click / type / fill / select / tabs, per-action or short-plan approval, structured results, honest "vision unavailable" when the model cannot accept images.

Why first: no second-cursor problem, no live-desktop risk, works with the text-only local model, and already extends `open_browser` in a way the [browser-check](BROWSER_ACTIONS.md) skill can later tell the truth about.

Effort: about one to two weeks including tests and UI approval cards.

Acceptance (fake backend, no live websites required in CI):

- A scripted page can be opened, read via the accessibility tree, clicked and filled; the structured result matches the fake page.
- Owner denial executes nothing and ends the request's tool phase.
- Stop / cancel during a pending approval or mid-plan leaves a recorded incomplete result and does not start the next action.
- Navigation to a non-allowlisted host is blocked before approval.
- Guests are never offered the tools.
- When vision is unavailable, screenshots may be stored as evidence for the owner but are not claimed as model-visible.
- `git diff --check` and the usual `./scripts/check` plus smoke tests stay green.

### CU2 — Isolated Linux agent desktop

A nested or headless session with its own cursor, a live view for the owner, the full session action catalogue, AT-SPI plus screenshots, SoM only if a vision model is loaded, clipboard and window actions with approval, optional tightening later into Podman/QEMU.

Effort: about three to six weeks, itself sliceable (session + view first, then AT-SPI, then the remaining actions).

Acceptance:

- Starting a session does not move the owner's live cursor; the owner can use their desktop while the bot works inside the nest.
- Fake-backend and, where possible, a nested-session test: move, click, type, key, screenshot, window list/focus, denial, cancel, AT-SPI read of a known widget.
- Killing the Bella tool or the session tears down injection and does not leave a stray compositor grabbing input.
- Untrusted content in the agent session cannot approve a later action.

### CU2-live — Optional live takeover (later)

Portal + libei (and ScreenCast) on the owner's GNOME session, only after an explicit consent dialog that states the owner's cursor will be used. Kill switch required.

Effort: about one to two weeks after CU2, plus UX review.

Acceptance: no injection without consent; Stop immediately ends injection; a denial or failed portal grant is a recorded failure, not a silent fallback to `ydotool`.

### CU3 — Smartphone

After the Android companion in PLAN milestone 6: AccessibilityService or ADB/Shizuku, with the same approval and logging rules.

Effort: depends on the companion; not scheduled ahead of it.

## Out of scope for this document

No code lands with this slice. No packages are installed, no model server is restarted, no portal session is started, and no merge or push is implied. Enabling the local vision projector, installing Xvfb/Weston, or turning on GNOME Remote Desktop are later, explicit steps.
