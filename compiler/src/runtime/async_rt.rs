// ===============================================================================
// QUANTALANG ASYNC RUNTIME
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Async runtime with work-stealing scheduler for QuantaLang.
//!
//! This module provides the async/await runtime infrastructure including:
//! - Work-stealing task scheduler
//! - Future and Task representations
//! - Executor and worker thread management
//! - Timer and I/O integration points
//!
//! ## Architecture
//!
//! The runtime uses a work-stealing scheduler where each worker thread has its
//! own local queue of tasks. When a worker runs out of work, it attempts to
//! steal tasks from other workers' queues.
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                         Executor                                 │
//! │  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐            │
//! │  │ Worker0 │  │ Worker1 │  │ Worker2 │  │ Worker3 │  ...       │
//! │  │ [Queue] │←→│ [Queue] │←→│ [Queue] │←→│ [Queue] │            │
//! │  └─────────┘  └─────────┘  └─────────┘  └─────────┘            │
//! │       ↓            ↓            ↓            ↓                  │
//! │  ┌─────────────────────────────────────────────────────────┐   │
//! │  │                    Global Queue                          │   │
//! │  └─────────────────────────────────────────────────────────┘   │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

// =============================================================================
// TASK STATE
// =============================================================================

/// Task state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TaskState {
    /// Task is ready to run.
    Ready = 0,
    /// Task is currently running.
    Running = 1,
    /// Task is waiting on I/O or timer.
    Waiting = 2,
    /// Task has completed successfully.
    Completed = 3,
    /// Task was cancelled.
    Cancelled = 4,
    /// Task panicked.
    Panicked = 5,
}

/// Poll result from a future.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Poll<T> {
    /// Future is ready with a value.
    Ready(T),
    /// Future is not ready yet.
    Pending,
}

impl<T> Poll<T> {
    pub fn is_ready(&self) -> bool {
        matches!(self, Poll::Ready(_))
    }

    pub fn is_pending(&self) -> bool {
        matches!(self, Poll::Pending)
    }

    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Poll<U> {
        match self {
            Poll::Ready(t) => Poll::Ready(f(t)),
            Poll::Pending => Poll::Pending,
        }
    }
}

// =============================================================================
// WAKER
// =============================================================================

/// Waker for notifying the runtime that a task is ready.
#[derive(Clone)]
pub struct Waker {
    /// Task ID to wake.
    task_id: u64,
    /// Reference to executor for scheduling.
    executor: Arc<ExecutorInner>,
}

impl Waker {
    /// Create a new waker for a task.
    pub fn new(task_id: u64, executor: Arc<ExecutorInner>) -> Self {
        Self { task_id, executor }
    }

    /// Wake the task, scheduling it for execution.
    pub fn wake(&self) {
        self.executor.wake_task(self.task_id);
    }

    /// Wake by reference without consuming.
    pub fn wake_by_ref(&self) {
        self.wake();
    }
}

/// Context passed to futures when polling.
pub struct Context<'a> {
    waker: &'a Waker,
}

impl<'a> Context<'a> {
    pub fn new(waker: &'a Waker) -> Self {
        Self { waker }
    }

    pub fn waker(&self) -> &Waker {
        self.waker
    }
}

// =============================================================================
// FUTURE TRAIT (Simplified for codegen)
// =============================================================================

/// Simplified future trait for code generation.
///
/// This is a simplified version that works with the codegen infrastructure.
/// Real futures would use trait objects or monomorphization.
pub trait Future {
    type Output;

    /// Poll the future, returning Ready if complete or Pending if not.
    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<Self::Output>;
}

// =============================================================================
// TASK
// =============================================================================

/// Unique task identifier.
pub type TaskId = u64;

