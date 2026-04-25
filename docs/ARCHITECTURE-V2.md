# Overseer v2 Architecture

**Status:** Draft
**Date:** 2026-02-04

## Vision

Overseer evolves from a task management CLI into a **local code review and agent orchestration platform**. Think: GitHub PR reviews + Linear task management + agent harness broker, all local-first.

## Design Principles

1. **Stripe SDK-style API** - `overseer.tasks.create()`, not heavy DDD abstractions
2. **Single Rust binary** - Boa JS engine for codemode, no Node.js dependency
3. **Multi-repo/multi-project** - Manage tasks across repositories
4. **Stacked diffs** - Each task has persistent VCS artifacts for review
5. **Event-driven** - Internal pub/sub for decoupling + future plugins
6. **Interface-agnostic core** - CLI, MCP, REST, UI all use same SDK

---

## System Architecture

### Single Binary, Multiple Modes

Overseer ships as **one Rust binary** (`os`) with multiple operational modes. No separate daemon binary needed.

```
os <command>      # Fast CLI, works standalone (direct SDK calls)
os serve          # Starts server (foreground, daemon-capable)
os mcp            # MCP server mode (Boa JS executor)
```

### Why Not a Separate Daemon?

| Separate `overseerd` | Single binary with modes |
|----------------------|--------------------------|
| Version skew risk | Always in sync |
| Extra packaging | One artifact |
| Process management complexity | `os serve` is just a mode |
| User must install service | Works immediately |

### Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                    overseers (single binary)                    │
│                                                                 │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                      Core SDK (lib)                       │  │
│  │  overseer::tasks::*  overseer::reviews::*  overseer::*    │  │
│  └───────────────────────────────────────────────────────────┘  │
│           │                    │                    │           │
│           ▼                    ▼                    ▼           │
│  ┌─────────────┐      ┌─────────────┐      ┌─────────────┐      │
│  │  CLI Mode   │      │ Serve Mode  │      │  MCP Mode   │      │
│  │   (clap)    │      │   (axum)    │      │   (Boa)     │      │
│  │             │      │             │      │             │      │
│  │ os task ... │      │ os serve    │      │ os mcp      │      │
│  └─────────────┘      └──────┬──────┘      └─────────────┘      │
│                              │                                  │
│                    ┌─────────┼─────────┐                        │
│                    ▼         ▼         ▼                        │
│              ┌─────────┐ ┌───────┐ ┌───────┐                    │
│              │REST API │ │ SSE/  │ │ Relay │                    │
│              │ /tasks  │ │ WS    │ │  WS   │                    │
│              └─────────┘ └───────┘ └───────┘                    │
│                    │         │         │                        │
│  Clients:    Web UI ◄────────┘         └────► Agent Harnesses   │
│              Tauri                            (OpenCode, etc)   │
│              TUI                                                │
└─────────────────────────────────────────────────────────────────┘
```

### Client Connectivity

| Client | Connection Strategy |
|--------|---------------------|
| **Web UI** | HTTP + SSE/WS to `os serve` |
| **Tauri desktop** | Sidecar `os serve`, talk HTTP/WS |
| **TUI** | HTTP/WS to `os serve` (or standalone CLI) |
| **MCP/codemode** | In-process Boa inside `os serve` or `os mcp` |

### SQLite Concurrency Strategy

- **WAL mode** enabled for concurrent reads
- **busy_timeout** set for write contention
- CLI works standalone (direct SQLite access)
- When `os serve` running, CLI can optionally route mutations through server
- Server as single writer → no contention, events always captured in-memory pub/sub

### Serve Mode Features

`os serve` provides:
- **REST API** (axum) - Task CRUD, reviews, learnings
- **SSE/WebSocket** - Real-time event streaming to UI
- **Relay WebSocket** - Agent harness broker
- **Boa executor** - MCP codemode endpoint

### CLI Behavior

- **Default:** `os task ...` runs in-process via Core SDK (fast, no daemon needed)
- **Optional:** If `os serve` running, CLI can route through server for consistency
- **Required:** `os events tail --follow` needs `os serve` (or starts temporary server)

### Daemonization Strategy

**Phase 1 (now):** `os serve` runs foreground, user manages with tmux/screen/&

**Phase 2 (later, if needed):** Add `os install-service` for:
- macOS: launchd user agent
- Linux: systemd user service
- Only if users demand always-on behavior

---

## Core SDK Design (Stripe-style)

### Module Structure

```rust
// overseer/src/lib.rs
pub mod tasks;      // Task CRUD + lifecycle
pub mod learnings;  // Learning management
pub mod reviews;    // Code review workflow
pub mod repos;      // Multi-repo management
pub mod harnesses;  // Agent harness broker
pub mod events;     // Event bus + subscriptions
pub mod vcs;        // VCS operations (internal)
pub mod db;         // Persistence (internal)
```

### API Surface

```rust
// Tasks
overseer::tasks::create(CreateTaskInput) -> Result<Task>
overseer::tasks::get(TaskId) -> Result<TaskWithContext>
overseer::tasks::list(TaskFilter) -> Result<Vec<Task>>
overseer::tasks::update(TaskId, UpdateTaskInput) -> Result<Task>
overseer::tasks::start(TaskId) -> Result<Task>
overseer::tasks::submit_for_review(TaskId, SubmitOptions) -> Result<Task>
overseer::tasks::delete(TaskId) -> Result<()>
overseer::tasks::block(TaskId, BlockerId) -> Result<()>
overseer::tasks::unblock(TaskId, BlockerId) -> Result<()>
overseer::tasks::next_ready(Option<MilestoneId>) -> Result<Option<TaskWithContext>>
overseer::tasks::tree(Option<RootId>) -> Result<TaskTree>
overseer::tasks::progress(Option<RootId>) -> Result<TaskProgress>

