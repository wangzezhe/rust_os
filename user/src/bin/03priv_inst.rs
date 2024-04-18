//应用程序，测试用户态执行内核态指令会不会报错并且安全返回
#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;
use core::arch::asm;
#[no_mangle]//此处为程序入口点，（没什么意义就是标准化编写）
fn main() -> i32 {
    println!("Try to execute privileged instruction in U Mode");
    println!("Kernel should kill this application!");
    unsafe{
        asm!(
            "sret"
        )
    }
    0
}