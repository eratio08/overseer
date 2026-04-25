# Task Archival & Cancellation Spec

**Status:** Ready for implementation
**Effort:** L (2-3 days, ~26 hours)
**Type:** Feature plan
**Date:** 2026-02-03
**Updated:** 2026-02-03 (added lifecycle enum formalization)

---

## Problem Definition

**What are we solving?**
Completed tasks accumulate in the active dataset, polluting queries, UI views, and progress calculations. Users need a way to:
1. Remove completed work from active views without losing data
2. Mark abandoned work as cancelled (distinct from deletion)
3. Prepare for future auto-archival (7-day age-off deferred to later iteration)

**For whom?**
- **Agents/users** working with long-lived projects where completed work becomes noise
- **UI users** who want cleaner task lists focused on active work
- **Progress tracking** that reflects current work, not historical artifacts

**Cost of not solving?**
- `os task list` returns 100s of completed tasks, obscuring active work
- UI performance degrades with large datasets
- Milestone progress calculations include tasks from months ago
- No way to mark abandoned work except deletion (loses context)

---

## Discovery Summary

**Explored areas:**
1. Task state management (db/task_repo.rs, core/task_service.rs)
2. MCP codemode API surface (host/src/api/tasks.ts)
3. UI task display and filtering (ui/src/client)
4. CLI command implementation patterns (overseer/src/commands/task.rs)

**Key findings:**
- Current state: `pending → in_progress → completed` (implicit, inferred from fields)
- No explicit state enum - state derived from `completed`, `started_at` booleans
- Queries default to showing ALL tasks (no filtering by completion state in UI)
- Progress calculations count all completed tasks (no archive concept)
- Follows "reopen" pattern for state transitions (simple CRUD, no VCS)

---

## Recommendation: Balanced Approach (Archive + Cancel, Manual)

**Chosen solution:**
- Add `cancelled` + `archived` states (both with timestamps)
- Manual cancel/archive commands (no auto-archive yet)
- Archive validation: Only completed OR cancelled tasks
- Cascade archive for milestones (block if any descendant incomplete)

**Why this approach:**
1. ✅ Solves both problems - cancelled for abandonment, archived for cleanup
2. ✅ Predictable - user controls when tasks archive (no surprises)
3. ✅ Incremental - can add auto-archive later if users want it
4. ✅ Clear semantics - cancelled ≠ completed ≠ archived (distinct states)
5. ✅ Low risk - follows existing "reopen" pattern, no time-based logic

**Deferred: Auto-archive (7-day age-off)**
- Surprising behavior (tasks vanish without user action)
- Hard-coded threshold may not fit all workflows
- Better to ship manual first, gather feedback

---

## Key Trade-offs

| Decision | Choice | Alternative Considered | Rationale |
|----------|--------|----------------------|-----------|
| **Archive trigger** | Manual only | Auto-archive after 7 days | Manual is predictable; can add auto later |
| **Cancel scope** | Incomplete tasks only | Any task | Completed tasks don't need cancellation |
| **Archive scope** | Completed OR cancelled | Completed only | Need to hide abandoned work too |
| **Unarchive** | Not supported | Allow unarchive | Permanent archival is simpler, clearer semantics |
| **Cascade behavior** | Block then cascade | Always cascade OR always block | User wants safety (block) + convenience (cascade) |
| **Blocker edges** | Preserved | Removed on archive | Preserves dependency graph for historical analysis |
| **Learnings** | Preserved | Archive with task | Knowledge persists independent of task visibility |
| **Progress calc** | Include archived | Exclude archived | Archived tasks are still completed work |

---

## State Machine

### Visual Representation

