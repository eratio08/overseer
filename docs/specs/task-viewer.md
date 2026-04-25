# Overseer Task Viewer — Feature Spec

**Status:** In Progress
**Effort:** M-L (2-3 days remaining)
**Created:** 2026-01-28
**Updated:** 2026-01-31

---

## Implementation Status

| Component | Status | Notes |
|-----------|--------|-------|
| CLI args (`--port`, `--no-open`) | Done | `overseer/src/commands/ui.rs` |
| CLI spawn/detection | Done | PORT env var, "listening on" detection |
| API skeleton | Done | `/health` only |
| API routes | Not started | — |
| CLI bridge (`cli.ts`) | Not started | — |
| Frontend | Not started | — |

**Completed fixes:**
- ✅ npm script: `dev:api` → `dev`
- ✅ Ready detection: CLI detects "listening on" (Hono output)
- ✅ Port: API defaults to 6969

---

## Problem Statement

Agents using Overseer via MCP have no visual way to inspect the task store state. Debugging task hierarchies, blocked dependencies, and context inheritance requires raw CLI commands. A local webapp would provide:

- At-a-glance understanding of milestone/task/subtask relationships
- Visual dependency graph (blockedBy edges)
- Quick edits without CLI syntax

## Constraints

| Constraint | Value |
|------------|-------|
| Deployment | Local-only, no auth |
| Port | :6969 (default) |
| VCS operations | Through existing workflow service (complete only) |
| Browser support | Modern (Chrome/Firefox/Safari) |
| Data freshness | Polling @ 5 seconds |
| Static file serving | Filesystem (`ui/dist/`) |

## Non-Goals (v1)

- Create new tasks (use CLI/MCP)
- Change parent relationships
- Add/remove blockers
- Start tasks
- Multi-datastore support
- Remote/hosted deployment

---

## Tech Stack

| Layer | Choice | Rationale |
|-------|--------|-----------|
| CLI command | Rust (`os ui`) | Consistent with existing CLI |
| API server | Hono | Type-safe RPC client via `hc<AppType>()` |
| Frontend build | Vite | Fast HMR, simpler than Astro for SPA |
| UI components | React + shadcn/ui | Custom nodes, dark theme |
| Graph visualization | @xyflow/react + dagre | Interactive, custom nodes, auto-layout |
| Data fetching | TanStack Query | Stable, simple polling |
| Styling | Tailwind CSS v4 | CSS-first config, OKLCH colors |

**Architecture Decision:** Single-package Hono + Vite SPA (simplified from original Astro spec). Astro's partial hydration benefits don't justify complexity for a local-only dashboard with heavy React interactivity.

---

## Architecture

```
+-------------------------------------------------------------+
|                    os ui (Rust CLI)                         |
|  - Spawns Hono server on :6969                              |
|  - Opens browser                                            |
|  - Waits for "listening on" in stdout                       |
+-------------------------------------------------------------+
                              |
                              v
+-------------------------------------------------------------+
|                    Hono Server (:6969)                      |
|                                                             |
|  +-------------------+  +--------------------------------+  |
|  | Static Files      |  | API Routes (/api/*)            |  |
|  | (Vite dist/)      |  |                                |  |
|  |                   |  | GET  /api/tasks                |  |
|  | - index.html      |  | GET  /api/tasks/:id            |  |
|  | - assets/*.js     |  | PUT  /api/tasks/:id            |  |
|  | - assets/*.css    |  | DEL  /api/tasks/:id            |  |
|  +-------------------+  | POST /api/tasks/:id/complete   |  |
|                         | CRUD /api/learnings            |  |
|                         +--------------------------------+  |
|                                        |                    |
+----------------------------------------|--------------------+
                                         v
                               +---------------------+
                               |   os CLI (spawn)    |
                               |   --json mode       |
                               +---------------------+
```

---

## Package Structure

