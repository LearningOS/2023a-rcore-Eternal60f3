//! Process management syscalls
use crate::{
    config::{MAX_SYSCALL_NUM, PAGE_SIZE},
    task::{
        change_program_brk, exit_current_and_run_next, suspend_current_and_run_next, TaskStatus,
        curr_translate_refmut, get_current_running_time, get_current_syscalls_cnt, is_map_vpn_current,
        remove_mem, add_maparea,
    },
    timer::{get_time_us, get_time_ms,},
    mm::{VirtAddr, VirtPageNum, StepByOne},
};
use super::{CH4_SYSCALL_CNT, TONG_MAP_SYSCALL};

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
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ? 
/// 这里并没有解决这个问题，因为get_refmut并没有解决物理地址分页的情况
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");

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
    trace!("kernel: sys_task_info");
    
    let ti_ref = curr_translate_refmut(ti);
    ti_ref.time = get_current_running_time(get_time_ms());
    ti_ref.status = TaskStatus::Running;
    let tong_syscalls_cnt = get_current_syscalls_cnt();
    for id in 0..CH4_SYSCALL_CNT {
        ti_ref.syscall_times[TONG_MAP_SYSCALL[id]] = tong_syscalls_cnt[id] as u32;
    }
    0
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, port: usize) -> isize {
    trace!("kernel: sys_mmap");
    
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

// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!("kernel: sys_munmap");
    
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
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