```
       create()
          ↓
    ┌──────────┐
    │ PENDING  │ (active)
    └──────────┘
       │      │
   start()  cancel() ← NEW
       │      ↓
       │  ┌─────��────┐
       │  │CANCELLED │ (inactive, not completed)
       │  └──────────┘
       │      │
       │   archive() ← NEW (only cancelled tasks)
       │      ↓
       │  ┌──────────┐
       │  │ARCHIVED  │ (hidden)
       │  └──────────┘
       ↓
┌────────────┐
│IN_PROGRESS │ (active)
└────────────┘
       │
  complete()
       ↓
┌──────────┐
│COMPLETED │ (inactive, but visible)
└──────────┘
       │
   archive() ← NEW (only completed tasks)
       ↓
┌──────────┐
│ARCHIVED  │ (hidden)
└──────────┘
```

### Formalization (Option C+: Internal Enum, Persist as Booleans)

**Problem:** With 5 states and 4 boolean/timestamp fields, `.completed` no longer means "not pending" in all contexts:
- **Hierarchy completion** uses "finished = completed OR cancelled"
- **Blocker satisfaction** uses "satisfies = completed only" (cancelled keeps dependents blocked)
- **Visibility** uses "hidden = archived"

**Solution:** Add internal lifecycle enum as single source of truth, but keep DB schema as booleans (per spec).

**Implementation:**

```rust
// overseer/src/types.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleState {
    Pending,
    InProgress,
    Completed,
    Cancelled,
    Archived,
}

impl Task {
    /// Compute lifecycle state from field values (single source of truth)
    pub fn lifecycle_state(&self) -> LifecycleState {
        // Precedence: archived > cancelled > completed > started > pending
        if self.archived {
            LifecycleState::Archived
        } else if self.cancelled {
            LifecycleState::Cancelled
        } else if self.completed {
            LifecycleState::Completed
        } else if self.started_at.is_some() {
            LifecycleState::InProgress
        } else {
            LifecycleState::Pending
        }
    }

    /// Task is active for work (not finished or archived)
    pub fn is_active_for_work(&self) -> bool {
        matches!(self.lifecycle_state(), LifecycleState::Pending | LifecycleState::InProgress)
    }

    /// Task is finished for hierarchy (completed OR cancelled, but not archived check)
    pub fn is_finished_for_hierarchy(&self) -> bool {
        self.completed || self.cancelled
    }

    /// Task satisfies blocker (only completed, not cancelled)
    pub fn satisfies_blocker(&self) -> bool {
        self.completed && !self.cancelled && !self.archived
    }

    /// Validate lifecycle invariants (call at DB hydrate in debug/tests)
    #[cfg(debug_assertions)]
    pub fn validate_lifecycle_invariants(&self) -> Result<(), String> {
        // Invalid: completed AND cancelled
        if self.completed && self.cancelled {
            return Err("Task cannot be both completed and cancelled".into());
        }
        // Invalid: archived but not finished
        if self.archived && !self.is_finished_for_hierarchy() {
            return Err("Archived task must be completed or cancelled".into());
        }
        // Invalid: state flag without timestamp
        if self.cancelled && self.cancelled_at.is_none() {
            return Err("Cancelled task must have cancelled_at timestamp".into());
        }
        if self.archived && self.archived_at.is_none() {
            return Err("Archived task must have archived_at timestamp".into());
        }
        if self.completed && self.completed_at.is_none() {
            return Err("Completed task must have completed_at timestamp".into());
        }
        Ok(())
    }
}
```

**Transition validation (centralized in service layer):**

