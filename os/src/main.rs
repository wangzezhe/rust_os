#![no_std]
#![no_main]
#![feature(panic_info_message)]

use core::arch::global_asm;
#[macro_use]
mod console;
mod lang_items;
mod sbi;

global_asm!(include_str!("entry.asm"));

pub fn clear_bss(){
    extern "C"{//引用外部c函数接口
        fn sbss();//把位置标志转换成usize获取地址
        fn ebss();
    }
    (sbss as usize..ebss as usize).for_each(|a|{
        unsafe{(a as *mut u8).write_volatile(0)}//bss段中的一个地址转换成一个裸指针，并且指针指向的只修改为零
    });
}

#[no_mangle]
pub fn rust_main() -> !{
    clear_bss();//内核初始化清空bss段
    println!("Hello,world");
    panic!("Shutdown machine");
}



