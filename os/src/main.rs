#![no_std]
#![no_main]
#![feature(panic_info_message)]

#[macro_use]
mod console;
mod config;
mod lang_items;
mod loader;
mod sbi;
mod sync;
mod timer;
pub mod syscall;
pub mod task;
pub mod trap;

use core::arch::global_asm;

global_asm!(include_str!("entry.asm"));
global_asm!(include_str!("link_app.S"));

fn clear_bss() {
    extern "C" {
        fn sbss();
        fn ebss();
    }
    unsafe {
        core::slice::from_raw_parts_mut(
            sbss as usize as *mut u8, 
            ebss as usize - sbss as usize,
        ).fill(0);
    }
}

#[no_mangle]
pub fn rust_main() -> ! {
    clear_bss();
    println!("[kernel] Hello, world!");
    trap::init();
    loader::load_apps();
    trap::enable_timer_interrupt();
    timer::set_next_trigger();
    task::run_first_task();
    panic!("unreachable in rust_main");
}