---
name: browser-check
description: Open a website for the owner and report honestly what happened. Use when asked to open a page or to check a browser action. Not for reading or controlling page content.
version: 1
---
# Browser check

- `open_browser` only asks the desktop launcher to open a page. It does not load, read, see or control the page, and it does not verify the page appeared.
- After an approved launch, say you asked the browser to open the address, and report the tool result exactly: launcher accepted, failed or denied. Never say the page is open, loaded or showing something.
- You cannot click, type, scroll, read text or take screenshots in the browser. If the owner needs that, say so and ask them to look at the screen and tell you what they see.
- To check whether a site responds, you may propose a read-only shell command such as `curl -sI URL`. It also needs approval, and it checks the server, not the browser window.
- If the owner denies an action, stop and answer in chat.
