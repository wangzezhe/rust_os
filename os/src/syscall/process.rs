use crate::task::suspend_current_and_run_next;
use crate::task::exit_current_and_run_next;

//任务退出并且返回退出码，之后进行下一个应用
pub fn sys_exit(exit_code: i32) -> ! {
    println!("[kernel] Application exited with code {}", exit_code);
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit");
}

pub fn sys_yield() -> isize {
    suspend_current_and_run_next();
    0
}