```rust
// overseer/src/core/task_service.rs
impl TaskService {
    pub fn cancel(&self, id: &TaskId) -> Result<Task> {
        let task = self.get_task_or_err(id)?;

        // Validate transition
        match task.lifecycle_state() {
            LifecycleState::Completed => return Err(OsError::CannotCancelCompleted(id.clone())),
            LifecycleState::Cancelled => return Err(OsError::AlreadyCancelled(id.clone())),
            LifecycleState::Archived => return Err(OsError::CannotModifyArchived(id.clone())),
            LifecycleState::Pending | LifecycleState::InProgress => {
                // Valid transition
                let mut task = task_repo::cancel_task(self.conn, id)?;
                self.enrich_task(&mut task)?;
                Ok(task)
            }
        }
    }

    pub fn archive(&self, id: &TaskId) -> Result<Task> {
        let task = self.get_task_or_err(id)?;

        // Validate transition
        match task.lifecycle_state() {
            LifecycleState::Archived => return Err(OsError::AlreadyArchived(id.clone())),
            LifecycleState::Pending | LifecycleState::InProgress => {
                return Err(OsError::CannotArchiveActive(id.clone()))
            }
            LifecycleState::Completed | LifecycleState::Cancelled => {
                // Valid transition - check cascade for milestones
                if task.depth == Some(0) {
                    self.validate_milestone_archivable(id)?;
                    self.cascade_archive(id)?;
                } else {
                    let mut task = task_repo::archive_task(self.conn, id)?;
                    self.enrich_task(&mut task)?;
                }
                self.get(id)  // Return updated task with cascaded changes
            }
        }
    }
}
```

**Usage in business logic:**

```rust
// BEFORE (error-prone):
if task.completed {
    // But does this mean "done for hierarchy" or "satisfies blocker"?
}

// AFTER (explicit):
if task.is_finished_for_hierarchy() {
    // Clearly: completed OR cancelled (for pending children check)
}

if task.satisfies_blocker() {
    // Clearly: only completed (cancelled keeps dependents blocked)
}

if task.is_active_for_work() {
    // Clearly: pending OR in_progress (for next_ready)
}
```

**Validation rules:**
- **Cancel**: Only `Pending | InProgress` → else `CannotCancelCompleted` or `AlreadyCancelled`
- **Archive**: Only `Completed | Cancelled` → else `CannotArchiveActive`
- **Archived** = permanent (no unarchive operation)

**Invalid states to prevent:**
- `completed=1 && cancelled=1` (mutually exclusive)
- `archived=1 && !is_finished_for_hierarchy()` (spec violation)
- State flag without corresponding timestamp
- Workflow ops (start/complete) on cancelled/archived tasks

---

## Design Specifications

### 1. Database Schema Changes

**Add columns to `tasks` table:**
```sql
ALTER TABLE tasks ADD COLUMN cancelled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tasks ADD COLUMN cancelled_at TEXT;
ALTER TABLE tasks ADD COLUMN archived INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tasks ADD COLUMN archived_at TEXT;
```

**Invariants:**
- `cancelled=1` → `cancelled_at IS NOT NULL`
- `archived=1` → `archived_at IS NOT NULL`
- `archived=1` → `completed=1 OR cancelled=1`

---

### 2. CLI Commands

```bash
# Cancel incomplete task (marks as cancelled, not deleted)
os task cancel <task_id>

# Archive completed or cancelled task (hides from default queries)
os task archive <task_id>

# List with archived filter
os task list                     # Default: hide archived (archived=false implicit)
os task list --archived          # Show only archived
os task list --no-archived       # Explicitly hide archived
os task list --all               # Show all (including archived) - NEW FLAG
```

**Errors:**
- `CannotCancelCompleted` - Attempted to cancel completed task
- `CannotArchiveActive` - Attempted to archive active (not completed/cancelled) task
- `PendingChildren` - Attempted to archive milestone with incomplete descendants

---

### 3. Hierarchy & Cascading

**Archive cascade rule:**
1. If task is milestone (`depth=0`), check all descendants
2. If ANY descendant is `completed=0 AND cancelled=0` → `PendingChildren` error
3. If ALL descendants are `completed=1 OR cancelled=1`:
   - Archive milestone
   - Cascade archive to all descendants (depth-1 and depth-2)
4. If task is not milestone, just archive (no cascade)

**Cancel cascade:**
- No cascade - only affects single task
- Children remain unchanged

---

### 4. Blocker Edges

**Rule:** Archiving does NOT remove blocker edges

**Implementation:**
- Archived task's `completed` state still affects dependent readiness
- If archived task was completed → dependents unblocked
- If archived task was cancelled → dependents stay blocked

**Rationale:** Preserves dependency graph integrity for historical analysis.

---

### 5. Learnings

