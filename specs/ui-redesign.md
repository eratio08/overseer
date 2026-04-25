# Overseer UI Redesign Spec

**Created:** 2026-01-31
**Type:** Feature Plan
**Effort:** XL (5-8 days)
**Status:** Ready for implementation

---

## Problem Definition

### What are we solving?

The current Overseer UI is functional but visually generic - it lacks distinctive character and the "developer tool" personality that matches its purpose. Developers debugging agent work need:

- Quick task status comprehension at a glance
- Efficient navigation through task hierarchies
- Real-time visibility into task changes
- Power-user workflows (keyboard shortcuts)

### For whom?

Developers monitoring/debugging AI agent task execution in local dev environments.

### Cost of not solving?

- UI feels like "any React app" - forgettable
- Inefficient workflows when debugging complex task trees
- Missed opportunity to create distinctive product identity

---

## Constraints

| Constraint | Source | Impact |
|------------|--------|--------|
| Performance: 100+ tasks | User requirement | Must maintain virtualization, memoization |
| Local dev only | Deployment context | No CDN/edge optimization needed |
| Existing stack | Tech debt | React, ReactFlow, TanStack Query, Tailwind v4 |
| No new major deps | Implicit | Avoid heavy UI frameworks |
| 10-30 tasks typical | User context | Optimize for this range, scale to 100+ |
| 5s polling exists | Current implementation | Enhance, don't replace |

---

## Aesthetic Direction: Industrial/Technical

Drawing from inspiration images (`docs/inspiration/04-aesthetic-02.png`, `05-aesthetic-03.png`, `10-aesthetic-08.png`):

**Tone:** Operational command center - like monitoring nuclear reactors or air traffic control
**Typography:** Monospace-dominant with technical labels
**Color:** High contrast dark + single vibrant accent (orange)
**Interaction:** Precise, immediate feedback - no gratuitous animation

### Design Tokens

```css
@theme {
  /* Core palette (refined) */
  --color-bg-primary: oklch(0.13 0 0);      /* Near black */
  --color-bg-secondary: oklch(0.16 0 0);    /* Panel background */
  --color-surface-primary: oklch(0.18 0 0); /* Cards, inputs */
  --color-surface-secondary: oklch(0.22 0 0);

  /* Text */
  --color-text-primary: oklch(0.9 0 0);     /* High contrast */
  --color-text-muted: oklch(0.55 0 0);
  --color-text-dim: oklch(0.4 0 0);

  /* Accent */
  --color-accent: oklch(0.7 0.18 45);       /* Warm orange */
  --color-accent-muted: oklch(0.5 0.12 45);
  --color-accent-subtle: oklch(0.3 0.08 45);

  /* Status (semantic) */
  --color-status-pending: oklch(0.55 0 0);    /* Neutral gray */
  --color-status-active: oklch(0.7 0.18 45);  /* Orange - pulsing */
  --color-status-blocked: oklch(0.65 0.2 25); /* Red-orange */
  --color-status-done: oklch(0.65 0.12 145);  /* Teal-green */

  /* Borders */
  --color-border: oklch(0.28 0 0);
  --color-border-hover: oklch(0.35 0 0);
  --color-border-focus: var(--color-accent);

  /* Typography - fully monospace */
  --font-display: "JetBrains Mono", "SF Mono", ui-monospace, monospace;
  --font-body: "JetBrains Mono", "SF Mono", ui-monospace, monospace;
  --font-mono: "JetBrains Mono", "SF Mono", ui-monospace, monospace;
}
```

### Visual Patterns

1. **Technical Labels**: All caps, letter-spacing, small font
   ```
   MILESTONE    TASK    SUBTASK
   [ACTIVE]     [BLOCKED]     [PEND]     [DONE]
   ```

2. **Grid Background**: Subtle dot pattern (existing, refine)

3. **Status Indicators**:
   - Pending: Static dot
   - Active: Pulsing dot (CSS animation)
   - Blocked: Static dot + dashed border
   - Done: Checkmark or filled dot

4. **Cards**: Sharp corners, 1px borders, no shadow (flat industrial)

5. **Transitions**: Fast (150ms), no bounce/spring - mechanical feel

---

## Features

### F1: Multi-View Layout

Switch between three views via header tabs:

