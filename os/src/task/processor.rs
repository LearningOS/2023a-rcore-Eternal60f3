//!Implementation of [`Processor`] and Intersection of control flow
//!
//! Here, the continuous operation of user apps in CPU is maintained,
//! the current running state of CPU is recorded,
//! and the replacement and transfer of control flow of different applications are executed.

use super::__switch;
use super::{fetch_task, TaskStatus};
use super::{TaskContext, TaskControlBlock};
use crate::mm::{VirtPageNum, is_map_vpn, MapPermission, VirtAddr, translated_refmut};
use crate::sync::UPSafeCell;
use crate::syscall::CH5_SYSCALL_CNT;
use crate::timer::get_time_us;
use crate::trap::TrapContext;
use alloc::sync::Arc;
use lazy_static::*;

/// 用于计算进程每一次运行后需要增加的pass
const BIG_STRIDER: u8 = 255;

/// Processor management structure
pub struct Processor {
    ///The task currently executing on the current processor
    current: Option<Arc<TaskControlBlock>>,

    ///The basic control flow of each core, helping to select and switch process
    idle_task_cx: TaskContext,
}

impl Processor {
    ///Create an empty Processor
    pub fn new() -> Self {
        Self {
            current: None,
            idle_task_cx: TaskContext::zero_init(),
        }
    }

    ///Get mutable reference to `idle_task_cx`
    fn get_idle_task_cx_ptr(&mut self) -> *mut TaskContext {
        &mut self.idle_task_cx as *mut _
    }

    ///Get current task in moving semanteme
    pub fn take_current(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.current.take()
    }

    ///Get current task in cloning semanteme
    pub fn current(&self) -> Option<Arc<TaskControlBlock>> {
        self.current.as_ref().map(Arc::clone)
    }
}

lazy_static! {
    pub static ref PROCESSOR: UPSafeCell<Processor> = unsafe { UPSafeCell::new(Processor::new()) };
}

///The main part of process execution and scheduling
///Loop `fetch_task` to get the process that needs to run, and switch the process through `__switch`
pub fn run_tasks() {
    loop {
        let mut processor = PROCESSOR.exclusive_access();
        if let Some(task) = fetch_task() {
            let idle_task_cx_ptr = processor.get_idle_task_cx_ptr();
            // access coming task TCB exclusively
            let mut task_inner = task.inner_exclusive_access();
            let next_task_cx_ptr = &task_inner.task_cx as *const TaskContext;
            task_inner.task_status = TaskStatus::Running;
            task_inner.stride += BIG_STRIDER / task_inner.prio_level;
            if task_inner.start_time < 0 {
                task_inner.start_time = get_time_us() as isize;
            }
            // release coming task_inner manually
            drop(task_inner);
            // release coming task TCB manually
            processor.current = Some(task);
            // release processor manually
            drop(processor);
            unsafe {
                __switch(idle_task_cx_ptr, next_task_cx_ptr);
            }
        } else {
            warn!("no tasks available in run_tasks");
        }
    }
}

/// Get current task through take, leaving a None in its place
pub fn take_current_task() -> Option<Arc<TaskControlBlock>> {
    PROCESSOR.exclusive_access().take_current()
}

/// Get a copy of the current task
pub fn current_task() -> Option<Arc<TaskControlBlock>> {
    PROCESSOR.exclusive_access().current()
}

/// Get the current user token(addr of page table)
pub fn current_user_token() -> usize {
    let task = current_task().unwrap();
    task.get_user_token()
}

///Get the mutable reference to trap context of current task
pub fn current_trap_cx() -> &'static mut TrapContext {
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .get_trap_cx()
}

///Return to idle control flow for new scheduling
pub fn schedule(switched_task_cx_ptr: *mut TaskContext) {
    let mut processor = PROCESSOR.exclusive_access();
    let idle_task_cx_ptr = processor.get_idle_task_cx_ptr();
    drop(processor);
    unsafe {
        __switch(switched_task_cx_ptr, idle_task_cx_ptr);
    }
}

/// 在当前进程中判断某个虚拟页面是否已经映射了
pub fn is_map_vpn_current(vpn: VirtPageNum) -> bool {
    let token = current_user_token();
    is_map_vpn(token, vpn)
}

/// 为当前进程增加一段内存(MapArea)
pub fn add_maparea(start_va: VirtAddr, end_va: VirtAddr, perm: usize) {
    let mut permission: MapPermission = MapPermission::U;
    if (perm & (1 << 0)) != 0 {
        permission.insert(MapPermission::R);
    }
    if (perm & (1 << 1)) != 0 {
        permission.insert(MapPermission::W);
    }
    if (perm & (1 << 2)) != 0 {
        permission.insert(MapPermission::X);
    }

    let curr_task = current_task().unwrap();
    let mut task_inner = curr_task.inner_exclusive_access();
    task_inner.memory_set.insert_framed_area(start_va, end_va, permission);
}

/// 删除当前进程中的一段内存
///     当前写法存在问题，只有当要删除的这段内存恰好和之前分配的某一段MapArea匹配时才会删除
pub fn remove_mem(start_va: VirtAddr, _end_va: VirtAddr) {
    let curr_task = current_task().unwrap();
    let mut task_inner = curr_task.inner_exclusive_access();
    task_inner.memory_set.remove_area_with_start_vpn(start_va.into());
}

/// 将当前进程的虚拟地址转换为物理地址
pub fn curr_translate_refmut<T>(ptr: *mut T) -> &'static mut T {
    let token = current_user_token();
    translated_refmut(token, ptr)
}

/// 获取当前进程运行时间 
pub fn get_current_running_time() -> usize {
    let curr_task = current_task().unwrap();
    let task_inner = curr_task.inner_exclusive_access();

    let now_time = get_time_us();
    (now_time - task_inner.start_time as usize + 1000 - 1) / 1000
}

/// 获取当前进程系统调用次数的桶
pub fn get_current_syscalls_cnt() -> [usize; CH5_SYSCALL_CNT] {
    let curr_task = current_task().unwrap();
    let task_inner = curr_task.inner_exclusive_access();

    task_inner.tong_syscalls_cnt
}

/// 增加当前进程当前使用的系统调用的次数 
pub fn add_current_syscall_cnt(curr_syscall_id: usize) {
    let curr_task = current_task().unwrap();
    let mut task_inner = curr_task.inner_exclusive_access();

    let pair = task_inner.tong_syscalls_cnt
        .iter()
        .enumerate()
        .find(|(_, syscall_id)| {
        **syscall_id == curr_syscall_id
    });
    if let Some((tong_id, _)) = pair {
        task_inner.tong_syscalls_cnt[tong_id] += 1;
    }
}