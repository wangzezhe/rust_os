mod address;
mod frame_allocator;
mod heap_allocator;
mod memory_set;
mod page_table;

use address::VPNRange;
pub use address::{PhysAddr, PhysPageNum, StepByOne, VirtAddr, VirtPageNum};
pub use frame_allocator::{frame_alloc, frame_dealloc, FrameTracker};
pub use memory_set::remap_test;
pub use memory_set::{kernel_token, MapPermission, MemorySet, KERNEL_SPACE};
use page_table::PTEFlags;
pub use page_table::{
    translated_byte_buffer, translated_ref, translated_refmut, translated_str, PageTable,
    PageTableEntry, UserBuffer, UserBufferIterator,
};

//内存管理子系统的初始化
pub fn init() {
    //全局动态内存分配器的初始化
    heap_allocator::init_heap();
    //初始化物理页帧管理器使能可用物理页帧的分配
    frame_allocator::init_frame_allocator();
    //创建内核地址空间并让 CPU 开启分页模式， MMU 在地址转换的时候使用内核的多级页表，
    KERNEL_SPACE.exclusive_access().activate();
}