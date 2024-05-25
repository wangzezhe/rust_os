use crate::fs::{open_file, OpenFlags};
use crate::mm::{translated_refmut, translated_str};
use crate::task::{
    add_task, current_task, current_user_token, exit_current_and_run_next,
    suspend_current_and_run_next,
};
use crate::timer::get_time_ms;
use alloc::sync::Arc;

pub fn sys_exit(exit_code: i32) -> ! {
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

//应用调用 sys_yield 主动交出使用权
pub fn sys_yield() -> isize {
    suspend_current_and_run_next();
    0
}

pub fn sys_get_time() -> isize {
    get_time_ms() as isize
}

pub fn sys_getpid() -> isize {
    current_task().unwrap().pid.0 as isize
}

//功能：当前进程 fork 出来一个子进程。
//返回值：对于子进程返回 0，对于当前进程则返回子进程的 PID 。
//syscall ID：220
//注意如何体现父子进程的差异
pub fn sys_fork() -> isize {
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    trap_cx.x[10] = 0;
    //生成的子进程通过 add_task 加入到任务管理器中
    add_task(new_task);
    new_pid as isize
}


//仅有 fork 的话，那么所有的进程都只能和用户初始进程一样执行同样的代码段
//引入 exec 系统调用来执行不同的可执行文件
//功能：将当前进程的地址空间清空并加载一个特定的可执行文件，返回用户态后开始它的执行。
//参数：path 给出了要加载的可执行文件的名字；
//返回值：如果出错的话（如找不到名字相符的可执行文件）则返回 -1，否则不应该返回。
//syscall ID：221
//利用 fork 和 exec 的组合，我们很容易在一个进程内 fork 出一个子进程并执行一个特定的可执行文件
pub fn sys_exec(path: *const u8) -> isize {
    let token = current_user_token();
    let path = translated_str(token, path);
    //从文件系统中获取
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let task = current_task().unwrap();
        task.exec(all_data.as_slice());
        0
    } else {
        -1
    }
}

//功能：当前进程等待一个子进程变为僵尸进程，回收其全部资源并收集其返回值。
//参数：pid 表示要等待的子进程的进程 ID，如果为 -1 的话表示等待任意一个子进程；
//exit_code 表示保存子进程返回值的地址，如果这个地址为 0 的话表示不必保存。
//返回值：如果要等待的子进程不存在则返回 -1；否则如果要等待的子进程均未结束则返回 -2；
//否则返回结束的子进程的进程 ID。
//syscall ID：260
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())

    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        let exit_code = child.inner_exclusive_access().exit_code;
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
}