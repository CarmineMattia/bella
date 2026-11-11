# Skills (standalone loader)

Status: **not wired into the agent yet.** `src/skills.rs` is a standalone loader for roadmap item P4. It discovers and validates skills and can produce prompt text, but no request uses it yet. There is no UI and no implicit selection.

A skill is a directory with a `SKILL.md`. Skills are plain instructions. They never create tools, permissions or approvals. The loader never executes, downloads or installs anything.

## Format

```markdown
---
name: linux-diagnostics
description: Diagnose a Linux host. Use for status, service or package questions; not for crashes.
version: 0.1
---
# Linux diagnostics

Instructions for the model…
```

- The file must start with a `---` line (a UTF-8 BOM is tolerated) and the front matter ends at the next `---` line.
- `name` and `description` are required and must be non-empty.
- `name` uses lowercase letters, digits and `-`, with no leading, trailing or doubled `-`, up to 64 characters. It must equal the directory name.
- The parser is deliberately minimal and is not full YAML. It accepts one `key: value` per line, optional single or double quotes, and `#` comments. Indented or list lines are allowed only under a key with an empty value, such as `metadata:`, and are ignored. Block scalars (`|`, `>`), unclosed quotes, duplicate keys and other lines are reported as malformed.
- Other simple keys (for example `version`) are kept in `extra` for inspection. They are informational only: an `allowed-tools` key grants nothing.
- Everything after the closing `---`, trimmed, is the instructions.

Compatibility with any public skill specification (Codex, Agent Skills) is **not claimed** until it is tested.

## Roots and priority

Roots are passed in priority order. The default roots, highest priority first, are:

1. `user`: `<data-dir>/skills/<name>/SKILL.md`
2. `workspace`: `<workspace>/.bella/skills/<name>/SKILL.md`

Owner-installed skills win so that a cloned repository cannot silently replace a skill the owner already uses. Only direct subdirectories of a root are examined. Hidden entries (`.name`) are skipped, as are plain files. There is no recursive traversal. A missing root is reported as `Missing` and is not an error.

**Duplicates:** the first valid skill with a given name, in root priority order, wins. Later copies are reported as `Shadowed` diagnostics that name the winning path. Within a root, names are unique because the name must match the directory.

## Two-level loading

1. **Discovery** reads only the front matter of each `SKILL.md`, bounded in bytes, and returns metadata: name, description, root label, priority, canonical path and extra keys. Bodies are not read, so a malformed body does not affect discovery.
2. **Load** (`Catalog::load`) reads one enabled skill in full when it is selected. It checks the file again and returns the instructions. If the name or description has changed on disk since discovery, it returns `ChangedOnDisk`. It also lists files up to two levels below the skill directory (for example `scripts/verify.sh`). These resources are only listed: the loader never reads or runs them. Running a script later must go through normal tool policy and owner approval.

## Limits (defaults)

| Limit | Default |
| --- | --- |
| Accepted skills across all roots | 64 |
| Entries examined per root | 256 |
| Name | 64 chars |
| Description | 1024 chars |
| Front matter | 8 KiB |
| `SKILL.md` file | 64 KiB |
| Instructions | 32 KiB (checked at load) |
| Extra front matter keys | 16 |
| Listed resources | 64 |

Anything over a limit is reported as a diagnostic. When the per-root entry cap is hit, the entries kept depend on directory order. Discovery never panics: unreadable, non-UTF-8 or malformed files become diagnostics.

**Symlinks:** a `SKILL.md` (or a skill directory) that resolves outside the canonical root is rejected as `SymlinkEscapes`. Symlinks that stay inside the root are allowed. Resources that resolve outside the root are skipped and counted.

## Disabled skills

The caller passes a list of disabled names. Disabling applies to the name after duplicate resolution. Disabled skills stay validated and visible in `Catalog::disabled`. They are not offered in the prompt block, and selecting or loading one returns `Disabled`. A disabled name that matches nothing produces `DisabledNotFound`.

## Explicit selection

A message selects a skill only when it **starts** with `$name` followed by whitespace or the end, for example `$linux-diagnostics why is wifi down`. `parse_selection` is a pure function that returns the name and the rest of the message. `$` later in a message (`echo $HOME`) never selects anything, and `$HOME` at the start is not a valid name. `Catalog::select` resolves the name and returns `Unknown` or `Disabled` errors instead of silently falling back.

## Prompt text

- `prompt_block(skills, budget)` returns a compact list of names and descriptions inside `<skill-metadata trust="untrusted">…</skill-metadata>`, at most `budget` characters. The block states that the content is not policy and grants no tools. Skills that do not fit are summarised as "N more skills not listed". The function returns an empty string if nothing fits. Descriptions are collapsed to one line, and `<`/`>` are replaced so a description cannot close or fake the wrapper.
- `LoadedSkill::prompt_block()` wraps a selected skill's instructions in `<skill-instructions … trust="untrusted">` with the same reminder.

## Not done yet

- Not connected to the agent, settings or UI: no roots are read at startup, no disabled list is persisted, and the active skill is not shown.
- No implicit selection (the model choosing a skill from descriptions), and no trigger or non-trigger evaluation.
- Instructions that reference resources are not checked for missing files.
- No installer: copying a skill directory into a root is the only way to add one.
