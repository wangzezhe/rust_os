use core::arch::asm;

const SYSCALL_WRITE: usize = 64;
const SYSCALL_EXIT: usize = 93;

fn syscall(id: usize, args: [usize; 3]) -> isize {
    let mut ret: isize;
    unsafe {
        asm!(
            "ecall",//通过ecall进入s-mode（os内核模式）
            inlateout("x10") args[0] => ret,//riscv x10 x11 x12保存的都是系统调用的参数
            in("x11") args[1],
            in("x12") args[2],
            in("x17") id//x17 保存的是riscv中系统调用的参数
        );
    }
    ret
}

pub fn sys_write(fd: usize, buffer: &[u8]) -> isize {
    syscall(SYSCALL_WRITE, [fd, buffer.as_ptr() as usize, buffer.len()])
}

pub fn sys_exit(exit_code: i32) -> isize {
    syscall(SYSCALL_EXIT, [exit_code as usize, 0, 0])
}