// 能在虚拟内存中以各种粒度大小来动态分配内存资源(堆)
use buddy_system_allocator::LockedHeap;
use crate::config::KERNEL_HEAP_SIZE;

//实例化成一个全局变量
//LockedHeap 是一个用于线程安全的堆分配器的类
#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap = LockedHeap::empty();

//堆分配出错的情况
#[alloc_error_handler]
pub fn handle_alloc_error(layout: core::alloc::Layout) -> ! {
    panic!("Heap allocation error, layout = {:?}", layout);
}

static mut HEAP_SPACE: [u8; KERNEL_HEAP_SIZE] = [0; KERNEL_HEAP_SIZE];


//使用任何 alloc 中提供的堆数据结构之前，我们需要先调用 init_heap 函数来给我们的全局分配器一块内存用于分配
pub fn init_heap() {
    unsafe {
        HEAP_ALLOCATOR
            .lock() // 加锁线程可以防止其他线程同时对它进行操作导致数据竞争
            .init(HEAP_SPACE.as_ptr() as usize, KERNEL_HEAP_SIZE); //指定堆的起始地址和大小
    }
}