**Rule:** Learnings remain active when task archived

**Implementation:**
- Archiving does NOT cascade to learnings
- `learning.task_id` may point to archived task (allowed)
- `learning.source_task_id` may point to archived task (allowed)
- Learnings still appear in `TaskWithContext.learnings` hierarchy

**Rationale:** Knowledge should persist even when tasks are hidden.

---

### 6. Progress Calculations

**Rule:** Archived tasks count toward milestone progress

**Implementation:**
```rust
TaskProgress {
    total: usize,      // Include archived (they're still work done)
    completed: usize,  // Include archived+completed
    ready: usize,      // Exclude archived (never ready)
    blocked: usize,    // Exclude archived (never blocked)
}
```

**Query behavior:**
```rust
// Default: Hide archived
tasks.list({}) → archived=false implicit

// Show only archived
tasks.list({ archived: true })

// Show all (including archived)
tasks.list({ archived: null }) OR use --all flag
```

---

### 7. Type Changes

**Rust (`overseer/src/types.rs`):**
```rust
pub struct Task {
    // ... existing fields ...
    #[serde(default)]
    pub cancelled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancelled_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub archived: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<DateTime<Utc>>,
}
```

**TypeScript (`host/src/types.ts`):**
```typescript
interface Task {
  // ... existing fields ...
  cancelled: boolean;
  cancelledAt: string | null;
  archived: boolean;
  archivedAt: string | null;
}
```

**Filter types:**
```rust
// Rust (overseer/src/db/task_repo.rs)
pub struct ListTasksFilter {
    pub archived: Option<bool>,  // None = exclude, Some(true) = only, Some(false) = exclude
    // ... existing filters ...
}
```

```typescript
// TypeScript (host/src/types.ts)
interface TaskFilter {
  archived?: boolean;  // undefined = exclude, true = only, false = exclude
  // ... existing filters ...
}
```

---

### 8. MCP API Changes

**New methods:**
```typescript
declare const tasks: {
  cancel(id: string): Promise<Task>;   // Marks incomplete task as cancelled
  archive(id: string): Promise<Task>;  // Hides completed/cancelled task
  // ... existing methods ...
};
```

**Updated filter:**
```typescript
// Default: Hide archived
tasks.list({})

// Show only archived
tasks.list({ archived: true })

// Explicitly hide archived
tasks.list({ archived: false })
```

---

### 9. UI Changes

**Header component** (add toggle next to milestone filter):
```tsx
<Toggle
  checked={showArchived}
  onChange={setShowArchived}
  label="Show archived"
/>
```

**TaskDetail panel** (add cancel/archive buttons):
```tsx
{!task.completed && !task.cancelled && (
  <Button onClick={() => cancelTask(task.id)} variant="secondary">
    Cancel
  </Button>
)}

{(task.completed || task.cancelled) && !task.archived && (
  <Button onClick={() => archiveTask(task.id)} variant="ghost">
    Archive
  </Button>
)}
```

**Status badge** (add cancelled/archived states):
```tsx
function getStatusVariant(task: Task) {
  if (task.archived) return "ghost";      // Gray, low emphasis
  if (task.cancelled) return "secondary"; // Yellow accent
  if (task.completed) return "success";   // Green
  if (task.effectivelyBlocked) return "destructive"; // Red
  if (task.startedAt) return "default";   // Blue, pulsing
  return "outline";                       // Gray outline
}
```

**Visual treatment:**
- Archived tasks: 60% opacity + strikethrough description
- Cancelled tasks: 80% opacity + yellow/orange accent
- Both: Show timestamp in task detail panel

---

## Implementation Deliverables (Ordered)

### [D1] Database schema migration (S - 2 hours)
**Files:** `overseer/src/db/migrations/`
- Add 4 columns: `cancelled`, `cancelled_at`, `archived`, `archived_at`
- Test migration on copy of existing DB
- **Depends on:** -