```
overseer/
├── overseer/                    # Existing Rust CLI
│   └── src/commands/ui.rs       # DONE: os ui command
│
└── ui/                          # Single package (Hono + Vite)
    ├── package.json             # DONE: @overseer/ui
    ├── tsconfig.json            # DONE
    ├── vite.config.ts           # TODO
    └── src/
        ├── api/                 # Hono API server
        │   ├── app.ts           # DONE: route definitions, AppType export
        │   ├── index.ts         # DONE: serve() entry
        │   ├── cli.ts           # TODO: spawn os --json
        │   └── routes/          # TODO
        │       ├── tasks.ts
        │       └── learnings.ts
        │
        ├── client/              # React SPA (TODO)
        │   ├── main.tsx         # React root
        │   ├── App.tsx          # Layout + routing
        │   ├── components/
        │   │   ├── TaskList.tsx
        │   │   ├── TaskGraph.tsx
        │   │   └── TaskDetail.tsx
        │   ├── lib/
        │   │   ├── api.ts       # hc<AppType> client
        │   │   └── queries.ts   # TanStack Query hooks
        │   └── styles/
        │       └── global.css
        │
        ├── client.ts            # DONE: hc<AppType> factory
        └── types.ts             # TODO: shared types
```

---

## API Routes

### Tasks

```
GET    /api/tasks              # List all (supports ?ready=true, ?completed=false, ?parentId=X)
GET    /api/tasks/:id          # Get with context chain + learnings
PUT    /api/tasks/:id          # Update description, context, priority
DELETE /api/tasks/:id          # Delete task
POST   /api/tasks/:id/complete # Complete task (triggers VCS)
```

### Learnings

```
GET    /api/tasks/:id/learnings     # List learnings for task
POST   /api/tasks/:id/learnings     # Add learning
PUT    /api/learnings/:id           # Update learning
DELETE /api/learnings/:id           # Delete learning
```

### Meta

```
GET    /api/health             # Health check
```

---

## API Types

```typescript
// === Shared Types (ui/src/types.ts) ===

interface Task {
  id: string                    // "task_01ABC..."
  parentId: string | null
  description: string
  context: TaskContext
  learnings: InheritedLearnings
  result: string | null
  priority: number              // 1-5, lower = higher
  completed: boolean
  completedAt: string | null    // ISO 8601
  startedAt: string | null
  createdAt: string
  updatedAt: string
  commitSha: string | null
  bookmark: string | null
  startCommit: string | null
  depth: number                 // 0=milestone, 1=task, 2=subtask
  blockedBy: string[]           // Task IDs
  blocks: string[]              // Task IDs
}

interface TaskContext {
  own: string
  parent: string | null         // Only for depth 2
  milestone: string | null      // For depth 1,2
}

interface InheritedLearnings {
  milestone: Learning[]
  parent: Learning[]
}

interface Learning {
  id: string                    // "lrn_01ABC..."
  taskId: string
  content: string
  sourceTaskId: string | null
  createdAt: string
}

// === Request/Response Types ===

// GET /api/tasks?ready=bool&completed=bool&parentId=string
type ListTasksResponse = Task[]

// GET /api/tasks/:id
type GetTaskResponse = Task

// PUT /api/tasks/:id
interface UpdateTaskRequest {
  description?: string
  context?: string
  priority?: number
}
type UpdateTaskResponse = Task

// DELETE /api/tasks/:id
type DeleteTaskResponse = { deleted: true }

// POST /api/tasks/:id/complete
interface CompleteTaskRequest {
  result?: string
}
type CompleteTaskResponse = Task

// POST /api/tasks/:id/learnings
interface CreateLearningRequest {
  content: string
}
type CreateLearningResponse = Learning

// PUT /api/learnings/:id
interface UpdateLearningRequest {
  content: string
}
type UpdateLearningResponse = Learning

// DELETE /api/learnings/:id
type DeleteLearningResponse = { deleted: true }
```

---

## Deliverables

### D1: CLI `os ui` Command

**Effort:** M
**Files:** `overseer/src/commands/ui.rs`, `overseer/src/main.rs`
**Status:** 70% - args/spawn done, ready detection broken

**Behavior:**
```bash
os ui              # Start server on :6969, open browser
os ui --no-open    # Start server only
os ui --port 8080  # Custom port
```

**Implementation:**
1. Spawn Hono server via `npm run dev` with `PORT` env var
2. Wait for stdout containing `listening on` or `http://localhost:<port>`
3. Open browser via `open` crate
4. Block until Ctrl+C