/// A task in the runtime.
///
/// Tasks wrap futures and provide the machinery for scheduling and execution.
pub struct Task {
    /// Unique task ID.
    pub id: TaskId,
    /// Current state.
    pub state: AtomicUsize,
    /// Priority (lower = higher priority).
    pub priority: u32,
    /// The future to poll (type-erased).
    /// In real implementation, this would be a trait object or union.
    pub future_ptr: *mut (),
    /// Drop function for the future.
    pub drop_fn: Option<fn(*mut ())>,
    /// Poll function for the future.
    pub poll_fn: Option<fn(*mut (), &mut Context<'_>) -> Poll<()>>,
    /// Completion callback.
    pub on_complete: Option<fn(TaskId)>,
    /// Parent task for structured concurrency.
    pub parent: Option<TaskId>,
    /// Child tasks.
    pub children: Mutex<Vec<TaskId>>,
    /// Creation time.
    pub created_at: Instant,
    /// Join handle waiters.
    pub waiters: Mutex<Vec<Waker>>,
}

impl Task {
    /// Create a new task.
    pub fn new(id: TaskId, priority: u32) -> Self {
        Self {
            id,
            state: AtomicUsize::new(TaskState::Ready as usize),
            priority,
            future_ptr: std::ptr::null_mut(),
            drop_fn: None,
            poll_fn: None,
            on_complete: None,
            parent: None,
            children: Mutex::new(Vec::new()),
            created_at: Instant::now(),
            waiters: Mutex::new(Vec::new()),
        }
    }

    /// Get current state.
    pub fn get_state(&self) -> TaskState {
        match self.state.load(Ordering::Acquire) {
            0 => TaskState::Ready,
            1 => TaskState::Running,
            2 => TaskState::Waiting,
            3 => TaskState::Completed,
            4 => TaskState::Cancelled,
            _ => TaskState::Panicked,
        }
    }

    /// Set task state.
    pub fn set_state(&self, state: TaskState) {
        self.state.store(state as usize, Ordering::Release);
    }

    /// Poll the task's future.
    pub fn poll(&self, cx: &mut Context<'_>) -> Poll<()> {
        if let Some(poll_fn) = self.poll_fn {
            poll_fn(self.future_ptr, cx)
        } else {
            Poll::Ready(())
        }
    }

    /// Add a child task.
    pub fn add_child(&self, child_id: TaskId) {
        // Mutex poisoning only occurs on thread panic
        self.children.lock().unwrap().push(child_id);
    }

    /// Notify waiters that task completed.
    pub fn notify_waiters(&self) {
        // Mutex poisoning only occurs on thread panic
        let waiters = std::mem::take(&mut *self.waiters.lock().unwrap());
        for waker in waiters {
            waker.wake();
        }
    }
}

impl Drop for Task {
    fn drop(&mut self) {
        if let Some(drop_fn) = self.drop_fn {
            if !self.future_ptr.is_null() {
                drop_fn(self.future_ptr);
            }
        }
    }
}

// Safety: Task is Send if the future is Send
unsafe impl Send for Task {}
unsafe impl Sync for Task {}

// =============================================================================
// WORK-STEALING DEQUE
// =============================================================================

/// A work-stealing deque for tasks.
///
/// Owners push/pop from one end, stealers steal from the other.
pub struct WorkStealingDeque {
    /// The actual queue.
    queue: Mutex<VecDeque<Arc<Task>>>,
    /// Number of items (for fast checking).
    len: AtomicUsize,
}

impl WorkStealingDeque {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            len: AtomicUsize::new(0),
        }
    }

    /// Push a task to the back (owner operation).
    pub fn push(&self, task: Arc<Task>) {
        // Mutex poisoning only occurs on thread panic
        let mut queue = self.queue.lock().unwrap();
        queue.push_back(task);
        self.len.fetch_add(1, Ordering::Release);
    }

    /// Pop a task from the back (owner operation).
    pub fn pop(&self) -> Option<Arc<Task>> {
        // Mutex poisoning only occurs on thread panic
        let mut queue = self.queue.lock().unwrap();
        if let Some(task) = queue.pop_back() {
            self.len.fetch_sub(1, Ordering::Release);
            Some(task)
        } else {
            None
        }
    }

    /// Steal a task from the front (stealer operation).
    pub fn steal(&self) -> Option<Arc<Task>> {
        // Mutex poisoning only occurs on thread panic
        let mut queue = self.queue.lock().unwrap();
        if let Some(task) = queue.pop_front() {
            self.len.fetch_sub(1, Ordering::Release);
            Some(task)
        } else {
            None
        }
    }

    /// Steal half the tasks (batch stealing).
    pub fn steal_batch(&self, max: usize) -> Vec<Arc<Task>> {
        // Mutex poisoning only occurs on thread panic
        let mut queue = self.queue.lock().unwrap();
        let steal_count = std::cmp::min(queue.len() / 2, max);
        let mut stolen = Vec::with_capacity(steal_count);
        for _ in 0..steal_count {
            if let Some(task) = queue.pop_front() {
                stolen.push(task);
            }
        }
        self.len.fetch_sub(stolen.len(), Ordering::Release);
        stolen
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.len.load(Ordering::Acquire) == 0
    }

    /// Get length.
    pub fn len(&self) -> usize {
        self.len.load(Ordering::Acquire)
    }
}