### [D2] Rust type updates + lifecycle enum (M - 2 hours)
**Files:** `overseer/src/types.rs`
- Update `Task` struct with 4 new fields
- Add `LifecycleState` enum (Pending/InProgress/Completed/Cancelled/Archived)
- Add `Task::lifecycle_state()` method (computed from fields)
- Add predicates: `is_active_for_work()`, `is_finished_for_hierarchy()`, `satisfies_blocker()`
- Add `validate_lifecycle_invariants()` (debug assertions)
- Update `serde` annotations with `skip_serializing_if`
- **Depends on:** D1

### [D3] Database layer (M - 3 hours)
**Files:** `overseer/src/db/task_repo.rs`
- `cancel_task(conn, id) -> Result<Task>`
- `archive_task(conn, id) -> Result<Task>`
- Update `list_tasks()` to accept `archived: Option<bool>` filter
- Add SQL: `WHERE archived = 0` (default), `WHERE archived = 1` (only), no filter (all)
- **Depends on:** D1, D2

### [D4] Service layer validation (M - 4 hours)
**Files:** `overseer/src/core/task_service.rs`, `overseer/src/error.rs`
- `cancel(id)` - use `lifecycle_state()` to validate transition, call `task_repo::cancel_task()`
- `archive(id)` - validate via `lifecycle_state()`, call `task_repo::archive_task()`
- Archive cascade for milestones: check descendants via `is_finished_for_hierarchy()`, cascade if all finished
- Update existing methods: Replace `.completed` checks with predicates where appropriate
- Add error variants: `CannotCancelCompleted`, `AlreadyCancelled`, `CannotArchiveActive`, `AlreadyArchived`, `CannotModifyArchived`
- **Depends on:** D3

### [D5] CLI commands (M - 3 hours)
**Files:** `overseer/src/commands/task.rs`, `overseer/src/main.rs`
- Add `TaskCommand::Cancel { id }`, `TaskCommand::Archive { id }`
- Update `TaskCommand::List` with `--archived`, `--no-archived`, `--all` flags
- Wire into `task::handle()` (no VCS required)
- Update `output.rs` to print cancelled/archived states
- **Depends on:** D4

### [D6] TypeScript type sync (S - 1 hour)
**Files:** `host/src/types.ts`, `host/src/decoder.ts`
- Update `Task` interface with 4 new fields
- Update `TaskFilter` interface with `archived?: boolean`
- Update decoders to validate new fields
- **Depends on:** D2

### [D7] MCP API (S - 1 hour)
**Files:** `host/src/api/tasks.ts`
- Add `cancel(id: string): Promise<Task>`
- Add `archive(id: string): Promise<Task>`
- Update `list(filter)` to pass `archived` param to CLI
- **Depends on:** D6

### [D8] UI queries (S - 1 hour)
**Files:** `ui/src/client/lib/queries.ts`
- Update `useTasks` hook to accept `archived` filter
- Add `useCancelTask` mutation hook
- Add `useArchiveTask` mutation hook
- **Depends on:** D7

### [D9] UI components (M - 4 hours)
**Files:** `ui/src/client/components/`
- Header: "Show archived" toggle (next to milestone filter)
- TaskDetail: Cancel/Archive buttons (conditional rendering)
- TaskList/KanbanView: Update status badges with cancelled/archived variants
- Visual treatment: Add opacity + strikethrough CSS for archived tasks
- **Depends on:** D8

### [D10] Documentation (S - 1 hour)
**Files:** `docs/CLI.md`, `docs/MCP.md`, `ui/README.md`
- Update CLI reference with `cancel`, `archive` commands
- Update MCP API reference with new methods
- Document UI toggle and filter behavior
- **Depends on:** D9

