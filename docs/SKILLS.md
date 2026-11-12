# Skills

Status: roadmap item P4, slice 3. Skills are connected to the owner's own agent requests and can only be selected **explicitly** with `$name`. There is no implicit selection yet.

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

## Using skills in the app

- Start a request to your agent with `$name`. In `/agent` that is `$linux-diagnostics is sshd running?`; in a group it is `@bot $linux-diagnostics …`, or the same text with Bot selected in the composer. The app loads that skill's instructions for this request only and sends the rest of the message as the request.
- The chat shows a note, **Skill linux-diagnostics · bundled · instructions only; tools and approvals unchanged**. The header status shows `· skill NAME` while the request runs.
- If the name matches no enabled skill, nothing is sent and your draft is kept. The status explains why: no skill with that name, or the skill is disabled. A `$` later in the message, or a name that is not a valid skill name such as `$HOME`, selects nothing, and the message goes out as ordinary text.
- `/skills` opens a window that lists the roots, enabled and disabled skills with their source, version, short content hash and path, and the diagnostics. **R** scans again. The hash is the first 12 hex digits of a stable FNV-1a digest of the `SKILL.md` bytes; it is for inspection only, not an integrity check.
- `/skills disable NAME` and `/skills enable NAME` change the `disabled_skills` list in `settings.json`. Existing settings files without that key load unchanged, and the key is left out while the list is empty. If no settings file exists, the change lasts only for the current session.
- Every owner request includes the compact metadata block, so the model knows which skills exist. The budget is 2,500 characters, kept small for local models. Only the selected skill's full instructions are added.
- A skill never changes the tool list, approvals or denial handling. The owner still approves every command. A denial still ends the tool phase for that request when a skill is active.
- **Guests of lent bots never get skills in this version.** That covers both chat-only and guest shell. A guest's `$name` is sent as ordinary text, with no metadata and no instructions. The agent applies this rule even if a caller forgets to clear the skill fields. Lent bots may get reviewed skills in a later slice.

Discovery runs at startup, before each of your agent requests, and when `/skills` opens, so edits on disk take effect without a restart. Loading a selected skill checks its file again.

## Roots and priority

Roots are passed in priority order. The default roots, highest priority first, are:

1. `user`: `<data-dir>/skills/<name>/SKILL.md`
2. `workspace`: `<workspace>/.bella/skills/<name>/SKILL.md`
3. `bundled`: `<data-dir>/bundled-skills/<name>/SKILL.md`

Owner-installed skills win so that a cloned repository cannot silently replace a skill the owner already uses.

### Bundled skills

The repository's `skills/` directory holds the skills shipped with Bella: `linux-diagnostics`, `crash-investigation` and `browser-check`. They are compiled into the binary. At startup the app writes them to `<data-dir>/bundled-skills/`, rewriting a file only when its content differs. That directory belongs to the app, and edits there are replaced. To change a bundled skill, copy its folder to `<data-dir>/skills/` and edit the copy: the `user` root wins, and the bundled copy shows as `Shadowed`. To turn one off, use `/skills disable NAME`. Nothing is downloaded. Each bundled skill has “When to use” and “When not to use” examples. `browser-check` states that `open_browser` only asks the launcher to open a page and does not verify, read or control it. `crash-investigation` gathers read-only evidence (`journalctl`, `coredumpctl list`/`info`, `dmesg` tail, `ulimit -c`, app logs), never runs destructive commands, and never uses `sudo` unless the owner has been told why and has approved it. Only direct subdirectories of a root are examined. Hidden entries (`.name`) are skipped, as are plain files. There is no recursive traversal of the root. A missing root is reported as `Missing` and is not an error.

**Duplicates:** the first valid skill with a given name, in root priority order, wins. Later copies are reported as `Shadowed` diagnostics that name the winning path. Within a root, names are unique because the name must match the directory.

## Two-level loading

1. **Discovery** reads only the front matter of each `SKILL.md`, bounded in bytes, and returns metadata: name, description, root label, priority, canonical path, extra keys and a short content hash. A malformed body does not fail discovery (a non-UTF-8 body is ignored for reference checks and reported when the skill is loaded).
2. **Load** (`Catalog::load`) reads one enabled skill in full when it is selected. It checks the file again and returns the instructions. If the name or description has changed on disk since discovery, it returns `ChangedOnDisk`. It also lists files up to two levels below the skill directory (for example `scripts/verify.sh`). These resources are only listed: the loader never executes them.
3. **References and scripts** (warnings, the skill still loads and stays listed). Markdown links and bare paths under `references/` or `scripts/` (and relative `./` / `../` paths) are checked: a missing file is `MissingResource`, a path that leaves the skill directory is `ResourceEscapes`. Referenced markdown files may be followed, bounded by `max_ref_depth` (default 3, counting `SKILL.md` as 1) and `max_refs` (default 32). Hitting the count limit is `ResourceTraversalLimit`. Scripts (files under `scripts/` or ending in `.sh` / `.bash`) are reported as `UnreviewedScript`. A front-matter list of reviewed scripts would not be enough to trust them: the skill system never runs a script. Any later run must go through the normal shell tool policy and owner approval. These warnings are also shown among the `/skills` diagnostics.

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
| Reference traversal depth (`SKILL.md` = 1) | 3 |
| Distinct local references checked | 32 |

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

- No implicit selection, where the model chooses a skill from its description, and no automatic trigger or non-trigger evaluation (the bundled skills document those examples for the owner and the model).
- No skills for guests of lent bots.
- The active skill is not saved with the task record. It is visible only through the chat note and the status while the request runs.
- There is no separate “review this script before enabling the skill” gate: unreviewed scripts are diagnosed and still require ordinary shell approval if someone tries to run them.
- No installer: copying a skill directory into a root is the only way to add one.