impl Default for WorkStealingDeque {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// WORKER
// =============================================================================

/// Worker thread state.
pub struct Worker {
    /// Worker ID.
    pub id: usize,
    /// Local task queue.
    pub local_queue: WorkStealingDeque,
    /// Tasks executed count.
    pub tasks_executed: AtomicU64,
    /// Tasks stolen count.
    pub tasks_stolen: AtomicU64,
    /// Is this worker active?
    pub active: AtomicBool,
    /// Random state for stealing.
    rng_state: AtomicU64,
}

impl Worker {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            local_queue: WorkStealingDeque::new(),
            tasks_executed: AtomicU64::new(0),
            tasks_stolen: AtomicU64::new(0),
            active: AtomicBool::new(true),
            rng_state: AtomicU64::new(id as u64 ^ 0x517cc1b727220a95),
        }
    }

    /// Simple xorshift64 PRNG for victim selection.
    fn next_random(&self) -> u64 {
        let mut x = self.rng_state.load(Ordering::Relaxed);
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng_state.store(x, Ordering::Relaxed);
        x
    }

    /// Select a random victim worker for stealing.
    pub fn select_victim(&self, num_workers: usize) -> usize {
        let mut victim = (self.next_random() as usize) % num_workers;
        // Don't steal from self
        if victim == self.id {
            victim = (victim + 1) % num_workers;
        }
        victim
    }
}

// =============================================================================
// EXECUTOR INNER
// =============================================================================

/// Shared executor state.
pub struct ExecutorInner {
    /// All workers.
    pub workers: Vec<Arc<Worker>>,
    /// Global task queue for overflow/new tasks.
    pub global_queue: Mutex<VecDeque<Arc<Task>>>,
    /// All tasks by ID.
    pub tasks: Mutex<std::collections::HashMap<TaskId, Arc<Task>>>,
    /// Next task ID.
    pub next_task_id: AtomicU64,
    /// Is the executor running?
    pub running: AtomicBool,
    /// Shutdown signal.
    pub shutdown: AtomicBool,
    /// Condition variable for parking workers.
    pub park_condvar: Condvar,
    /// Mutex for parking.
    pub park_mutex: Mutex<()>,
    /// Number of active (non-parked) workers.
    pub active_workers: AtomicUsize,
}

impl ExecutorInner {
    /// Wake a task by ID, moving it to ready state and scheduling it.
    pub fn wake_task(&self, task_id: TaskId) {
        let task = {
            // Mutex poisoning only occurs on thread panic
            let tasks = self.tasks.lock().unwrap();
            if let Some(task) = tasks.get(&task_id) {
                let state = task.get_state();
                if state == TaskState::Waiting {
                    task.set_state(TaskState::Ready);
                    Some(task.clone())
                } else {
                    None
                }
            } else {
                None
            }
        };

        // Add to global queue outside the lock
        if let Some(task) = task {
            // Mutex poisoning only occurs on thread panic
            self.global_queue.lock().unwrap().push_back(task);
            // Wake a parked worker
            self.park_condvar.notify_one();
        }
    }

    /// Allocate a new task ID.
    pub fn alloc_task_id(&self) -> TaskId {
        self.next_task_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Get task by ID.
    pub fn get_task(&self, task_id: TaskId) -> Option<Arc<Task>> {
        // Mutex poisoning only occurs on thread panic
        self.tasks.lock().unwrap().get(&task_id).cloned()
    }

    /// Unpark workers.
    pub fn unpark_workers(&self) {
        self.park_condvar.notify_all();
    }
}

// =============================================================================
// EXECUTOR
// =============================================================================

/// Configuration for the executor.
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    /// Number of worker threads.
    pub num_workers: usize,
    /// Global queue batch size when distributing.
    pub global_queue_batch: usize,
    /// Enable work stealing.
    pub work_stealing: bool,
    /// Stack size for worker threads.
    pub stack_size: usize,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            num_workers: num_cpus(),
            global_queue_batch: 32,
            work_stealing: true,
            stack_size: 2 * 1024 * 1024, // 2MB
        }
    }
}