// Reviews
overseer::reviews::get(TaskId) -> Result<Review>
overseer::reviews::diff(TaskId) -> Result<Diff>
overseer::reviews::comment(TaskId, CommentInput) -> Result<Comment>
overseer::reviews::approve(TaskId, ApproveOptions) -> Result<Task>
overseer::reviews::reject(TaskId, RejectOptions) -> Result<Task>
overseer::reviews::request_changes(TaskId, Vec<Comment>) -> Result<Review>

// Learnings
overseer::learnings::list(TaskId) -> Result<Vec<Learning>>
overseer::learnings::add(TaskId, content: String) -> Result<Learning>

// Repos
overseer::repos::register(RepoPath) -> Result<Repo>
overseer::repos::list() -> Result<Vec<Repo>>
overseer::repos::get(RepoId) -> Result<Repo>
overseer::repos::set_active(RepoId) -> Result<()>

// Harnesses
overseer::harnesses::connect(HarnessConfig) -> Result<HarnessConnection>
overseer::harnesses::list() -> Result<Vec<Harness>>
overseer::harnesses::invoke(HarnessId, TaskId) -> Result<HarnessSession>
overseer::harnesses::abort(SessionId) -> Result<()>

// Events
overseer::events::subscribe(EventFilter) -> EventStream
overseer::events::list(EventQuery) -> Result<Vec<Event>>
overseer::events::replay(AfterSeq, Limit) -> Result<Vec<Event>>
```

### Internal Structure

```rust
// Each module has a struct that holds dependencies
pub struct Tasks {
    db: Arc<Database>,
    vcs: Arc<VcsManager>,
    events: Arc<EventBus>,
}

impl Tasks {
    pub fn create(&self, input: CreateTaskInput) -> Result<Task> {
        // Validation
        self.validate_create(&input)?;

        // Persist
        let task = self.db.tasks().insert(&input)?;

        // Emit event
        self.events.emit(TaskEvent::Created { task: task.clone() });

        Ok(task)
    }
}

// Top-level client that holds all modules
pub struct Overseer {
    pub tasks: Tasks,
    pub reviews: Reviews,
    pub learnings: Learnings,
    pub repos: Repos,
    pub harnesses: Harnesses,
    pub events: Events,
}