**Fixes needed:**
- `ui.rs:82`: Pass port as `PORT=<port>` env var, not `--port` arg
- `ui.rs:107`: Change detection from `"Local:"` to `"listening on"` or `"http://localhost"`
- `ui/package.json`: Rename `dev:api` to `dev`

**Acceptance:**
- [x] CLI args: `--port`, `--no-open`
- [x] `find_ui_dir()` locates package
- [x] Browser opens via `open::that()`
- [ ] npm script `dev` exists
- [ ] PORT env var passed correctly
- [ ] Server ready detection works
- [ ] Graceful shutdown on SIGINT

---

### D2: Hono API Server

**Effort:** M
**Files:** `ui/src/api/`
**Status:** 10% - skeleton only

**Implementation:**
- Each route spawns `os --json <command>` and returns parsed JSON
- Type-safe with Zod validators
- Export `AppType` for client generation
- Serve Vite dist/ as static files in production

**Existing:**
- `app.ts`: Hono app with `/health` route, exports `AppType`
- `index.ts`: `serve()` on `PORT` env (default 3001 -> **change to 6969**)
- `../client.ts`: `hc<AppType>()` factory

**TODO:**
- `cli.ts`: Spawn `os --json` helper
- `routes/tasks.ts`: GET/PUT/DELETE `/api/tasks/*`
- `routes/learnings.ts`: CRUD `/api/learnings/*`
- Static file serving for production

**Acceptance:**
- [x] Hono app exports `AppType`
- [x] `hc<AppType>()` client factory exists
- [ ] Default port is 6969
- [ ] All routes return proper JSON
- [ ] Errors return `{ error: string }` with appropriate status codes
- [ ] CLI spawn helper works

---

### D3: Vite + React Frontend Shell

**Effort:** M
**Files:** `ui/src/client/`
**Status:** Not started

**Structure:**
```
ui/src/client/
├── main.tsx           # React root, QueryClientProvider
├── App.tsx            # Layout: sidebar + graph + detail
├── components/
│   ├── TaskList.tsx   # Sidebar list
│   ├── TaskGraph.tsx  # xyflow graph
│   ├── TaskDetail.tsx # Detail panel
│   └── ...
├── lib/
│   ├── api.ts         # hc<AppType> client
│   └── queries.ts     # TanStack Query hooks
└── styles/
    └── global.css     # Tailwind + theme
```

**Vite Config:**
- Dev: Proxy `/api/*` to Hono server
- Build: Output to `dist/` for Hono to serve

**Acceptance:**
- [ ] `vite.config.ts` exists with proxy config
- [ ] `npm run dev` starts both Vite + Hono
- [ ] Builds to static files (`dist/`)
- [ ] Dark theme matching inspiration images
- [ ] Responsive (but desktop-first)

---

### D4: Task List View

**Effort:** M
**Component:** `TaskList.tsx`

**Features:**
- Sidebar showing all tasks grouped by milestone
- Status indicators (completed, in-progress, blocked)
- Priority badges
- Click to select -> shows in detail panel
- Filter: completed/active/blocked/ready

**Acceptance:**
- [ ] Shows all tasks hierarchically
- [ ] Clicking task updates detail panel
- [ ] Filters work correctly
- [ ] Blocked tasks visually distinct

---

### D5: Dependency Graph View

**Effort:** L
**Component:** `TaskGraph.tsx`

**Features:**
- Full-screen graph with pan/zoom
- Nodes = tasks (custom component with title, status, priority)
- Edges = parent->child (solid) + blockedBy (dashed orange)
- Dagre auto-layout (top-down for hierarchy)
- Click node -> select in sidebar + detail panel
- Minimap for navigation

**Node Design:**
```
+-----------------------------+
| #75  [checkmark] Completed  |  <- status badge
+-----------------------------+
| Create SagaRunner base      |  <- description (truncated)
| class and saga schemas      |
+-----------------------------+
| P3  |  2 children           |  <- priority + child count
+-----------------------------+
```

**Acceptance:**
- [ ] Graph renders all tasks with correct edges
- [ ] Parent->child edges solid, blockedBy edges dashed
- [ ] Auto-layout produces readable hierarchy
- [ ] Click node selects it
- [ ] Minimap shows overview

---

### D6: Task Detail Panel

**Effort:** M
**Component:** `TaskDetail.tsx`

