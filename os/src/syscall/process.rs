use crate::batch::run_next_app;

//任务退出并且返回退出码，之后进行下一个应用
pub fn sys_exit(exit_code: i32) -> ! {
    println!("[kernel] Application exited with code {}", exit_code);
    run_next_app()
}