/// Get number of CPUs (simplified).
fn num_cpus() -> usize {
    // In real implementation, use libc/winapi to get actual count
    std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4)
}

/// The async runtime executor.
pub struct Executor {
    /// Shared inner state.
    inner: Arc<ExecutorInner>,
    /// Configuration.
    config: ExecutorConfig,
}

impl Executor {
    /// Create a new executor with default config.
    pub fn new() -> Self {
        Self::with_config(ExecutorConfig::default())
    }

    /// Create with custom configuration.
    pub fn with_config(config: ExecutorConfig) -> Self {
        let workers: Vec<Arc<Worker>> = (0..config.num_workers)
            .map(|id| Arc::new(Worker::new(id)))
            .collect();

        let inner = Arc::new(ExecutorInner {
            workers,
            global_queue: Mutex::new(VecDeque::new()),
            tasks: Mutex::new(std::collections::HashMap::new()),
            next_task_id: AtomicU64::new(1),
            running: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
            park_condvar: Condvar::new(),
            park_mutex: Mutex::new(()),
            active_workers: AtomicUsize::new(0),
        });

        Self { inner, config }
    }

    /// Spawn a new task.
    pub fn spawn(&self, priority: u32) -> TaskId {
        let task_id = self.inner.alloc_task_id();
        let task = Arc::new(Task::new(task_id, priority));

        // Register task
        // Mutex poisoning only occurs on thread panic
        self.inner
            .tasks
            .lock()
            .unwrap()
            .insert(task_id, task.clone());

        // Add to global queue — Mutex poisoning only occurs on thread panic
        self.inner.global_queue.lock().unwrap().push_back(task);

        // Wake a worker if parked
        self.inner.park_condvar.notify_one();

        task_id
    }

    /// Spawn a task with a parent (structured concurrency).
    pub fn spawn_child(&self, parent_id: TaskId, priority: u32) -> Option<TaskId> {
        let parent = self.inner.get_task(parent_id)?;
        let task_id = self.spawn(priority);

        if let Some(_task) = self.inner.get_task(task_id) {
            // Link parent-child
            parent.add_child(task_id);
        }

        Some(task_id)
    }

    /// Cancel a task.
    pub fn cancel(&self, task_id: TaskId) -> bool {
        if let Some(task) = self.inner.get_task(task_id) {
            task.set_state(TaskState::Cancelled);
            // Cancel children recursively
            // Mutex poisoning only occurs on thread panic
            let children = task.children.lock().unwrap().clone();
            for child_id in children {
                self.cancel(child_id);
            }
            task.notify_waiters();
            true
        } else {
            false
        }
    }

    /// Check if a task is complete.
    pub fn is_complete(&self, task_id: TaskId) -> bool {
        self.inner
            .get_task(task_id)
            .map(|t| {
                matches!(
                    t.get_state(),
                    TaskState::Completed | TaskState::Cancelled | TaskState::Panicked
                )
            })
            .unwrap_or(true)
    }

    /// Block until a task completes.
    pub fn block_on(&self, task_id: TaskId) {
        while !self.is_complete(task_id) {
            // Run one task from global queue
            // Mutex poisoning only occurs on thread panic
            if let Some(task) = self.inner.global_queue.lock().unwrap().pop_front() {
                self.run_task(&task);
            } else {
                std::thread::yield_now();
            }
        }
    }

    /// Run a single task.
    fn run_task(&self, task: &Arc<Task>) {
        let state = task.get_state();
        if state != TaskState::Ready {
            return;
        }

        task.set_state(TaskState::Running);

        let waker = Waker::new(task.id, self.inner.clone());
        let mut cx = Context::new(&waker);

        match task.poll(&mut cx) {
            Poll::Ready(()) => {
                task.set_state(TaskState::Completed);
                if let Some(on_complete) = task.on_complete {
                    on_complete(task.id);
                }
                task.notify_waiters();
            }
            Poll::Pending => {
                task.set_state(TaskState::Waiting);
            }
        }
    }

