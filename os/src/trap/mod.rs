mod context;

use crate::config::{TRAMPOLINE, TRAP_CONTEXT};
use crate::syscall::syscall;
use crate::task::{
    current_trap_cx, current_user_token, exit_current_and_run_next, suspend_current_and_run_next,
};
use crate::timer::set_next_trigger;
use core::arch::{asm, global_asm};
use riscv::register::{
    mtvec::TrapMode,
    scause::{self, Exception, Interrupt, Trap},
    sie, stval, stvec,
};

global_asm!(include_str!("trap.S"));

pub fn init() {
    set_kernel_trap_entry();
}

fn set_kernel_trap_entry() {
    unsafe {
        stvec::write(trap_from_kernel as usize, TrapMode::Direct);
    }
}

fn set_user_trap_entry() {
    unsafe {
        stvec::write(TRAMPOLINE as usize, TrapMode::Direct);
    }
}

pub fn enable_timer_interrupt() {
    unsafe {
        sie::set_stimer();
    }
}

#[no_mangle]
pub fn trap_handler() -> ! {
    //一旦进入内核后再次触发到 S态 Trap，则硬件在设置一些 CSR 寄存器之后，会跳过对通用寄存器的保存过程，直接跳转到 trap_from_kernel 函数，在这里直接 panic 退出
    set_kernel_trap_entry();
    let cx = current_trap_cx();
    let scause = scause::read();
    let stval = stval::read();
    match scause.cause() {
        //将当前进程 Trap 上下文中的 sepc 向后移动了 4 字节，使得它回到用户态之后，会从发出系统调用的 ecall 指令的下一条指令开始执行
        Trap::Exception(Exception::UserEnvCall) => {
            cx.sepc += 4;
            //父进程系统调用的返回值会在 trap_handler 中 syscall 返回之后再设置为 sys_fork 的返回值，这里我们返回子进程的 PID 。这就做到了父进程 fork 的返回值为子进程的 PID ，而子进程的返回值则为 0 。通过返回值是否为 0 可以区分父子进程
            cx.x[10] = syscall(cx.x[17], [cx.x[10], cx.x[11], cx.x[12]]) as usize;
        }
        Trap::Exception(Exception::StoreFault) |
        Trap::Exception(Exception::StorePageFault) |
        Trap::Exception(Exception::InstructionFault) |
        Trap::Exception(Exception::InstructionPageFault) |
        Trap::Exception(Exception::LoadFault) |
        Trap::Exception(Exception::LoadPageFault) => {
            println!(
                "[kernel] {:?} in application, bad addr = {:#x}, bad instruction = {:#x}, kernel killed it.",
                scause.cause(),
                stval,
                current_trap_cx().sepc,
            );
            exit_current_and_run_next(-2);
        }
        Trap::Exception(Exception::IllegalInstruction) => {
            println!("[kernel] IllegalInstruction in application, kernel killed it.");
            exit_current_and_run_next(-3);
        }
        Trap::Interrupt(Interrupt::SupervisorTimer) => {
            set_next_trigger();
            suspend_current_and_run_next();
        }
        _ => {
            panic!(
                "Unsupported trap {:?}, stval = {:#x}!",
                scause.cause(),
                stval
            );
        }
    }
    //完成 Trap 处理之后，调用 trap_return 返回用户态
    trap_return();
}

#[no_mangle]
pub fn trap_return() -> ! {

    set_user_trap_entry();
    // Trap 上下文在应用地址空间中的虚拟地址
    let trap_cx_ptr = TRAP_CONTEXT;
    //要继续执行的应用地址空间的 token
    let user_satp = current_user_token();
    extern "C" {
        fn __alltraps();
        fn __restore();
    }
    //计算 __restore 虚地址的过程
    let restore_va = __restore as usize - __alltraps as usize + TRAMPOLINE;
    unsafe {
        asm!(
            "fence.i",
            //使用 jr 指令完成了跳转到 __restore 的任务
            "jr {restore_va}",             
            restore_va = in(reg) restore_va,
            in("a0") trap_cx_ptr,      
            in("a1") user_satp, 
            options(noreturn)
        );
    }
}

#[no_mangle]
pub fn trap_from_kernel() -> ! {
    panic!("a trap from kernel!");
}

pub use context::TrapContext;