### [D11] Tests (M - 4 hours)
**Files:** `overseer/tests/`, `host/tests/`, `ui/tests/`
- Unit tests: `cancel()`, `archive()` validation rules + lifecycle state transitions
- Unit tests: Invalid state combinations (completed && cancelled, archived without finished, etc.)
- Unit tests: Predicate correctness (is_active_for_work, is_finished_for_hierarchy, satisfies_blocker)
- Integration tests: Cascade archive behavior (milestone → children)
- Integration tests: Blocker satisfaction (cancelled doesn't unblock, completed does)
- UI tests: Toggle archived, cancel/archive buttons (agent-browser)
- **Depends on:** D10

---

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|-----------|
| **Schema migration breaks existing DBs** | High | Test migration on copy of production DB first, document rollback script |
| **Type sync breaks (Rust ↔ TS)** | High | Run both CLI and MCP tests in CI, validate JSON shape in decoders |
| **Cascade logic misses edge cases** | Medium | Write comprehensive tests for 3-level hierarchies (milestone→task→subtask) |
| **UI performance degrades with large datasets** | Low | SQLite query with `archived=0` is indexed, minimal perf impact |
| **Users accidentally cascade-archive large milestones** | Medium | Add confirmation prompt in UI for cascade archives (>10 descendants) |
| **Archived tasks break existing queries** | Medium | Default to `archived=false` in all list operations (backward compatible) |
| **Predicate refactoring introduces regressions** | Medium | Systematic replacement of `.completed` with predicates, comprehensive test matrix for each predicate |
| **Invalid state combinations slip through** | Medium | Add `validate_lifecycle_invariants()` in debug builds, test invalid combos explicitly |

---

## Future Enhancements (Out of Scope)

1. **Auto-archive** - After 7 days, mark completed tasks as archived (lazy eval on list)
2. **Unarchive** - Allow restoring archived tasks to active state
3. **Archive audit log** - Track who archived what and when
4. **Per-milestone archive settings** - Custom age-off periods per milestone
5. **Bulk archive** - Archive all completed tasks in milestone at once

---

## Success Criteria

**Implementation complete when:**
1. ✅ `LifecycleState` enum implemented with predicates (`is_active_for_work`, `is_finished_for_hierarchy`, `satisfies_blocker`)
2. ✅ `os task cancel <id>` marks incomplete task as cancelled (validates via `lifecycle_state()`)
3. ✅ `os task archive <id>` hides completed/cancelled task from default queries
4. ✅ `os task list` excludes archived by default, `--archived` shows only archived
5. ✅ Archiving milestone with incomplete children fails with `PendingChildren`
6. ✅ Archiving milestone with all children complete cascades to descendants
7. ✅ Blocker satisfaction: completed tasks unblock dependents, cancelled tasks keep them blocked
8. ✅ UI "Show archived" toggle filters task list
9. ✅ UI shows Cancel button for incomplete tasks, Archive button for completed/cancelled
10. ✅ Progress calculations include archived tasks in totals
11. ✅ Invalid state combinations prevented (completed && cancelled, archived without finished, etc.)
12. ✅ All tests pass (unit + integration + UI + predicate correctness)
13. ✅ Documentation updated (CLI, MCP, UI)

---

## Estimation Summary

**Total effort:** L (2-3 days, ~26 hours)

| Deliverable | Effort | Hours |
|-------------|--------|-------|
| D1 (Schema) | S | 2 |
| D2 (Types + enum) | M | 2 |
| D3 (DB layer) | M | 3 |
| D4 (Service + refactor) | M | 4 |
| D5 (CLI) | M | 3 |
| D6-D7 (MCP API) | S | 2 |
| D8-D9 (UI) | M | 5 |
| D10 (Docs) | S | 1 |
| D11 (Tests) | M | 4 |
| **Total** | **L** | **~26 hours** |

*Note: Initial estimate of 16 hours updated to 26 hours accounting for lifecycle enum formalization, predicate refactoring, and comprehensive state transition testing.*

---

## Related Documents

- `docs/ARCHITECTURE.md` - System design overview
- `docs/CLI.md` - Command reference (update with cancel/archive)
- `docs/MCP.md` - MCP API reference (update with new methods)
- `ui/docs/UI-TESTING.md` - UI testing with agent-browser

---

**Spec approved:** 2026-02-03
**Ready for implementation**