    /// Get executor statistics.
    pub fn stats(&self) -> ExecutorStats {
        let mut total_executed = 0;
        let mut total_stolen = 0;
        let mut queue_lengths = Vec::new();

        for worker in &self.inner.workers {
            total_executed += worker.tasks_executed.load(Ordering::Relaxed);
            total_stolen += worker.tasks_stolen.load(Ordering::Relaxed);
            queue_lengths.push(worker.local_queue.len());
        }

        ExecutorStats {
            total_tasks_executed: total_executed,
            total_tasks_stolen: total_stolen,
            // Mutex poisoning only occurs on thread panic
            global_queue_len: self.inner.global_queue.lock().unwrap().len(),
            worker_queue_lengths: queue_lengths,
            active_workers: self.inner.active_workers.load(Ordering::Relaxed),
        }
    }

    /// Shutdown the executor.
    pub fn shutdown(&self) {
        self.inner.shutdown.store(true, Ordering::Release);
        self.inner.running.store(false, Ordering::Release);
        self.inner.unpark_workers();
    }

    /// Get the inner executor state.
    pub fn inner(&self) -> &Arc<ExecutorInner> {
        &self.inner
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

/// Executor statistics.
#[derive(Debug, Clone)]
pub struct ExecutorStats {
    pub total_tasks_executed: u64,
    pub total_tasks_stolen: u64,
    pub global_queue_len: usize,
    pub worker_queue_lengths: Vec<usize>,
    pub active_workers: usize,
}

// =============================================================================
// TIMER
// =============================================================================

/// A timer entry.
#[derive(Debug)]
pub struct TimerEntry {
    /// When the timer fires.
    pub deadline: Instant,
    /// Task to wake.
    pub task_id: TaskId,
    /// Was this timer cancelled?
    pub cancelled: AtomicBool,
}

/// Timer wheel for managing timeouts.
pub struct TimerWheel {
    /// Entries sorted by deadline.
    entries: Mutex<Vec<Arc<TimerEntry>>>,
    /// Executor reference for waking tasks.
    executor: Arc<ExecutorInner>,
}

impl TimerWheel {
    pub fn new(executor: Arc<ExecutorInner>) -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
            executor,
        }
    }

    /// Register a timer.
    pub fn register(&self, deadline: Instant, task_id: TaskId) -> Arc<TimerEntry> {
        let entry = Arc::new(TimerEntry {
            deadline,
            task_id,
            cancelled: AtomicBool::new(false),
        });

        // Mutex poisoning only occurs on thread panic
        let mut entries = self.entries.lock().unwrap();
        // Insert sorted by deadline
        let pos = entries
            .binary_search_by(|e| e.deadline.cmp(&deadline))
            .unwrap_or_else(|e| e);
        entries.insert(pos, entry.clone());

        entry
    }

    /// Register a timer with duration from now.
    pub fn register_delay(&self, delay: Duration, task_id: TaskId) -> Arc<TimerEntry> {
        self.register(Instant::now() + delay, task_id)
    }

    /// Cancel a timer.
    pub fn cancel(&self, entry: &TimerEntry) {
        entry.cancelled.store(true, Ordering::Release);
    }

    /// Process expired timers, waking their tasks.
    pub fn process(&self) -> Option<Duration> {
        let now = Instant::now();
        // Mutex poisoning only occurs on thread panic
        let mut entries = self.entries.lock().unwrap();

        // Wake all expired, non-cancelled timers
        while let Some(entry) = entries.first() {
            if entry.deadline <= now {
                let entry = entries.remove(0);
                if !entry.cancelled.load(Ordering::Acquire) {
                    self.executor.wake_task(entry.task_id);
                }
            } else {
                // Return time until next timer
                return Some(entry.deadline - now);
            }
        }

        None
    }
}

// =============================================================================
// CHANNEL (for task communication)
// =============================================================================

