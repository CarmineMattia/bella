---
name: crash-investigation
description: Investigate an app crash with read-only logs and core-dump info (journalctl, coredumpctl, dmesg, ulimit, app logs). Use when a process died or dumped core. Not for slowness, service status or browser tasks.
version: 1
---
# Crash investigation

Goal: gather read-only evidence of a crash, then explain what it shows and what is still unknown.

## When to use

- "Bella crashed" / "the app segfaulted" / "it died after I clicked X"
- A core dump appeared, or `coredumpctl list` might have a recent entry
- Journal or app logs show a panic, abort, or unexpected exit

## When not to use

- "Is sshd running?" or disk/memory/package status: use `linux-diagnostics`
- Opening or checking a website: use `browser-check`
- The machine is merely slow, or the owner wants a fix applied rather than evidence collected
- The owner asked you to delete logs, cores, or files to "clean up"

## How to work

1. Propose one shell command at a time. Every command goes through the normal approval flow. Never run a command that has not been approved.
2. Start with read-only checks, in an order that fits what you already know:
   - `journalctl -n 80 --no-pager` or `journalctl -u NAME -n 50 --no-pager` (and `--user` when it is a user service)
   - `coredumpctl list` then `coredumpctl info PID` or `coredumpctl info NAME` for a recent match
   - `dmesg -T | tail -n 80` for kernel oops or out-of-memory lines (this may need privileges; if it fails, say so)
   - `ulimit -c` to see whether core dumps can be produced
   - App logs under the data directory (for Bella, typically `$XDG_DATA_HOME/bella` or `~/.local/share/bella`)
3. Never run destructive commands: no `rm`, shred, truncate, vacuum, overwrite, reboot, install, or config edits just to investigate. Do not use `coredumpctl dump` unless the owner clearly wants a file written, you name the destination, and they approve it.
4. Never use `sudo` unless you have told the owner why that exact command needs it, what it will do, and they approve it. Never ask for a password in chat. If a command fails with permission denied, report that and continue with other non-privileged checks.
5. Report the exact commands you ran, the key output lines, timestamps, and what is still unknown. Do not claim a cause the output does not show.
