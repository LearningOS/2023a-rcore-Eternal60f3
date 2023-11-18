//! Process management syscalls
use alloc::sync::Arc;

use crate::{
    config::{MAX_SYSCALL_NUM, PAGE_SIZE},
    loader::get_app_data_by_name,
    mm::{translated_refmut, translated_str, VirtPageNum, VirtAddr, StepByOne, },
    task::{
        add_task, current_task, current_user_token, exit_current_and_run_next,
        suspend_current_and_run_next, TaskStatus, is_map_vpn_current, add_maparea,
        remove_mem,  curr_translate_refmut, get_current_running_time, get_current_syscalls_cnt,
    },
    timer::get_time_us, syscall::{CH5_SYSCALL_CNT, TONG_MAP_SYSCALL},
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// Task information
#[allow(dead_code)]
pub struct TaskInfo {
    /// Task status in it's life cycle
    status: TaskStatus,
    /// The numbers of syscall called by task
    syscall_times: [u32; MAX_SYSCALL_NUM],
    /// Total running time of task
    time: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(exit_code: i32) -> ! {
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel:pid[{}] sys_yield", current_task().unwrap().pid.0);
    suspend_current_and_run_next();
    0
}

pub fn sys_getpid() -> isize {
    trace!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    current_task().unwrap().pid.0 as isize
}

pub fn sys_fork() -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}

pub fn sys_exec(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(data) = get_app_data_by_name(path.as_str()) {
        let task = current_task().unwrap();
        task.exec(data);
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    trace!("kernel::pid[{}] sys_waitpid [{}]", current_task().unwrap().pid.0, pid);
    let task = current_task().unwrap();
    // find a child process

    // ---- access current PCB exclusively
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after being removed from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB automatically
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_get_time",
        current_task().unwrap().pid.0
    );

    let us = get_time_us();
    let ts_ref = curr_translate_refmut(ts);
    ts_ref.sec = us / 1_000_000;
    ts_ref.usec = us % 1_000_000;
    0
}

/// YOUR JOB: Finish sys_task_info to pass testcases
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ?
pub fn sys_task_info(ti: *mut TaskInfo) -> isize {
    trace!(
        "kernel:pid[{}] sys_task_info",
        current_task().unwrap().pid.0
    );
    
    let ti_ref = curr_translate_refmut(ti);
    ti_ref.time = get_current_running_time();
    ti_ref.status = TaskStatus::Running;
    let tong_syscalls_cnt = get_current_syscalls_cnt();
    for id in 0..CH5_SYSCALL_CNT {
        ti_ref.syscall_times[TONG_MAP_SYSCALL[id]] = tong_syscalls_cnt[id] as u32;
    }
    0
}

/// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, port: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_mmap",
        current_task().unwrap().pid.0
    );
    
    if start % PAGE_SIZE != 0 || (port & !0x7) != 0 || (port & 0x7) == 0 {
        return -1;
    }

    let start_va: VirtAddr = start.into();
    let end_va: VirtAddr = (start + len).into();
    
    let start_vpn: VirtPageNum = start_va.into();
    let end_vpn: VirtPageNum = end_va.ceil().into();
    let mut curr_vpn:VirtPageNum = start_vpn;
    loop {
        if curr_vpn == end_vpn {
            break;
        }
        if is_map_vpn_current(curr_vpn) {
            return -1;
        }
        curr_vpn.step();
    }

    add_maparea(start_va, end_va, port);
    // 对于物理内存不足的情况，就直接靠系统panic
    0
}

/// YOUR JOB: Implement munmap.
///     当前写法存在问题，只有当要删除的这段内存恰好和之前分配的某一段MapArea匹配时才会删除
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_munmap",
        current_task().unwrap().pid.0
    );

    if start % PAGE_SIZE != 0 {
        return -1;
    }

    let start_va: VirtAddr = start.into();
    let end_va: VirtAddr = (start + len).into();
    
    let start_vpn: VirtPageNum = start_va.into();
    let end_vpn: VirtPageNum = end_va.ceil().into();
    let mut curr_vpn:VirtPageNum = start_vpn;
    loop {
        if curr_vpn == end_vpn {
            break;
        }
        if !is_map_vpn_current(curr_vpn) {
            return -1;
        }
        curr_vpn.step();
    }

    remove_mem(start_va, end_va)
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

/// YOUR JOB: Implement spawn.
/// HINT: fork + exec =/= spawn
pub fn sys_spawn(path: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_spawn",
        current_task().unwrap().pid.0
    );
    
    let curr_task = current_task().unwrap();
    let new_task = curr_task.fork();
    let new_pid = &new_task.pid;

    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    trap_cx.x[10] = 0;
    
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(data) = get_app_data_by_name(path.as_str()) {
        new_task.exec(data);
        new_pid.0 as isize
    } else {
        -1
    }
}

// YOUR JOB: Set task priority.
pub fn sys_set_priority(prio: isize) -> isize {
    trace!(
        "kernel:pid[{}] sys_set_priority",
        current_task().unwrap().pid.0
    );

    if prio < 2 || prio > 16 {
        return -1;
    }
    
    let task = current_task().unwrap();
    let mut task_inner = task.inner_exclusive_access();
    task_inner.prio_level = prio as u8;
    prio
}