/// A bounded multi-producer multi-consumer channel.
pub struct Channel<T> {
    /// The buffer.
    buffer: Mutex<VecDeque<T>>,
    /// Capacity.
    capacity: usize,
    /// Waiting senders.
    send_waiters: Mutex<Vec<Waker>>,
    /// Waiting receivers.
    recv_waiters: Mutex<Vec<Waker>>,
    /// Is the channel closed?
    closed: AtomicBool,
}

impl<T> Channel<T> {
    pub fn new(capacity: usize) -> Arc<Self> {
        Arc::new(Self {
            buffer: Mutex::new(VecDeque::with_capacity(capacity)),
            capacity,
            send_waiters: Mutex::new(Vec::new()),
            recv_waiters: Mutex::new(Vec::new()),
            closed: AtomicBool::new(false),
        })
    }

    /// Try to send a value.
    pub fn try_send(&self, value: T) -> Result<(), ChannelError<T>> {
        if self.closed.load(Ordering::Acquire) {
            return Err(ChannelError::Closed(value));
        }

        // Mutex poisoning only occurs on thread panic
        let mut buffer = self.buffer.lock().unwrap();
        if buffer.len() >= self.capacity {
            Err(ChannelError::Full(value))
        } else {
            buffer.push_back(value);
            // Wake one receiver
            if let Some(waker) = self.recv_waiters.lock().unwrap().pop() {
                waker.wake();
            }
            Ok(())
        }
    }

    /// Try to receive a value.
    pub fn try_recv(&self) -> Result<T, ChannelError<()>> {
        // Mutex poisoning only occurs on thread panic
        let mut buffer = self.buffer.lock().unwrap();
        if let Some(value) = buffer.pop_front() {
            // Wake one sender
            if let Some(waker) = self.send_waiters.lock().unwrap().pop() {
                waker.wake();
            }
            Ok(value)
        } else if self.closed.load(Ordering::Acquire) {
            Err(ChannelError::Closed(()))
        } else {
            Err(ChannelError::Empty(()))
        }
    }

    /// Register to wait for send.
    pub fn register_send_wait(&self, waker: Waker) {
        // Mutex poisoning only occurs on thread panic
        self.send_waiters.lock().unwrap().push(waker);
    }

    /// Register to wait for receive.
    pub fn register_recv_wait(&self, waker: Waker) {
        // Mutex poisoning only occurs on thread panic
        self.recv_waiters.lock().unwrap().push(waker);
    }

    /// Close the channel.
    pub fn close(&self) {
        self.closed.store(true, Ordering::Release);
        // Wake all waiters
        // Mutex poisoning only occurs on thread panic
        for waker in self.send_waiters.lock().unwrap().drain(..) {
            waker.wake();
        }
        for waker in self.recv_waiters.lock().unwrap().drain(..) {
            waker.wake();
        }
    }

