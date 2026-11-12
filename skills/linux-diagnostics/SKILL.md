---
name: linux-diagnostics
description: Check the state of this Linux computer (services, packages, disk, memory, network, logs). Use for "is X running, installed or broken" questions. Not for app crashes or browser tasks.
version: 1
---
# Linux diagnostics

Goal: find the current state with read-only commands, then explain it.

## When to use

- "Is sshd running?" / "is package X installed?"
- Disk, memory, network or uptime questions: "how full is the disk?", "why is Wi-Fi down?"
- "What do the service logs show?" when the process is still meant to be running

## When not to use

- An app crashed, dumped core, or panicked: use `crash-investigation`
- Opening or checking a web page in the browser: use `browser-check`
- The owner asked you to upgrade, install, reboot or edit config just to inspect status

## How to work

1. Propose one shell command at a time. Each one needs the owner's approval.
2. Start read-only: `systemctl status NAME --no-pager`, `journalctl -u NAME -n 50 --no-pager`, `rpm -q NAME` or `dpkg -s NAME`, `df -h`, `free -h`, `ip -brief addr`, `uptime`.
3. Never start an upgrade, install, reboot or config edit just to check status. Suggest those only as a clearly labelled next step.
4. If a command fails (permission denied, not found, sudo needs a password), say so and continue with other non-privileged checks. Never ask for a password in chat.
5. An unfamiliar package name is not proof that it does not exist. Check before saying so.
6. Report what you checked, the key output lines, and what is still unknown. Do not claim anything the output does not show.