```
┌─────────────────────────────────────────────────────────────────┐
│ OVERSEER                [Graph] [Kanban] [List]      2s ago  ⌘? │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│                    ... view content ...                         │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│ ▸ TASK DETAIL (collapsible)                               [×]  │
│   ID: task_01JKXYZ...                                           │
│   ...                                                           │
└─────────────────────────────────────────────────────────────────┘
```

**Views:**
- **Graph**: Enhanced ReactFlow visualization (default)
- **Kanban**: 4-column board (Pending | Active | Blocked | Done)
- **List**: Compact hierarchical tree, full-width

### F2: Enhanced Graph Visualization

**Node Redesign:**
```
┌─────────────────────────────────────┐
│ MILESTONE                    P1     │
│ ● Build authentication system       │
│                                     │
│ [ACTIVE]              3/5 tasks     │
└─────────────────────────────────────┘
```

- Industrial card style with prominent status
- Progress indicator for parent tasks
- Collapse/expand in-place
- Keyboard focus ring

**Edge Improvements:**
- Parent-child: Solid, muted color
- Blocker: Dashed, accent color, animated flow direction
- Labels on hover showing relationship type

**Navigation:**
- Arrow keys traverse nodes
- Enter to select/focus detail
- Space to collapse/expand
- Minimap toggle for large graphs

### F3: Keyboard Navigation System

| Key | Context | Action |
|-----|---------|--------|
| `j` / `↓` | List/Graph | Next task |
| `k` / `↑` | List/Graph | Previous task |
| `h` / `←` | Graph | Parent / collapse |
| `l` / `→` | Graph | First child / expand |
| `Enter` | Any | Open detail panel |
| `Escape` | Detail/Edit | Close / cancel |
| `e` | Detail | Edit description |
| `c` | Detail | Complete task (if unblocked) |
| `d` | Detail | Delete task (with confirm) |
| `1` | Header | Switch to Graph view |
| `2` | Header | Switch to Kanban view |
| `3` | Header | Switch to List view |
| `g g` | Any | Jump to first task |
| `G` | Any | Jump to last task |
| `?` | Any | Show keyboard shortcuts |

### F4: Real-time Status Indicators

- **Pulsing dot**: CSS animation for active tasks
- **Last updated**: Header shows "2s ago", "1m ago", etc.
- **Change highlight**: Brief orange flash when task updates (refetch)
- **Connection status**: Indicator if API unreachable

### F5: Kanban View

```
┌──────────────┬──────────────┬──────────────┬──────────────┐
│ PENDING (5)  │ ACTIVE (2)   │ BLOCKED (1)  │ DONE (12)    │
├──────────────┼──────────────┼──────────────┼──────────────┤
│ ┌──────────┐ │ ┌──────────┐ │ ┌──────────┐ │ ┌──────────┐ │
│ │ Task A   │ │ │ ● Task D │ │ │ ⊘ Task G │ │ │ ✓ Task J │ │
│ │ P2       │ │ │ P1       │ │ │ P1       │ │ │ P3       │ │
│ └──────────┘ │ └──────────┘ │ │ blocked  │ │ └──────────┘ │
│ ┌──────────┐ │ ┌──────────┐ │ │ by: D    │ │ ...          │
│ │ Task B   │ │ │ ● Task E │ │ └──────────┘ │              │
│ └──────────┘ │ └──────────┘ │              │              │
│ ...          │              │              │              │
└──────────────┴──────────────┴──────────────┴──────────────┘
```

- Virtualized columns for performance
- Click card to select, opens detail panel
- No drag-drop (read-only, matches Graph)

---

## Architecture

### Component Library (`ui/src/client/components/ui/`)

New primitive components with variant styling:

```typescript
// Button variants
<Button variant="primary" />   // Orange accent
<Button variant="secondary" /> // Border only
<Button variant="ghost" />     // Text only
<Button variant="danger" />    // Red for delete

// Badge variants (status)
<Badge status="pending" />
<Badge status="active" />
<Badge status="blocked" />
<Badge status="done" />

// Card
<Card selected={true} />

// Other primitives
<Input />, <Textarea />, <Dialog />, <Kbd />
```

Use `tailwind-variants` for variant definitions.

### State Management

Add Zustand store for UI state:

```typescript
interface UIStore {
  // View
  viewMode: 'graph' | 'kanban' | 'list';
  setViewMode: (mode: ViewMode) => void;

  // Selection
  selectedTaskId: TaskId | null;
  setSelectedTaskId: (id: TaskId | null) => void;

  // Graph state
  collapsedNodes: Set<TaskId>;
  toggleCollapsed: (id: TaskId) => void;

  // Detail panel
  detailPanelOpen: boolean;
  toggleDetailPanel: () => void;

  // Keyboard focus
  focusedTaskId: TaskId | null;
  setFocusedTaskId: (id: TaskId | null) => void;
}
```

### API Layer

Switch from raw `fetch()` to Hono RPC client:

```typescript
// Before (queries.ts)
const res = await fetch(`/api/tasks/${id}`);

// After
import { client } from './api';
const res = await client.api.tasks[':id'].$get({ param: { id } });
```

---

## Deliverables

| # | Deliverable | Effort | Depends On | Acceptance Criteria |
|---|-------------|--------|------------|---------------------|
| D1 | Component library foundation | M | - | Button, Badge, Card, Input, Dialog, Kbd components with variants |
| D2 | Design token system | S | - | Updated global.css with OKLCH colors, working in all components |
| D3 | Multi-view layout shell | M | D1 | Header with tabs, view switching works, detail panel toggles |
| D4 | TaskList redesign | M | D1, D2 | Industrial styling, keyboard nav (j/k), focus states |
| D5 | TaskGraph redesign | L | D1, D2 | New node design, edge labels, keyboard nav, minimap |
| D6 | TaskDetail redesign | M | D1, D2 | Collapsible panel, redesigned fields, keyboard shortcuts |
| D7 | Kanban view | M | D1, D2, D3 | 4-column layout, virtualized, click-to-select |
| D8 | Keyboard navigation | M | D3, D4, D5 | All shortcuts working, help modal (?) |
| D9 | Real-time indicators | S | D4, D5, D6 | Pulsing active dot, last-updated time, change highlight |
| D10 | Polish + refinements | S | All | Consistent spacing, transitions, edge cases |

---

## Implementation Order

```
Week 1:
├── D1: Component library (2 days)
├── D2: Design tokens (0.5 days)
└── D3: Layout shell (1 day)

Week 2:
├── D4: TaskList redesign (1 day)
├── D5: TaskGraph redesign (2 days)
└── D6: TaskDetail redesign (1 day)

Week 3:
├── D7: Kanban view (1.5 days)
├── D8: Keyboard navigation (1 day)
├── D9: Real-time indicators (0.5 days)
└── D10: Polish (1 day)
```

---

## Trade-offs

| Decision | Choice | Alternative | Why |
|----------|--------|-------------|-----|
| Fully monospace | Yes | Mixed typography | Stronger industrial identity |
| Zustand for UI state | Yes | Context only | Cleaner API, devtools, persist option |
| tailwind-variants | Yes | cva | Better Tailwind integration |
| No drag-drop in Kanban | Yes | Add drag-drop | Keeps UI read-only, simpler, matches Graph |
| OKLCH colors | Yes | Hex | Better perceptual uniformity, future-proof |
| Dark mode only | Yes | Theme toggle | Industrial aesthetic is dark; defer light mode |

---

## Risks & Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Graph perf with 100+ nodes | Medium | High | Keep virtualization, profile early, test with synthetic data |
| Keyboard conflicts | Low | Medium | Use unmodified keys only in focused state, document clearly |
| Scope creep | Medium | High | Explicit scope ceiling, defer command palette to v2 |
| Design inconsistency mid-migration | Medium | Low | Complete D1-D2 first, systematic rollout |

---

## Non-Goals (Explicit)

- Command palette (⌘K) - defer to v2
- Light theme - industrial aesthetic is dark-only
- Mobile/responsive - local dev tool, desktop only
- Sound effects - nice-to-have, not core
- Drag-drop task management - read-only viewer
- Task creation in UI - CLI/MCP only per existing design

---

## Open Questions (Resolved)

| Question | Resolution |
|----------|------------|
| Sound on completion? | Deferred - not core |
| Dark/light toggle? | Dark only initially |
| Responsive design? | Out of scope - desktop only |
| Command palette? | Defer to v2 |

---

## References

- Inspiration: `docs/inspiration/01-amp-task-ui.png` (task graph layout)
- Inspiration: `docs/inspiration/02-cloudflare-workflows.png` (step history)
- Inspiration: `docs/inspiration/04-aesthetic-02.png`, `05`, `10` (industrial aesthetic)
- Skills: `frontend-design`, `vercel-react-best-practices`
- Current UI: `ui/src/client/`