    /// Check if closed.
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    /// Get current length.
    pub fn len(&self) -> usize {
        // Mutex poisoning only occurs on thread panic
        self.buffer.lock().unwrap().len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Channel operation error.
#[derive(Debug)]
pub enum ChannelError<T> {
    /// Channel is full.
    Full(T),
    /// Channel is empty.
    Empty(T),
    /// Channel is closed.
    Closed(T),
}

// =============================================================================
// SEMAPHORE
// =============================================================================

/// An async semaphore for limiting concurrency.
pub struct Semaphore {
    /// Available permits.
    permits: AtomicUsize,
    /// Maximum permits.
    max_permits: usize,
    /// Waiting acquirers.
    waiters: Mutex<Vec<Waker>>,
}

impl Semaphore {
    pub fn new(permits: usize) -> Self {
        Self {
            permits: AtomicUsize::new(permits),
            max_permits: permits,
            waiters: Mutex::new(Vec::new()),
        }
    }

    /// Try to acquire a permit.
    pub fn try_acquire(&self) -> bool {
        loop {
            let current = self.permits.load(Ordering::Acquire);
            if current == 0 {
                return false;
            }
            if self
                .permits
                .compare_exchange_weak(current, current - 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return true;
            }
        }
    }

    /// Register to wait for a permit.
    pub fn register_wait(&self, waker: Waker) {
        // Mutex poisoning only occurs on thread panic
        self.waiters.lock().unwrap().push(waker);
    }

    /// Release a permit.
    pub fn release(&self) {
        let prev = self.permits.fetch_add(1, Ordering::AcqRel);
        debug_assert!(prev < self.max_permits, "released more than acquired");

        // Wake one waiter
        // Mutex poisoning only occurs on thread panic
        if let Some(waker) = self.waiters.lock().unwrap().pop() {
            waker.wake();
        }
    }

    /// Get available permits.
    pub fn available(&self) -> usize {
        self.permits.load(Ordering::Acquire)
    }
}

// =============================================================================
// ONESHOT CHANNEL
// =============================================================================

/// A single-use channel for one value.
pub struct Oneshot<T> {
    /// The value, if set.
    value: Mutex<Option<T>>,
    /// Has the value been set?
    completed: AtomicBool,
    /// Waiter for the receiver.
    waiter: Mutex<Option<Waker>>,
}

impl<T> Oneshot<T> {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            value: Mutex::new(None),
            completed: AtomicBool::new(false),
            waiter: Mutex::new(None),
        })
    }

    /// Send a value (can only be called once).
    pub fn send(&self, value: T) -> Result<(), T> {
        if self.completed.load(Ordering::Acquire) {
            return Err(value);
        }

        // Mutex poisoning only occurs on thread panic
        *self.value.lock().unwrap() = Some(value);
        self.completed.store(true, Ordering::Release);

        if let Some(waker) = self.waiter.lock().unwrap().take() {
            waker.wake();
        }

        Ok(())
    }

    /// Try to receive the value.
    pub fn try_recv(&self) -> Option<T> {
        if self.completed.load(Ordering::Acquire) {
            // Mutex poisoning only occurs on thread panic
            self.value.lock().unwrap().take()
        } else {
            None
        }
    }

    /// Register to wait for the value.
    pub fn register_wait(&self, waker: Waker) {
        // Mutex poisoning only occurs on thread panic
        *self.waiter.lock().unwrap() = Some(waker);
    }

    /// Check if completed.
    pub fn is_completed(&self) -> bool {
        self.completed.load(Ordering::Acquire)
    }
}

impl<T> Default for Oneshot<T> {
    fn default() -> Self {
        Self {
            value: Mutex::new(None),
            completed: AtomicBool::new(false),
            waiter: Mutex::new(None),
        }
    }
}

// =============================================================================
// JOIN HANDLE
// =============================================================================

/// Handle for waiting on a task to complete.
pub struct JoinHandle {
    /// Task ID.
    task_id: TaskId,
    /// Executor reference.
    executor: Arc<ExecutorInner>,
}

impl JoinHandle {
    pub fn new(task_id: TaskId, executor: Arc<ExecutorInner>) -> Self {
        Self { task_id, executor }
    }

    /// Check if the task is complete.
    pub fn is_finished(&self) -> bool {
        self.executor
            .get_task(self.task_id)
            .map(|t| {
                matches!(
                    t.get_state(),
                    TaskState::Completed | TaskState::Cancelled | TaskState::Panicked
                )
            })
            .unwrap_or(true)
    }

    /// Register to be woken when task completes.
    pub fn register_wait(&self, waker: Waker) {
        if let Some(task) = self.executor.get_task(self.task_id) {
            // Mutex poisoning only occurs on thread panic
            task.waiters.lock().unwrap().push(waker);
        }
    }

    /// Get the task ID.
    pub fn task_id(&self) -> TaskId {
        self.task_id
    }
}

// =============================================================================
// CODEGEN SUPPORT
// =============================================================================

/// Metadata for async function code generation.
#[derive(Debug, Clone)]
pub struct AsyncFnMetadata {
    /// Function name.
    pub name: String,
    /// Number of await points.
    pub await_points: usize,
    /// State size in bytes.
    pub state_size: usize,
    /// Captured variable types.
    pub captures: Vec<String>,
}

/// State machine state for an async function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsyncState {
    /// Initial state, start execution.
    Start,
    /// At an await point (1-indexed).
    AwaitPoint(u32),
    /// Function completed.
    Complete,
}

impl AsyncState {
    /// State value representing Start.
    pub const START_VALUE: u32 = 0;
    /// State value representing Complete.
    pub const COMPLETE_VALUE: u32 = u32::MAX;

