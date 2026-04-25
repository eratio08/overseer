use rusqlite::Connection;

use crate::core::TaskService;
use crate::db::task_repo;
use crate::error::{NotReadyReason, OsError, Result};
use crate::id::TaskId;
use crate::types::Task;

/// Coordinates task lifecycle transitions.
pub struct TaskWorkflowService<'a> {
    task_service: TaskService<'a>,
    conn: &'a Connection,
}

impl<'a> TaskWorkflowService<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self {
            task_service: TaskService::new(conn),
            conn,
        }
    }

    /// Access the underlying TaskService (used primarily in tests)
    #[allow(dead_code)]
    pub fn task_service(&self) -> &TaskService<'a> {
        &self.task_service
    }

    pub fn start(&self, id: &TaskId) -> Result<Task> {
        let task = self.task_service.get(id)?;

        // Guard: cannot start non-active tasks (cancelled, completed, archived)
        // This check MUST come before the idempotent path to prevent starting
        // tasks that were started then cancelled.
        if !task.is_active_for_work() {
            return match task.lifecycle_state() {
                crate::types::LifecycleState::Completed => Err(OsError::CannotStartCompleted),
                crate::types::LifecycleState::Cancelled => Err(OsError::CannotStartCancelled),
                crate::types::LifecycleState::Archived => Err(OsError::CannotModifyArchived),
                // These are active states - is_active_for_work() would have returned true
                crate::types::LifecycleState::Pending
                | crate::types::LifecycleState::InProgress => {
                    unreachable!("is_active_for_work() returned false but state is active")
                }
            };
        }

        if task.started_at.is_some() {
            return self.task_service.get(id);
        }

        // Validate: must be the next ready task in its subtree
        self.validate_start_target(id, &task)?;

        self.task_service.start(id)?;

        // Bubble started_at to ancestors.
        self.bubble_start_to_ancestors(id)?;

        self.task_service.get(id)
    }

    /// Validate that a task can be started.
    /// Returns error if task is not the next ready task in its subtree.
    fn validate_start_target(&self, id: &TaskId, task: &Task) -> Result<()> {
        // Check if blocked first (more specific error)
        if self.task_service.is_effectively_blocked(task)? {
            let blockers: Vec<TaskId> = task
                .blocked_by
                .iter()
                .filter(|b| !task_repo::is_task_completed(self.conn, b).unwrap_or(false))
                .cloned()
                .collect();

            // Search globally for a ready task (not within blocked subtree)
            let next_ready = self.task_service.next_ready(None)?;

            return Err(OsError::NotNextReady {
                message: format!(
                    "Cannot start '{}' - blocked by {}. {}",
                    task.description,
                    blockers
                        .iter()
                        .map(|b| b.to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                    next_ready
                        .as_ref()
                        .map(|nr| format!("Start '{}' instead.", nr))
                        .unwrap_or_else(|| "No ready tasks available.".to_string())
                ),
                requested: id.clone(),
                next_ready,
                reason: NotReadyReason::Blocked { blockers },
            });
        }

        // Check if this is the next ready task in its subtree
        let next_ready = self.task_service.next_ready(Some(id))?;

        match next_ready {
            Some(ref ready_id) if ready_id == id => Ok(()), // This is the next ready task
            Some(ref ready_id) => {
                // Has incomplete children - should start the ready child instead
                let ready_task = self.task_service.get(ready_id)?;
                Err(OsError::NotNextReady {
                    message: format!(
                        "Cannot start '{}' - has incomplete children. Start '{}' instead.",
                        task.description, ready_task.description
                    ),
                    requested: id.clone(),
                    next_ready: Some(ready_id.clone()),
                    reason: NotReadyReason::HasIncompleteChildren,
                })
            }
            None => {
                // No ready tasks - might be all complete or all blocked
                Err(OsError::NotNextReady {
                    message: format!(
                        "Cannot start '{}' - no ready tasks in subtree (all complete or blocked).",
                        task.description
                    ),
                    requested: id.clone(),
                    next_ready: None,
                    reason: NotReadyReason::NoReadyTasksInSubtree,
                })
            }
        }
    }

    /// Bubble started_at to all ancestors that don't have it set.
    fn bubble_start_to_ancestors(&self, id: &TaskId) -> Result<()> {
        let mut current_id = id.clone();

        loop {
            let current = task_repo::get_task(self.conn, &current_id)?
                .ok_or_else(|| OsError::TaskNotFound(current_id.clone()))?;

            let Some(parent_id) = current.parent_id else {
                break;
            };

            let parent = task_repo::get_task(self.conn, &parent_id)?
                .ok_or_else(|| OsError::TaskNotFound(parent_id.clone()))?;

            // Only set started_at if not already set
            if parent.started_at.is_none() {
                self.task_service.start(&parent_id)?;
            }

            current_id = parent_id;
        }

        Ok(())
    }

    /// Start a task, following blockers to find startable work.
    ///
    /// If the requested task or any of its descendants are blocked,
    /// follows blockers until finding a startable task.
    /// Cascades down to deepest incomplete leaf.
    pub fn start_follow_blockers(&self, root: &TaskId) -> Result<Task> {
        let target = self.task_service.resolve_start_target(root)?;
        self.start(&target)
    }

    /// Convenience method for completing without learnings.
    #[allow(dead_code)] // Used in tests
    pub fn complete(&self, id: &TaskId, result: Option<&str>) -> Result<Task> {
        self.complete_with_learnings(id, result, &[])
    }

    /// Complete a task with optional learnings.
    /// Learnings are added to the task and bubbled to immediate parent.
    pub fn complete_with_learnings(
        &self,
        id: &TaskId,
        result: Option<&str>,
        learnings: &[String],
    ) -> Result<Task> {
        let task = self.task_service.get(id)?;

        // Lifecycle guard: reject inactive states
        if task.archived {
            return Err(OsError::CannotCompleteArchived);
        }
        if task.cancelled {
            return Err(OsError::CannotCompleteCancelled);
        }

        // Idempotent: already completed
        if task.completed {
            return Ok(task);
        }

        // Auto-detect milestone (depth 0)
        if task.depth == Some(0) {
            return self.complete_milestone_with_learnings(id, result, learnings);
        }

        let completed_task = self
            .task_service
            .complete_with_learnings(id, result, learnings)?;

        // Bubble up: auto-complete parents if all children done and unblocked
        self.bubble_up_completion(id)?;

        Ok(completed_task)
    }

    /// Auto-complete parent tasks if all siblings are done and parent is unblocked.
    /// Bubbles up recursively until hitting a blocked parent or pending children.
    fn bubble_up_completion(&self, completed_id: &TaskId) -> Result<()> {
        let mut current_id = completed_id.clone();

        loop {
            let current = task_repo::get_task(self.conn, &current_id)?
                .ok_or_else(|| crate::error::OsError::TaskNotFound(current_id.clone()))?;

            let Some(parent_id) = current.parent_id else {
                break;
            };

            // Check if parent has pending children
            if task_repo::has_pending_children(self.conn, &parent_id)? {
                break;
            }

            // Check if parent is blocked
            let parent = self.task_service.get(&parent_id)?;
            if self.task_service.is_effectively_blocked(&parent)? {
                break;
            }

            // Auto-complete parent (use service method to handle depth-0 special case)
            if parent.depth == Some(0) {
                self.complete_milestone(&parent_id, None)?;
            } else {
                self.task_service.complete(&parent_id, None)?;
            }

            current_id = parent_id;
        }

        Ok(())
    }

    pub fn complete_milestone(&self, id: &TaskId, result: Option<&str>) -> Result<Task> {
        self.complete_milestone_with_learnings(id, result, &[])
    }

    /// Complete a milestone with optional learnings.
    pub fn complete_milestone_with_learnings(
        &self,
        id: &TaskId,
        result: Option<&str>,
        learnings: &[String],
    ) -> Result<Task> {
        let task = self.task_service.get(id)?;

        // Lifecycle guard: reject inactive states
        if task.archived {
            return Err(OsError::CannotCompleteArchived);
        }
        if task.cancelled {
            return Err(OsError::CannotCompleteCancelled);
        }

        // Idempotent: already completed
        if task.completed {
            return Ok(task);
        }

        // Not a milestone - delegate to regular complete (avoid infinite recursion)
        if task.depth != Some(0) {
            let completed_task = self
                .task_service
                .complete_with_learnings(id, result, learnings)?;

            return Ok(completed_task);
        }

        self.task_service.complete_with_learnings(id, result, learnings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::init_schema;
    use crate::types::CreateTaskInput;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        init_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn test_start_records_vcs_state() {
        let conn = setup_db();
        let service = TaskWorkflowService::new(&conn);

        let task = service
            .task_service()
            .create(&CreateTaskInput {
                description: "Test task".to_string(),
                context: None,
                parent_id: None,
                priority: None,
                blocked_by: vec![],
            })
            .unwrap();

        let started = service.start(&task.id).unwrap();
        assert!(started.started_at.is_some());
    }

    #[test]
    fn test_complete_updates_state() {
        let conn = setup_db();
        let service = TaskWorkflowService::new(&conn);

        let task = service
            .task_service()
            .create(&CreateTaskInput {
                description: "Test task".to_string(),
                context: None,
                parent_id: None,
                priority: None,
                blocked_by: vec![],
            })
            .unwrap();

        let completed = service.complete(&task.id, Some("Done")).unwrap();
        assert!(completed.completed);
        assert_eq!(completed.result, Some("Done".to_string()));
    }

    #[test]
    fn test_start_cascades_to_deepest_leaf() {
        let conn = setup_db();
        let service = TaskWorkflowService::new(&conn);
        let svc = service.task_service();

        // Create: milestone -> task -> subtask
        let milestone = svc
            .create(&CreateTaskInput {
                description: "Milestone".to_string(),
                context: None,
                parent_id: None,
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        let task = svc
            .create(&CreateTaskInput {
                description: "Task".to_string(),
                context: None,
                parent_id: Some(milestone.id.clone()),
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        let subtask = svc
            .create(&CreateTaskInput {
                description: "Subtask".to_string(),
                context: None,
                parent_id: Some(task.id.clone()),
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        // Starting milestone should cascade to subtask
        let started = service.start_follow_blockers(&milestone.id).unwrap();
        assert_eq!(started.id, subtask.id);
        assert!(started.started_at.is_some());
    }

    #[test]
    fn test_start_follows_blockers_to_startable() {
        let conn = setup_db();
        let service = TaskWorkflowService::new(&conn);
        let svc = service.task_service();

        // Create: blocker_task, blocked_milestone -> task
        let blocker_task = svc
            .create(&CreateTaskInput {
                description: "Blocker task".to_string(),
                context: None,
                parent_id: None,
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        let blocked_milestone = svc
            .create(&CreateTaskInput {
                description: "Blocked milestone".to_string(),
                context: None,
                parent_id: None,
                priority: Some(0),
                blocked_by: vec![blocker_task.id.clone()],
            })
            .unwrap();

        let _task = svc
            .create(&CreateTaskInput {
                description: "Task under blocked milestone".to_string(),
                context: None,
                parent_id: Some(blocked_milestone.id.clone()),
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        // Starting blocked_milestone should follow blocker and start blocker_task
        let started = service
            .start_follow_blockers(&blocked_milestone.id)
            .unwrap();
        assert_eq!(started.id, blocker_task.id);
        assert!(started.started_at.is_some());
    }

    #[test]
    fn test_complete_bubbles_up_to_parent() {
        let conn = setup_db();
        let service = TaskWorkflowService::new(&conn);
        let svc = service.task_service();

        // Create: milestone -> task -> subtask1, subtask2
        let milestone = svc
            .create(&CreateTaskInput {
                description: "Milestone".to_string(),
                context: None,
                parent_id: None,
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        let task = svc
            .create(&CreateTaskInput {
                description: "Task".to_string(),
                context: None,
                parent_id: Some(milestone.id.clone()),
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        let subtask1 = svc
            .create(&CreateTaskInput {
                description: "Subtask 1".to_string(),
                context: None,
                parent_id: Some(task.id.clone()),
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        let subtask2 = svc
            .create(&CreateTaskInput {
                description: "Subtask 2".to_string(),
                context: None,
                parent_id: Some(task.id.clone()),
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        // Complete first subtask - task should NOT be auto-completed
        service.complete(&subtask1.id, None).unwrap();
        let task_after = svc.get(&task.id).unwrap();
        assert!(!task_after.completed);

        // Complete second subtask - task SHOULD be auto-completed
        service.complete(&subtask2.id, None).unwrap();
        let task_after = svc.get(&task.id).unwrap();
        assert!(task_after.completed);
    }

    #[test]
    fn test_complete_bubbles_up_to_milestone() {
        let conn = setup_db();
        let service = TaskWorkflowService::new(&conn);
        let svc = service.task_service();

        // Create: milestone -> task (single task, no siblings)
        let milestone = svc
            .create(&CreateTaskInput {
                description: "Milestone".to_string(),
                context: None,
                parent_id: None,
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        let task = svc
            .create(&CreateTaskInput {
                description: "Task".to_string(),
                context: None,
                parent_id: Some(milestone.id.clone()),
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        // Complete task - milestone should auto-complete
        service.complete(&task.id, None).unwrap();

        let milestone_after = svc.get(&milestone.id).unwrap();
        assert!(milestone_after.completed);
    }

    #[test]
    fn test_complete_stops_at_blocked_parent() {
        let conn = setup_db();
        let service = TaskWorkflowService::new(&conn);
        let svc = service.task_service();

        // Create: blocker, milestone (blocked by blocker) -> task
        let blocker = svc
            .create(&CreateTaskInput {
                description: "Blocker".to_string(),
                context: None,
                parent_id: None,
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        let milestone = svc
            .create(&CreateTaskInput {
                description: "Blocked milestone".to_string(),
                context: None,
                parent_id: None,
                priority: Some(0),
                blocked_by: vec![blocker.id.clone()],
            })
            .unwrap();

        let task = svc
            .create(&CreateTaskInput {
                description: "Task".to_string(),
                context: None,
                parent_id: Some(milestone.id.clone()),
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        // Complete task - milestone should NOT auto-complete (it's blocked)
        service.complete(&task.id, None).unwrap();

        let milestone_after = svc.get(&milestone.id).unwrap();
        assert!(!milestone_after.completed);
    }

    #[test]
    fn test_complete_stops_at_pending_siblings() {
        let conn = setup_db();
        let service = TaskWorkflowService::new(&conn);
        let svc = service.task_service();

        // Create: milestone -> task1, task2
        let milestone = svc
            .create(&CreateTaskInput {
                description: "Milestone".to_string(),
                context: None,
                parent_id: None,
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        let task1 = svc
            .create(&CreateTaskInput {
                description: "Task 1".to_string(),
                context: None,
                parent_id: Some(milestone.id.clone()),
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        let _task2 = svc
            .create(&CreateTaskInput {
                description: "Task 2".to_string(),
                context: None,
                parent_id: Some(milestone.id.clone()),
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        // Complete task1 - milestone should NOT auto-complete (task2 still pending)
        service.complete(&task1.id, None).unwrap();

        let milestone_after = svc.get(&milestone.id).unwrap();
        assert!(!milestone_after.completed);
    }

    #[test]
    fn test_complete_with_learnings_bubbles_to_parent() {
        let conn = setup_db();
        let service = TaskWorkflowService::new(&conn);
        let svc = service.task_service();

        // Create: milestone -> task -> subtask1, subtask2 (sibling prevents auto-complete)
        let milestone = svc
            .create(&CreateTaskInput {
                description: "Milestone".to_string(),
                context: None,
                parent_id: None,
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        let task = svc
            .create(&CreateTaskInput {
                description: "Task".to_string(),
                context: None,
                parent_id: Some(milestone.id.clone()),
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        let subtask1 = svc
            .create(&CreateTaskInput {
                description: "Subtask 1".to_string(),
                context: None,
                parent_id: Some(task.id.clone()),
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        // Second subtask prevents task from auto-completing
        let _subtask2 = svc
            .create(&CreateTaskInput {
                description: "Subtask 2".to_string(),
                context: None,
                parent_id: Some(task.id.clone()),
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        // Complete subtask1 with learnings
        service
            .complete_with_learnings(
                &subtask1.id,
                Some("done"),
                &["Learning 1".to_string(), "Learning 2".to_string()],
            )
            .unwrap();

        // Learnings should be on subtask1
        let subtask_learnings =
            crate::db::learning_repo::list_learnings(&conn, &subtask1.id).unwrap();
        assert_eq!(subtask_learnings.len(), 2);
        assert_eq!(subtask_learnings[0].content, "Learning 1");
        assert_eq!(subtask_learnings[1].content, "Learning 2");
        // Origin should be subtask1 itself
        assert_eq!(
            subtask_learnings[0].source_task_id,
            Some(subtask1.id.clone())
        );

        // Learnings should have bubbled to task (parent)
        let task_learnings = crate::db::learning_repo::list_learnings(&conn, &task.id).unwrap();
        assert_eq!(task_learnings.len(), 2);
        assert_eq!(task_learnings[0].content, "Learning 1");
        // Origin preserved through bubble
        assert_eq!(task_learnings[0].source_task_id, Some(subtask1.id.clone()));

        // Task should NOT be auto-completed (subtask2 still pending)
        let task_after = svc.get(&task.id).unwrap();
        assert!(!task_after.completed);

        // Learnings should NOT be on milestone yet (task not completed)
        let milestone_learnings =
            crate::db::learning_repo::list_learnings(&conn, &milestone.id).unwrap();
        assert_eq!(milestone_learnings.len(), 0);
    }

    #[test]
    fn test_learnings_bubble_transitively_on_parent_complete() {
        let conn = setup_db();
        let service = TaskWorkflowService::new(&conn);
        let svc = service.task_service();

        // Create: milestone -> task -> subtask
        let milestone = svc
            .create(&CreateTaskInput {
                description: "Milestone".to_string(),
                context: None,
                parent_id: None,
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        let task = svc
            .create(&CreateTaskInput {
                description: "Task".to_string(),
                context: None,
                parent_id: Some(milestone.id.clone()),
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        let subtask = svc
            .create(&CreateTaskInput {
                description: "Subtask".to_string(),
                context: None,
                parent_id: Some(task.id.clone()),
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        // Complete subtask with learning
        service
            .complete_with_learnings(&subtask.id, None, &["From subtask".to_string()])
            .unwrap();

        // Task auto-completes (only child done), which bubbles learnings to milestone
        let task_after = svc.get(&task.id).unwrap();
        assert!(task_after.completed);

        // Now milestone should have the learning (bubbled from task which had it from subtask)
        let milestone_learnings =
            crate::db::learning_repo::list_learnings(&conn, &milestone.id).unwrap();
        assert_eq!(milestone_learnings.len(), 1);
        assert_eq!(milestone_learnings[0].content, "From subtask");
        // Origin preserved: still points to subtask
        assert_eq!(
            milestone_learnings[0].source_task_id,
            Some(subtask.id.clone())
        );
    }

    #[test]
    fn test_sibling_sees_learnings_via_parent() {
        let conn = setup_db();
        let service = TaskWorkflowService::new(&conn);
        let svc = service.task_service();

        // Create: milestone -> task_a (with subtasks), task_b (with subtasks)
        let milestone = svc
            .create(&CreateTaskInput {
                description: "Milestone".to_string(),
                context: None,
                parent_id: None,
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        let task_a = svc
            .create(&CreateTaskInput {
                description: "Task A".to_string(),
                context: None,
                parent_id: Some(milestone.id.clone()),
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        let subtask_a1 = svc
            .create(&CreateTaskInput {
                description: "Subtask A1".to_string(),
                context: None,
                parent_id: Some(task_a.id.clone()),
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        let subtask_a2 = svc
            .create(&CreateTaskInput {
                description: "Subtask A2".to_string(),
                context: None,
                parent_id: Some(task_a.id.clone()),
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        // Complete A1 with learning
        service
            .complete_with_learnings(&subtask_a1.id, None, &["A1 discovery".to_string()])
            .unwrap();

        // A2 should see A1's learning via parent (task_a)
        let task_a_learnings = crate::db::learning_repo::list_learnings(&conn, &task_a.id).unwrap();
        assert_eq!(task_a_learnings.len(), 1);
        assert_eq!(task_a_learnings[0].content, "A1 discovery");

        // Start A2 and get its inherited learnings
        let a2_with_context = svc.get(&subtask_a2.id).unwrap();
        // InheritedLearnings.parent should contain A1's learning
        assert!(a2_with_context.learnings.is_some());
        let inherited = a2_with_context.learnings.unwrap();
        assert_eq!(inherited.parent.len(), 1);
        assert_eq!(inherited.parent[0].content, "A1 discovery");
    }

    #[test]
    fn test_learnings_idempotent_on_retry() {
        let conn = setup_db();
        let service = TaskWorkflowService::new(&conn);
        let svc = service.task_service();

        let milestone = svc
            .create(&CreateTaskInput {
                description: "Milestone".to_string(),
                context: None,
                parent_id: None,
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        let task = svc
            .create(&CreateTaskInput {
                description: "Task".to_string(),
                context: None,
                parent_id: Some(milestone.id.clone()),
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        // Complete with learning
        service
            .complete_with_learnings(&task.id, None, &["Important note".to_string()])
            .unwrap();

        // Try to complete again (idempotent) - should not duplicate learnings
        service
            .complete_with_learnings(&task.id, None, &["Important note".to_string()])
            .unwrap();

        // Should still only have 1 learning on milestone (not duplicated)
        let milestone_learnings =
            crate::db::learning_repo::list_learnings(&conn, &milestone.id).unwrap();
        assert_eq!(milestone_learnings.len(), 1);
    }

    #[test]
    fn test_start_rejects_parent_with_incomplete_children() {
        let conn = setup_db();
        let service = TaskWorkflowService::new(&conn);
        let svc = service.task_service();

        // Create: milestone -> task -> subtask
        let milestone = svc
            .create(&CreateTaskInput {
                description: "Milestone".to_string(),
                context: None,
                parent_id: None,
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        let task = svc
            .create(&CreateTaskInput {
                description: "Task".to_string(),
                context: None,
                parent_id: Some(milestone.id.clone()),
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        let subtask = svc
            .create(&CreateTaskInput {
                description: "Subtask".to_string(),
                context: None,
                parent_id: Some(task.id.clone()),
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        // Starting milestone directly should fail
        let result = service.start(&milestone.id);
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            OsError::NotNextReady {
                requested,
                next_ready,
                ..
            } => {
                assert_eq!(requested, milestone.id);
                assert_eq!(next_ready, Some(subtask.id.clone()));
            }
            _ => panic!("Expected NotNextReady error, got {:?}", err),
        }

        // Starting task directly should also fail (has subtask)
        let result = service.start(&task.id);
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            OsError::NotNextReady {
                requested,
                next_ready,
                ..
            } => {
                assert_eq!(requested, task.id);
                assert_eq!(next_ready, Some(subtask.id.clone()));
            }
            _ => panic!("Expected NotNextReady error, got {:?}", err),
        }

        // Starting subtask should succeed
        let result = service.start(&subtask.id);
        assert!(result.is_ok());
    }

    #[test]
    fn test_start_rejects_blocked_task() {
        let conn = setup_db();
        let service = TaskWorkflowService::new(&conn);
        let svc = service.task_service();

        // Create: blocker, blocked_task (blocked by blocker)
        let blocker = svc
            .create(&CreateTaskInput {
                description: "Blocker".to_string(),
                context: None,
                parent_id: None,
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        let blocked_task = svc
            .create(&CreateTaskInput {
                description: "Blocked task".to_string(),
                context: None,
                parent_id: None,
                priority: Some(0),
                blocked_by: vec![blocker.id.clone()],
            })
            .unwrap();

        // Starting blocked_task should fail with blocked error
        let result = service.start(&blocked_task.id);
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            OsError::NotNextReady { reason, .. } => {
                assert!(matches!(reason, NotReadyReason::Blocked { .. }));
            }
            _ => panic!(
                "Expected NotNextReady error with Blocked reason, got {:?}",
                err
            ),
        }
    }

    #[test]
    fn test_start_bubbles_started_at_to_ancestors() {
        let conn = setup_db();
        let service = TaskWorkflowService::new(&conn);
        let svc = service.task_service();

        // Create: milestone -> task -> subtask
        let milestone = svc
            .create(&CreateTaskInput {
                description: "Milestone".to_string(),
                context: None,
                parent_id: None,
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        let task = svc
            .create(&CreateTaskInput {
                description: "Task".to_string(),
                context: None,
                parent_id: Some(milestone.id.clone()),
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        let subtask = svc
            .create(&CreateTaskInput {
                description: "Subtask".to_string(),
                context: None,
                parent_id: Some(task.id.clone()),
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        // Verify ancestors have no started_at initially
        assert!(svc.get(&milestone.id).unwrap().started_at.is_none());
        assert!(svc.get(&task.id).unwrap().started_at.is_none());
        assert!(svc.get(&subtask.id).unwrap().started_at.is_none());

        // Start the subtask
        let started = service.start(&subtask.id).unwrap();
        assert!(started.started_at.is_some());

        // Ancestors should now have started_at.
        let task_after = svc.get(&task.id).unwrap();
        assert!(task_after.started_at.is_some());

        let milestone_after = svc.get(&milestone.id).unwrap();
        assert!(milestone_after.started_at.is_some());
    }

    #[test]
    fn test_start_is_idempotent() {
        let conn = setup_db();
        let service = TaskWorkflowService::new(&conn);
        let svc = service.task_service();

        let task = svc
            .create(&CreateTaskInput {
                description: "Task".to_string(),
                context: None,
                parent_id: None,
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        // Start task
        let first_start = service.start(&task.id).unwrap();
        assert!(first_start.started_at.is_some());
        let first_started_at = first_start.started_at;

        // Start again - should be idempotent
        let second_start = service.start(&task.id).unwrap();
        assert_eq!(second_start.started_at, first_started_at);
    }

    #[test]
    fn test_start_allows_leaf_without_children() {
        let conn = setup_db();
        let service = TaskWorkflowService::new(&conn);
        let svc = service.task_service();

        // Create a milestone with no children (it's a leaf)
        let milestone = svc
            .create(&CreateTaskInput {
                description: "Leaf milestone".to_string(),
                context: None,
                parent_id: None,
                priority: Some(0),
                blocked_by: vec![],
            })
            .unwrap();

        // Should be startable since it's a leaf
        let result = service.start(&milestone.id);
        assert!(result.is_ok());
        assert!(result.unwrap().started_at.is_some());
    }

    #[test]
    fn test_complete_cancelled_task_fails() {
        let conn = setup_db();
        let service = TaskWorkflowService::new(&conn);
        let svc = service.task_service();

        let task = svc
            .create(&CreateTaskInput {
                description: "Task".to_string(),
                context: None,
                parent_id: None,
                priority: None,
                blocked_by: vec![],
            })
            .unwrap();

        // Cancel the task
        svc.cancel(&task.id).unwrap();

        // Try to complete cancelled task
        let result = service.complete_with_learnings(&task.id, None, &[]);
        assert!(
            matches!(result, Err(OsError::CannotCompleteCancelled)),
            "Expected CannotCompleteCancelled error, got {:?}",
            result
        );
    }

    #[test]
    fn test_complete_archived_task_fails() {
        let conn = setup_db();
        let service = TaskWorkflowService::new(&conn);
        let svc = service.task_service();

        let task = svc
            .create(&CreateTaskInput {
                description: "Task".to_string(),
                context: None,
                parent_id: None,
                priority: None,
                blocked_by: vec![],
            })
            .unwrap();

        // Complete then archive the task
        svc.complete(&task.id, None).unwrap();
        svc.archive(&task.id).unwrap();

        // Try to complete archived task (even though already completed, archived check comes first)
        let result = service.complete_with_learnings(&task.id, None, &[]);
        assert!(
            matches!(result, Err(OsError::CannotCompleteArchived)),
            "Expected CannotCompleteArchived error, got {:?}",
            result
        );
    }
}
