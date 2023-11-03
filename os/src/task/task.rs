//! Types related to task management

use super::TaskContext;
use crate::config::CH2_SYSCALL_NUM;

/// The task control block (TCB) of a task.
#[derive(Copy, Clone)]
pub struct TaskControlBlock {
    /// The task status in it's lifecycle
    pub task_status: TaskStatus,
    /// The task context
    pub task_cx: TaskContext,
    /// 该任务的开始时间
    pub start_time: isize,
    /// 该任务所调用的不同syscall的次数
    pub syscalls_cnt: [u32; CH2_SYSCALL_NUM],
}

/// The status of a task
#[derive(Copy, Clone, PartialEq)]
pub enum TaskStatus {
    /// uninitialized
    UnInit,
    /// ready to run
    Ready,
    /// running
    Running,
    /// exited
    Exited,
}