    /// Convert from a u32 state value.
    pub fn from_u32(v: u32) -> Self {
        match v {
            Self::START_VALUE => AsyncState::Start,
            Self::COMPLETE_VALUE => AsyncState::Complete,
            n => AsyncState::AwaitPoint(n),
        }
    }

    /// Convert to a u32 state value.
    pub fn to_u32(self) -> u32 {
        match self {
            AsyncState::Start => Self::START_VALUE,
            AsyncState::Complete => Self::COMPLETE_VALUE,
            AsyncState::AwaitPoint(n) => n,
        }
    }
}

/// Generate async state machine struct layout.
pub fn async_state_machine_layout(
    name: &str,
    state_fields: &[(String, String)], // (name, type)
) -> String {
    let mut layout = format!("struct {}State {{\n", name);
    layout.push_str("    __state: u32,\n");
    for (field_name, field_type) in state_fields {
        layout.push_str(&format!("    {}: {},\n", field_name, field_type));
    }
    layout.push_str("}\n");
    layout
}

// =============================================================================
// GLOBAL RUNTIME
// =============================================================================

use std::cell::RefCell;

thread_local! {
    /// Current task ID for this thread.
    static CURRENT_TASK: RefCell<Option<TaskId>> = const { RefCell::new(None) };
}

/// Get the current task ID.
pub fn current_task_id() -> Option<TaskId> {
    CURRENT_TASK.with(|t| *t.borrow())
}

/// Set the current task ID.
pub fn set_current_task_id(id: Option<TaskId>) {
    CURRENT_TASK.with(|t| *t.borrow_mut() = id);
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_work_stealing_deque() {
        let deque = WorkStealingDeque::new();
        let task1 = Arc::new(Task::new(1, 0));
        let task2 = Arc::new(Task::new(2, 0));
        let task3 = Arc::new(Task::new(3, 0));

        deque.push(task1.clone());
        deque.push(task2.clone());
        deque.push(task3.clone());

        assert_eq!(deque.len(), 3);

        // Pop gets from back (LIFO for owner)
        assert_eq!(deque.pop().unwrap().id, 3);

        // Steal gets from front (FIFO for stealers)
        assert_eq!(deque.steal().unwrap().id, 1);

        assert_eq!(deque.len(), 1);
    }

    #[test]
    fn test_executor_spawn() {
        let executor = Executor::new();
        let task_id = executor.spawn(0);

        assert!(task_id > 0);
        assert!(!executor.is_complete(task_id));
    }

    #[test]
    fn test_channel() {
        let channel: Arc<Channel<i32>> = Channel::new(2);

        assert!(channel.try_send(1).is_ok());
        assert!(channel.try_send(2).is_ok());
        assert!(matches!(channel.try_send(3), Err(ChannelError::Full(3))));

        assert_eq!(channel.try_recv().unwrap(), 1);
        assert_eq!(channel.try_recv().unwrap(), 2);
        assert!(matches!(channel.try_recv(), Err(ChannelError::Empty(()))));
    }

    #[test]
    fn test_semaphore() {
        let sem = Semaphore::new(2);

        assert!(sem.try_acquire());
        assert!(sem.try_acquire());
        assert!(!sem.try_acquire());

        sem.release();
        assert!(sem.try_acquire());
    }

    #[test]
    fn test_oneshot() {
        let oneshot: Arc<Oneshot<i32>> = Oneshot::new();

        assert!(!oneshot.is_completed());
        assert!(oneshot.try_recv().is_none());

        assert!(oneshot.send(42).is_ok());
        assert!(oneshot.is_completed());
        assert_eq!(oneshot.try_recv(), Some(42));

        // Can't receive twice
        assert!(oneshot.try_recv().is_none());
    }

    #[test]
    fn test_poll() {
        let ready: Poll<i32> = Poll::Ready(42);
        let pending: Poll<i32> = Poll::Pending;

        assert!(ready.is_ready());
        assert!(!ready.is_pending());
        assert!(!pending.is_ready());
        assert!(pending.is_pending());

        let mapped = Poll::Ready(10).map(|x| x * 2);
        assert!(matches!(mapped, Poll::Ready(20)));
    }
}
