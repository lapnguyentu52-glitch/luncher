use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::{Mutex, RwLock};
use serde::Serialize;

use crate::error::{CoreError, CoreResult};

/// §90 — Task state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskState {
    Queued,
    Running,
    Paused,
    CancelRequested,
    Cancelled,
    Failed,
    Completed,
}

impl TaskState {
    pub fn is_terminal(self) -> bool {
        matches!(self, TaskState::Cancelled | TaskState::Failed | TaskState::Completed)
    }
}

/// §91 — Task queue priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskPriority {
    P0Critical,
    P1UserAction,
    P2Background,
    P3Prefetch,
}

/// §90 — Task model.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub kind: String,
    pub dedupe_key: Option<String>,
    pub parent_id: Option<String>,
    pub state: TaskState,
    pub priority: TaskPriority,
    pub progress: f64,
    pub message: Option<String>,
    pub cancellable: bool,
    pub started_at_ms: Option<u64>,
    pub finished_at_ms: Option<u64>,
}

impl Task {
    fn new(kind: impl Into<String>, priority: TaskPriority, dedupe_key: Option<String>) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        Self {
            id: format!("task_{}", SEQ.fetch_add(1, Ordering::Relaxed)),
            kind: kind.into(),
            dedupe_key,
            parent_id: None,
            state: TaskState::Queued,
            priority,
            progress: 0.0,
            message: None,
            cancellable: true,
            started_at_ms: None,
            finished_at_ms: None,
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Transition rules của state machine §90.
fn transition_allowed(from: TaskState, to: TaskState) -> bool {
    use TaskState::*;
    matches!(
        (from, to),
        (Queued, Running)
            | (Queued, Cancelled)
            | (Running, Paused)
            | (Running, CancelRequested)
            | (Running, Failed)
            | (Running, Completed)
            | (Paused, Running)
            | (Paused, CancelRequested)
            | (Paused, Cancelled)
            | (CancelRequested, Cancelled)
    )
}

/// TaskRegistry: tạo/transition task, dedupe theo key (§169), snapshot list.

/// Giới hạn history in-memory (Batch 01 — bounded task history): vượt cap →
/// dọn task terminal cũ nhất để tasks/queue không tích lũy vô hạn.
const MAX_TASK_HISTORY: usize = 512;
pub struct TaskRegistry {
    tasks: RwLock<HashMap<String, Task>>,
    queue: Mutex<Vec<String>>,
    dedupe: RwLock<HashMap<String, String>>,
}

impl Default for TaskRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskRegistry {
    pub fn new() -> Self {
        Self {
            tasks: RwLock::new(HashMap::new()),
            queue: Mutex::new(Vec::new()),
            dedupe: RwLock::new(HashMap::new()),
        }
    }

    /// Tạo task. Nếu đang có task active cùng dedupe_key → trả task cũ (attach) (§169).
    pub fn spawn(
        &self,
        kind: impl Into<String>,
        priority: TaskPriority,
        dedupe_key: Option<String>,
    ) -> CoreResult<Task> {
        if let Some(key) = &dedupe_key {
            let dedupe = self.dedupe.read();
            if let Some(existing_id) = dedupe.get(key) {
                let tasks = self.tasks.read();
                if let Some(existing) = tasks.get(existing_id) {
                    if !existing.state.is_terminal() {
                        return Ok(existing.clone());
                    }
                }
            }
        }

        let task = Task::new(kind, priority, dedupe_key.clone());
        {
            let mut tasks = self.tasks.write();
            tasks.insert(task.id.clone(), task.clone());
            evict_over_cap(&mut tasks, &self.queue);
        }
        self.queue.lock().push(task.id.clone());
        if let Some(key) = dedupe_key {
            self.dedupe.write().insert(key, task.id.clone());
        }
        Ok(task)
    }

    fn transition(&self, task_id: &str, to: TaskState) -> CoreResult<Task> {
        let mut tasks = self.tasks.write();
        let task = tasks
            .get_mut(task_id)
            .ok_or_else(|| CoreError::TaskNotFound(task_id.to_string()))?;

        if !transition_allowed(task.state, to) {
            return Err(CoreError::InvalidTransition {
                task_id: task_id.to_string(),
                from: state_name(task.state),
                to: state_name(to),
            });
        }

        task.state = to;
        match to {
            TaskState::Running => task.started_at_ms = Some(now_ms()),
            TaskState::Completed | TaskState::Failed | TaskState::Cancelled => {
                task.finished_at_ms = Some(now_ms());
                if let Some(key) = &task.dedupe_key {
                    drop_lock(&self.dedupe, key, task_id);
                }
            }
            _ => {}
        }
        Ok(task.clone())
    }

    pub fn start(&self, task_id: &str) -> CoreResult<Task> {
        self.transition(task_id, TaskState::Running)
    }

    pub fn complete(&self, task_id: &str) -> CoreResult<Task> {
        self.transition(task_id, TaskState::Completed)
    }

    pub fn fail(&self, task_id: &str, message: impl Into<String>) -> CoreResult<Task> {
        // F-03 — transition trước, rồi set message và clone KẾT QUẢ cuối cùng.
        // Bản cũ clone ngay lúc transition (chưa có message) rồi mới ghi message
        // vào store → caller nhận Task không có failure message.
        self.transition(task_id, TaskState::Failed)?;
        let mut tasks = self.tasks.write();
        let task = tasks
            .get_mut(task_id)
            .ok_or_else(|| CoreError::TaskNotFound(task_id.to_string()))?;
        task.message = Some(message.into());
        Ok(task.clone())
    }

    pub fn request_cancel(&self, task_id: &str) -> CoreResult<Task> {
        let tasks_read = self.tasks.read();
        let state = tasks_read
            .get(task_id)
            .ok_or_else(|| CoreError::TaskNotFound(task_id.to_string()))?
            .state;
        drop(tasks_read);
        // Queued → huỷ thẳng; Running → qua CancelRequested
        if state == TaskState::Queued {
            self.transition(task_id, TaskState::Cancelled)
        } else {
            self.transition(task_id, TaskState::CancelRequested)
        }
    }

    pub fn confirm_cancelled(&self, task_id: &str) -> CoreResult<Task> {
        self.transition(task_id, TaskState::Cancelled)
    }

    pub fn set_progress(&self, task_id: &str, progress: f64, message: Option<String>) -> CoreResult<Task> {
        let mut tasks = self.tasks.write();
        let task = tasks
            .get_mut(task_id)
            .ok_or_else(|| CoreError::TaskNotFound(task_id.to_string()))?;
        task.progress = progress.clamp(0.0, 1.0);
        task.message = message;
        Ok(task.clone())
    }

    pub fn get(&self, task_id: &str) -> CoreResult<Task> {
        self.tasks
            .read()
            .get(task_id)
            .cloned()
            .ok_or_else(|| CoreError::TaskNotFound(task_id.to_string()))
    }

    /// Snapshot các task đang active (không terminal).
    pub fn active(&self) -> Vec<Task> {
        let tasks = self.tasks.read();
        let mut list: Vec<Task> = tasks
            .values()
            .filter(|t| !t.state.is_terminal())
            .cloned()
            .collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
    }

    /// Dọn task terminal cũ hơn `older_than_ms`.
    pub fn prune_finished(&self, older_than_ms: u64) -> usize {
        let now = now_ms();
        let mut tasks = self.tasks.write();
        let before = tasks.len();
        let mut removed: Vec<String> = Vec::new();
        tasks.retain(|id, t| {
            let kill = t.state.is_terminal()
                && t.finished_at_ms
                    .is_some_and(|ts| now.saturating_sub(ts) >= older_than_ms);
            if kill {
                removed.push(id.clone());
            }
            !kill
        });
        // Queue không được giữ id của task đã xoá (lifecycle Batch 01).
        if !removed.is_empty() {
            let mut queue = self.queue.lock();
            queue.retain(|id| !removed.contains(id));
        }
        before - tasks.len()
    }
}

/// Dọn task terminal cũ nhất khi vượt [`MAX_TASK_HISTORY`] + gỡ id khỏi queue.
/// Gọi trong khi đang giữ `tasks` write lock; tự lấy `queue` lock theo thứ tự
/// tasks → queue (không chỗ nào lấy ngược).
fn evict_over_cap(tasks: &mut HashMap<String, Task>, queue: &Mutex<Vec<String>>) {
    if tasks.len() <= MAX_TASK_HISTORY {
        return;
    }
    let mut finished: Vec<(u64, String)> = tasks
        .iter()
        .filter(|(_, t)| t.state.is_terminal())
        .map(|(id, t)| (t.finished_at_ms.unwrap_or(0), id.clone()))
        .collect();
    finished.sort_unstable();
    let mut excess = tasks.len().saturating_sub(MAX_TASK_HISTORY);
    let mut evicted: Vec<String> = Vec::new();
    for (_, id) in finished {
        if excess == 0 {
            break;
        }
        tasks.remove(&id);
        evicted.push(id);
        excess -= 1;
    }
    if !evicted.is_empty() {
        let mut queue = queue.lock();
        queue.retain(|id| !evicted.contains(id));
    }
}

fn drop_lock(dedupe: &RwLock<HashMap<String, String>>, key: &str, task_id: &str) {
    let mut dedupe = dedupe.write();
    if dedupe.get(key).map(String::as_str) == Some(task_id) {
        dedupe.remove(key);
    }
}

fn state_name(state: TaskState) -> &'static str {
    match state {
        TaskState::Queued => "QUEUED",
        TaskState::Running => "RUNNING",
        TaskState::Paused => "PAUSED",
        TaskState::CancelRequested => "CANCEL_REQUESTED",
        TaskState::Cancelled => "CANCELLED",
        TaskState::Failed => "FAILED",
        TaskState::Completed => "COMPLETED",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_start_complete_happy_path() {
        let registry = TaskRegistry::new();
        let task = registry.spawn("download", TaskPriority::P1UserAction, None).expect("spawn");
        assert_eq!(task.state, TaskState::Queued);

        let started = registry.start(&task.id).expect("start");
        assert_eq!(started.state, TaskState::Running);
        assert!(started.started_at_ms.is_some());

        let done = registry.complete(&task.id).expect("complete");
        assert_eq!(done.state, TaskState::Completed);
        assert!(done.finished_at_ms.is_some());
    }

    #[test]
    fn invalid_transition_rejected() {
        let registry = TaskRegistry::new();
        let task = registry.spawn("download", TaskPriority::P1UserAction, None).expect("spawn");
        // Queued → Completed không hợp lệ
        assert!(registry.complete(&task.id).is_err());
    }

    #[test]
    fn dedupe_attaches_existing_active_task() {
        let registry = TaskRegistry::new();
        let first = registry
            .spawn("download", TaskPriority::P1UserAction, Some("file:x.zip".into()))
            .expect("spawn");
        let second = registry
            .spawn("download", TaskPriority::P1UserAction, Some("file:x.zip".into()))
            .expect("spawn");
        assert_eq!(first.id, second.id, "second spawn phải attach vào task cũ");
    }

    #[test]
    fn dedupe_released_after_terminal() {
        let registry = TaskRegistry::new();
        let first = registry
            .spawn("download", TaskPriority::P1UserAction, Some("file:y.zip".into()))
            .expect("spawn");
        registry.start(&first.id).expect("start");
        registry.complete(&first.id).expect("complete");

        let second = registry
            .spawn("download", TaskPriority::P1UserAction, Some("file:y.zip".into()))
            .expect("spawn");
        assert_ne!(first.id, second.id);
    }

    #[test]
    fn cancel_from_running_goes_through_cancel_requested() {
        let registry = TaskRegistry::new();
        let task = registry.spawn("install", TaskPriority::P1UserAction, None).expect("spawn");
        registry.start(&task.id).expect("start");
        let requested = registry.request_cancel(&task.id).expect("cancel");
        assert_eq!(requested.state, TaskState::CancelRequested);
        let cancelled = registry.confirm_cancelled(&task.id).expect("confirm");
        assert_eq!(cancelled.state, TaskState::Cancelled);
    }

    #[test]
    fn cancel_from_queued_is_immediate() {
        let registry = TaskRegistry::new();
        let task = registry.spawn("prefetch", TaskPriority::P3Prefetch, None).expect("spawn");
        let cancelled = registry.request_cancel(&task.id).expect("cancel");
        assert_eq!(cancelled.state, TaskState::Cancelled);
    }

    #[test]
    fn progress_clamped() {
        let registry = TaskRegistry::new();
        let task = registry.spawn("scan", TaskPriority::P2Background, None).expect("spawn");
        registry.start(&task.id).expect("start");
        let updated = registry.set_progress(&task.id, 1.7, None).expect("progress");
        assert!((updated.progress - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn fail_returns_and_stores_failure_message() {
        // F-03 regression — cả returned lẫn stored Task phải có message.
        let registry = TaskRegistry::new();
        let task = registry.spawn("download", TaskPriority::P1UserAction, None).expect("spawn");
        registry.start(&task.id).expect("start");

        let failed = registry.fail(&task.id, "boom").expect("fail");
        assert_eq!(failed.message.as_deref(), Some("boom"));
        assert_eq!(failed.state, TaskState::Failed);

        let stored = registry.get(&task.id).expect("stored");
        assert_eq!(stored.message.as_deref(), Some("boom"));
    }

    #[test]
    fn task_history_evicts_terminal_at_cap() {
        let registry = TaskRegistry::new();
        let mut ids = Vec::new();
        for i in 0..(MAX_TASK_HISTORY + 10) {
            let t = registry
                .spawn(format!("k{i}"), TaskPriority::P3Prefetch, None)
                .expect("spawn");
            registry.start(&t.id).expect("start");
            registry.complete(&t.id).expect("complete");
            ids.push(t.id);
        }
        // History bị cap: không quá MAX_TASK_HISTORY, ít nhất 10 task cũ bị dọn.
        // (Không assert theo id cụ thể — timestamp có thể trùng millisecond,
        // tie-break theo id string không ổn định giữa các lần chạy.)
        let tasks_map = registry.tasks.read();
        assert!(
            tasks_map.len() <= MAX_TASK_HISTORY,
            "history phải ≤ cap: {}",
            tasks_map.len()
        );
        let evicted: Vec<String> = ids
            .iter()
            .filter(|id| !tasks_map.contains_key(*id))
            .cloned()
            .collect();
        assert!(evicted.len() >= 10, "cần dọn ≥10 task cũ: {}", evicted.len());
        assert!(tasks_map.contains_key(ids.last().unwrap()), "task mới nhất phải còn");
        drop(tasks_map);
        // queue không giữ id đã evict
        let queue = registry.queue.lock();
        for id in &evicted {
            assert!(!queue.contains(id), "queue còn id đã evict: {id}");
        }
        assert!(queue.contains(ids.last().unwrap()));
    }

    #[test]
    fn prune_finished_removes_old_tasks() {
        let registry = TaskRegistry::new();
        let task = registry.spawn("old", TaskPriority::P2Background, None).expect("spawn");
        registry.start(&task.id).expect("start");
        registry.complete(&task.id).expect("complete");
        let removed = registry.prune_finished(0);
        assert_eq!(removed, 1);
        assert!(registry.get(&task.id).is_err());
    }
}
