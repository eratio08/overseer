# CLI DX/UX Improvements

**Status:** Ready for implementation
**Effort:** L (1-2 days)
**Author:** spec-planner
**Date:** 2026-02-01

## Problem

Overseer CLI (`os`) has poor discoverability and visual feedback:
1. No shell completions - must remember commands/flags
2. No color - output hard to scan, status buried
3. Flat task list - hierarchy invisible, relationships unclear
4. No level-specific filtering - `--parent` required to scope

## Goals

- Shell completions for bash, zsh, fish (static)
- Colored, scannable output respecting NO_COLOR
- Tree view as default for task list
- Type filter flags for milestone/task/subtask

## Non-Goals

- Dynamic completions (task IDs from DB)
- Interactive TUI
- Themes/customization
- nushell support

## Design

### D1: Shell Completions

Add `clap_complete` crate. New subcommand:

```
os completions <shell>
```

Where `<shell>` is `bash | zsh | fish`.

Outputs shell script to stdout. User sources it:
```bash
# ~/.zshrc
eval "$(os completions zsh)"
```

**Scope:** Static only (subcommands, flags, no task IDs).

### D2: Styled Output

Add `owo-colors` crate. Create `output.rs` module with `Printer` struct.

**Color scheme:**
| Element | Style |
|---------|-------|
| Task ID | cyan, dim |
| Completed `✓` | green |
| Pending `○` | yellow |
| Blocked `⊘` | red |
| Priority 1-2 | red/yellow |
| Milestone | bold |
| Tree lines | dim |
| Errors | red, bold |

**Behavior:**
- Auto-detect TTY
- Respect `NO_COLOR` env
- Add `--no-color` global flag

**Refactor:** Extract `print_human()` from `main.rs` into `output.rs`.

### D3: Hierarchy UX

#### D3a: Tree View Default

Change `os task list` default output from flat to tree:

```
○ task_01... Build auth (milestone)
├─ ○ task_02... Implement login
│  └─ ✓ task_03... Add form validation
└─ ○ task_04... Implement logout
```

Add `--flat` flag for old behavior.

#### D3b: Type Filter Flags

New flags for `os task list`:
- `--milestones` / `-m` — depth=0 only
- `--tasks` / `-t` — depth=1 only
- `--subtasks` / `-s` — depth=2 only

#### D3c: Status Indicators

Visual state in tree output:
- `✓` completed (green)
- `○` pending/ready (yellow)
- `⊘` blocked (red)

#### D3d: Progress Summary

Footer after list:
```
3/7 complete | 2 blocked | 2 ready
```

## Deliverables

| # | Deliverable | Effort | Depends On |
|---|-------------|--------|------------|
| D1 | `os completions <shell>` subcommand | S | - |
| D2 | Styled output + `Printer` refactor | M | - |
| D3a | Tree view as default list | M | D2 |
| D3b | Type filter flags | S | - |
| D3c | Status indicators in tree | S | D2, D3a |
| D3d | Progress summary footer | S | D3a |

**Suggested order:** D1 → D2 → D3b → D3a → D3c → D3d

D1, D2, D3b are independent (can parallelize).

## Implementation Notes

### Files to Modify

1. `overseer/Cargo.toml` — add `clap_complete`, `owo-colors`
2. `overseer/src/main.rs` — add completions cmd, `--no-color` flag
3. `overseer/src/output.rs` (new) — `Printer` struct, color logic
4. `overseer/src/commands/task.rs` — type filter flags

### Crate Choices

- **clap_complete** — native clap integration, well-maintained
- **owo-colors** — zero-alloc, minimal, NO_COLOR support built-in

### Tree Rendering

Existing `print_tree()` in `main.rs:474-493` already uses unicode box chars. Enhance with:
- Color via `Printer`
- Status symbols
- Milestone indicator

## Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| `owo-colors` API mismatch | Low | S | Fallback to `colored` crate |
| Tree slow for large counts | Low | S | Lazy render, add `--limit` |

## Acceptance Criteria

- [ ] `os completions zsh` outputs valid zsh completion script
- [ ] `os completions bash` outputs valid bash completion script
- [ ] `os completions fish` outputs valid fish completion script
- [ ] `os task list` shows colored tree by default
- [ ] `os task list --flat` shows flat list (old behavior)
- [ ] `os task list --milestones` shows only depth-0 tasks
- [ ] `NO_COLOR=1 os task list` outputs without color
- [ ] `os --no-color task list` outputs without color
- [ ] Piped output has no color codes
- [ ] Progress summary shows at bottom of list