impl Overseer {
    pub fn new(config: Config) -> Result<Self> { /* ... */ }
}
```

---

## Multi-Repo/Project Model

### Schema

```sql
-- Registered repositories
CREATE TABLE repos (
  id TEXT PRIMARY KEY,              -- repo_01ABC...
  path TEXT NOT NULL UNIQUE,        -- /Users/me/Code/project
  name TEXT NOT NULL,               -- project (derived from path)
  vcs_type TEXT NOT NULL,           -- 'jj' | 'git'
  is_active INTEGER DEFAULT 0,      -- Current working repo
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

-- Tasks now belong to a repo
ALTER TABLE tasks ADD COLUMN repo_id TEXT REFERENCES repos(id);

-- VCS artifacts per task
CREATE TABLE task_vcs (
  task_id TEXT PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
  repo_id TEXT NOT NULL REFERENCES repos(id),
  ref_name TEXT NOT NULL,           -- bookmark/branch name
  base_rev TEXT NOT NULL,           -- Diff base (parent's head_rev for stacking)
  head_rev TEXT,                    -- Latest submitted revision
  worktree_path TEXT,               -- If using separate worktree
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
```

### Stacked Diffs Model

```
Milestone (task_01ABC)
├── base_rev: main@abc123
├── head_rev: abc456
│
├── Task A (task_01DEF)
│   ├── base_rev: abc456 (milestone's head)
│   ├── head_rev: def789
│   │
│   ├── Subtask A1 (task_01GHI)
│   │   ├── base_rev: def789 (parent's head)
│   │   └── head_rev: ghi012
│   │
│   └── Subtask A2 (task_01JKL)
│       ├── base_rev: def789 (parent's head)
│       └── head_rev: jkl345
│
└── Task B (task_01MNO)
    ├── base_rev: abc456 (milestone's head)
    └── head_rev: mno678
```

**Key insight:** Each task's `base_rev` is its parent's `head_rev`. This creates a true stack where:
- Diff for Task A1 = `def789..ghi012` (shows only A1's changes)
- Diff for Task A = `abc456..def789` (shows only A's changes, not subtasks)

### Worktree/Workspace Strategy

To avoid impacting the main working directory during reviews:

```rust
// On task start (if configured)
overseer::tasks::start(task_id) {
    let task_vcs = self.db.task_vcs().get(task_id)?;

    // Create isolated worktree for this task
    let worktree_path = match self.vcs.backend() {
        VcsType::Jj => {
            // jj workspace add --name task_01ABC /tmp/overseer/task_01ABC
            self.vcs.jj().create_workspace(&task_vcs.ref_name)?
        }
        VcsType::Git => {
            // git worktree add /tmp/overseer/task_01ABC task_01ABC
            self.vcs.git().create_worktree(&task_vcs.ref_name)?
        }
    };

    self.db.task_vcs().set_worktree_path(task_id, &worktree_path)?;
}
```

---

## Review Model (GitHub PR-style)

### Schema

```sql
-- Review sessions
CREATE TABLE reviews (
  id TEXT PRIMARY KEY,              -- rev_01ABC...
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  status TEXT NOT NULL,             -- 'pending' | 'approved' | 'changes_requested' | 'rejected'
  submitted_at TEXT,
  decided_at TEXT,
  reviewer TEXT,                    -- Optional reviewer identity
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

-- Comments on diffs
CREATE TABLE review_comments (
  id TEXT PRIMARY KEY,              -- cmt_01ABC...
  review_id TEXT NOT NULL REFERENCES reviews(id) ON DELETE CASCADE,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  file_path TEXT NOT NULL,          -- src/main.rs
  line_start INTEGER,               -- Starting line (nullable for file-level)
  line_end INTEGER,                 -- Ending line
  side TEXT,                        -- 'left' | 'right' (old vs new)
  body TEXT NOT NULL,
  status TEXT NOT NULL,             -- 'pending' | 'posted' | 'resolved'
  posted_at TEXT,                   -- When sent to agent
  resolved_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
```

### Review Workflow

```
                    ┌─────────────┐
                    │   pending   │
                    └──────┬──────┘
                           │ start()
                           ▼
                   ┌─────────────┐
         ┌─────────│ in_progress │◄────────┐
         │         └──────┬──────┘         │
         │                │ submit_for_review()
         │                ▼                │
         │         ┌─────────────┐         │
         │         │   review    │─────────┤ reject()
         │         └──────┬──────┘         │
         │                │                │
         │    ┌───────────┴───────────┐    │
         │    │                       │    │
         │    ▼                       ▼    │
         │ approve()            request_changes()
         │    │                       │    │
         │    ▼                       └────┘
         │ ┌─────────────┐
         │ │  completed  │
         │ └─────────────┘
         │
         └─ cancel()
            ┌─────────────┐
            │  cancelled  │
            └─────────────┘
```

### Comment → Agent Feedback Loop

```rust
// Immediate comment (posts to active harness session)
overseer::reviews::comment(task_id, CommentInput {
    file_path: "src/auth.rs",
    line_start: Some(42),
    line_end: Some(45),
    body: "This should use bcrypt, not md5",
    post_immediately: true,  // Send to agent now
}) -> Result<Comment>

// When post_immediately = true:
// 1. Find active harness session for this task
// 2. Send comment as agent message
// 3. Agent receives feedback in real-time

// Batched review (posts all pending comments)
overseer::reviews::request_changes(task_id, pending_comments) {
    // 1. Mark review as changes_requested
    // 2. Emit ReviewEvent::ChangesRequested with all comments
    // 3. Relay broadcasts to connected harness
    // 4. Agent picks up review feedback
}
```

---

## Event System

### Event Types

```rust
#[derive(Debug, Clone, Serialize)]
pub struct Event {
    pub id: EventId,
    pub seq: i64,                    // Monotonic, for tailing
    pub at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub source: EventSource,         // Cli | Mcp | Ui | Relay | Plugin
    pub body: EventBody,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum EventBody {
    // Task lifecycle
    TaskCreated { task: Task },
    TaskStarted { task: Task },
    TaskSubmitted { task: Task, review_id: ReviewId },
    TaskApproved { task: Task, review_id: ReviewId },
    TaskRejected { task: Task, review_id: ReviewId, reason: Option<String> },
    TaskCompleted { task: Task },
    TaskCancelled { task: Task },

    // Reviews
    ReviewCreated { review: Review },
    CommentAdded { comment: Comment },
    ChangesRequested { review: Review, comments: Vec<Comment> },

    // VCS
    RefCreated { task_id: TaskId, ref_name: String, target: String },
    Committed { task_id: TaskId, rev: String },

    // Harnesses
    HarnessConnected { harness_id: String },
    HarnessDisconnected { harness_id: String },
    SessionStarted { session_id: String, task_id: TaskId, harness_id: String },
    SessionProgress { session_id: String, message: String },
    SessionCompleted { session_id: String },

    // Blockers
    BlockerAdded { task_id: TaskId, blocker_id: TaskId },
    BlockerRemoved { task_id: TaskId, blocker_id: TaskId },

    // Learnings
    LearningAdded { learning: Learning },
    LearningBubbled { from: TaskId, to: TaskId, learning_ids: Vec<LearningId> },
}
```

### Persistence

```sql
CREATE TABLE events (
  seq INTEGER PRIMARY KEY AUTOINCREMENT,
  id TEXT NOT NULL UNIQUE,
  at TEXT NOT NULL,
  correlation_id TEXT,
  source TEXT NOT NULL,
  event_type TEXT NOT NULL,         -- 'task.created', 'review.approved', etc.
  task_id TEXT,                     -- Indexed for task-scoped queries
  payload TEXT NOT NULL             -- JSON EventBody
);

CREATE INDEX idx_events_task ON events(task_id, seq);
CREATE INDEX idx_events_type ON events(event_type, seq);
CREATE INDEX idx_events_corr ON events(correlation_id);
```

### Subscription API

```rust
// In-process subscription
let mut stream = overseer.events.subscribe(EventFilter {
    types: vec!["task.*", "review.*"],
    task_id: Some(task_id),
});

while let Some(event) = stream.next().await {
    match event.body {
        EventBody::TaskApproved { task, .. } => { /* ... */ }
        _ => {}
    }
}

// External tailing (plugins, relay)
// Plugins tail via: os events tail --after-seq 123 --follow
// Relay tails SQLite directly or via CLI
```

---

## Unified Rust Binary (Boa Integration)

### Why Boa?

- **Single binary** - No Node.js runtime dependency
- **Direct function calls** - No CLI spawn overhead
- **94% ES2023 conformance** - Full async/await support
- **Pure Rust** - Easy cross-compilation

### Architecture

```rust
// overseer/src/mcp/executor.rs
use boa_engine::{Context, Source, JsObject, NativeFunction, js_string};

pub struct JsExecutor {
    context: Context,
    overseer: Arc<Overseer>,
}

impl JsExecutor {
    pub fn new(overseer: Arc<Overseer>) -> Result<Self> {
        let mut context = Context::default();

        // Register tasks API
        let tasks = Self::create_tasks_api(&overseer, &mut context)?;
        context.register_global_property(js_string!("tasks"), tasks, Attribute::default())?;

        // Register reviews API
        let reviews = Self::create_reviews_api(&overseer, &mut context)?;
        context.register_global_property(js_string!("reviews"), reviews, Attribute::default())?;

        // Register learnings API
        let learnings = Self::create_learnings_api(&overseer, &mut context)?;
        context.register_global_property(js_string!("learnings"), learnings, Attribute::default())?;

        Ok(Self { context, overseer })
    }

    fn create_tasks_api(overseer: &Arc<Overseer>, ctx: &mut Context) -> Result<JsObject> {
        let tasks = JsObject::default();
        let os = overseer.clone();

        // tasks.create()
        tasks.create_data_property(
            js_string!("create"),
            NativeFunction::from_async_fn(move |_, args, context| {
                let os = os.clone();
                async move {
                    let input = parse_create_input(args, context)?;
                    let task = os.tasks.create(input)?;
                    Ok(task_to_js(task, context))
                }
            }).to_js_function(ctx.realm()),
            ctx
        )?;

        // tasks.list(), tasks.get(), tasks.start(), etc.
        // ...

        Ok(tasks)
    }

    pub async fn execute(&mut self, code: &str) -> Result<JsValue> {
        let wrapped = format!("(async () => {{ {} }})()", code);
        self.context.eval(Source::from_bytes(&wrapped))
    }
}
```

---

## Relay Server (Agent Harness Broker)

Adapted from react-grab relay pattern:

```rust
// overseer/src/relay/server.rs
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;

pub struct RelayServer {
    overseer: Arc<Overseer>,
    handlers: Arc<RwLock<HashMap<String, HarnessHandler>>>,
    sessions: Arc<RwLock<HashMap<String, Session>>>,
}

impl RelayServer {
    pub async fn run(&self, addr: &str) -> Result<()> {
        let listener = TcpListener::bind(addr).await?;

        while let Ok((stream, _)) = listener.accept().await {
            let ws = accept_async(stream).await?;
            let relay = self.clone();

            tokio::spawn(async move {
                relay.handle_connection(ws).await
            });
        }

        Ok(())
    }

    async fn handle_connection(&self, ws: WebSocketStream) {
        // Determine if this is a harness provider or UI client
        // Route messages accordingly
    }
}

// Protocol
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RelayMessage {
    // From harness providers
    RegisterHandler { agent_id: String },
    AgentStatus { session_id: String, message: String },
    AgentDone { session_id: String, result: Option<String> },

    // From UI/clients
    TaskRequest { task_id: TaskId, agent_id: String },
    TaskAbort { session_id: String },

    // Review feedback (bidirectional)
    ReviewComment { task_id: TaskId, comment: Comment },
    ReviewSubmitted { task_id: TaskId, status: ReviewStatus },
}
```

---

## Migration Path

### Phase 1: Foundation (M)
1. Fix jj commit ID bug (head_rev from commit result, not re-query)
2. Add `repos` table + `repo_id` to tasks
3. Add `task_vcs` table with `base_rev`/`head_rev`
4. Add `events` table
5. Enable SQLite WAL mode + busy_timeout

### Phase 2: Serve Mode (M-L)
1. Add `os serve` command with axum
2. Implement REST API for tasks/learnings
3. Add SSE endpoint for event streaming
4. Keep existing CLI working (direct SDK calls)

### Phase 3: Review Workflow (M-L)
1. Add `reviews` and `review_comments` tables
2. Add `requires_review` status
3. Implement `submit_for_review()`, `approve()`, `reject()`
4. Update task state machine

### Phase 4: SDK Refactor (L)
1. Restructure into Stripe-style modules
2. Extract business logic from services
3. Add event emission throughout
4. Keep CLI working during transition

### Phase 5: Boa Integration (L)
1. Add boa-engine dependency
2. Implement JsExecutor with task/review/learning APIs
3. Wire into `os serve` and/or `os mcp` mode
4. Remove Node.js host package

### Phase 6: Relay + Harnesses (XL)
1. Implement relay WebSocket server (in `os serve`)
2. Define harness protocol
3. Build OpenCode/Claude Code providers
4. Add review → agent feedback loop

### Phase 7: Multi-Client Support (L)
1. Tauri desktop app (sidecar `os serve`)
2. TUI client (ratatui or similar)
3. Optional: `os install-service` for launchd/systemd

---

## Unresolved Questions

1. **Cancelled task refs**: Keep forever or explicit `os gc`?
2. **Review comments on cancelled tasks**: Preserve or delete?
3. **Multi-repo task moves**: Allow moving task to different repo?
4. **Worktree cleanup**: Auto-delete on complete, or keep for history?
5. **Event retention**: TTL for events, or keep forever?
6. **Harness auth**: How do harnesses authenticate with relay?
7. **Diff storage**: Store computed diffs, or always compute from VCS?
8. **Serve mode port**: Fixed (e.g., 4820) or dynamic with discovery?
9. **CLI → server routing**: Auto-detect running server, or explicit flag?
10. **Tauri embedding**: Sidecar vs embedded Rust library?