**Features:**
- Shows selected task full details
- Editable fields: description, context, priority
- Read-only: id, parent, depth, blockedBy, blocks, timestamps
- Learnings list with add/edit/delete
- Complete button (with confirmation)
- Delete button (with confirmation)

**Acceptance:**
- [ ] All task fields displayed
- [ ] Edit saves via API
- [ ] Complete triggers VCS operation
- [ ] Delete removes task
- [ ] Learnings CRUD works

---

### D7: Visual Theme

**Effort:** M
**Files:** `ui/src/client/styles/`, `ui/tailwind.config.ts`

**Aesthetic:**
- **Background:** Deep charcoal (`#0a0a0a` - `#1a1a1a`)
- **Surface:** Slightly lighter (`#1f1f1f` - `#2a2a2a`)
- **Text:** Warm white (`#e5e5e5`), muted (`#737373`)
- **Accent:** Orange (`#f97316`) for CTAs, active states
- **Success:** Muted green for completed
- **Error:** Muted red for blocked/failed
- **Typography:**
  - Display: JetBrains Mono or similar monospace
  - Body: System sans or Inter
- **Borders:** Subtle (`#333333`)
- **Shadows:** Minimal, dark

**Acceptance:**
- [ ] Matches inspiration mood board
- [ ] No purple gradients, no generic AI look
- [ ] Consistent spacing scale
- [ ] All components themed

---

## Dependency Order

```
Phase 1: Fix CLI/API sync
  D1 fixes (npm script, port, detection)
  D2 port fix (default 6969)
       |
       v
Phase 2: API routes
  D2 (CLI bridge + routes)
       |
       v
Phase 3: Frontend
  D7 (Theme) --> D3 (Shell) --> D4 (List) --+
                                            +--> D6 (Panel)
                    D5 (Graph) -------------+
       |
       v
Integration test
```

**Critical path:** D1/D2 fixes -> D2 routes -> D3 shell -> D5 graph

---

## Risks & Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| xyflow + dagre layout issues with deep hierarchies | Medium | Medium | Test with 50+ tasks early; fallback to manual positioning |
| CLI spawn latency feels slow | Low | Medium | Add loading states; consider caching |
| Theme inconsistency across components | Low | Low | Design tokens in Tailwind config; review pass at end |

---

## Visual References

See `docs/inspiration/` for mood board:

| Image | Reference |
|-------|-----------|
| 01-amp-task-ui.png | Task list sidebar, dependency graph, detail panel layout |
| 02-cloudflare-workflows.png | Node-based workflow diagram, step history |
| 03-10-aesthetic-*.png | Industrial brutalism, monospace typography, orange accent, data-dense displays |

---

## Discovery Notes

### Hono RPC Client

Hono provides type-safe client generation via `hc<AppType>()`:
- No codegen step, pure TypeScript inference
- Export `type AppType = typeof app` from server
- Client gets full autocomplete for routes, params, responses
- Use `InferResponseType` / `InferRequestType` for extracting types

**Status:** Implemented in `ui/src/client.ts`

### TanStack Query vs TanStack DB

**Decision:** Use TanStack Query (not TanStack DB)

- TanStack DB is beta, overkill for simple REST polling
- TanStack Query v5 is stable, well-documented
- Simple `refetchInterval: 5000` for polling
- Can add optimistic updates later if needed

### React Flow (@xyflow/react)

**Decision:** Use @xyflow/react v12 (not beautiful-mermaid)

- beautiful-mermaid renders static SVG, no click handlers
- React Flow has built-in node click/hover/drag
- Custom node components for task cards
- Dagre integration well-documented
- Used by Supabase, Turborepo, Microsoft AutoGen

### Vite + Hono Architecture (Updated 2026-01-31)

**Decision:** Single-package Vite SPA + Hono API (simplified from Astro)

**Rationale:**
- Astro's partial hydration doesn't help when entire page is React
- Single package simpler than workspace with 3 packages
- Local-only app doesn't need SSR/SSG benefits

**Dev mode:**
- `npm run dev` starts Hono on :6969 + Vite HMR
- Vite proxies `/api/*` to Hono

**Production:**
- Vite builds to `dist/`
- Hono serves `dist/` as static + API routes
