pub const USER_STACK_SIZE: usize = 4096 * 2;
pub const KERNEL_STACK_SIZE: usize = 4096 * 2;
pub const KERNEL_HEAP_SIZE: usize = 0x30_0000;
pub const PAGE_SIZE: usize = 0x1000;
pub const PAGE_SIZE_BITS: usize = 0xc;

pub const TRAMPOLINE: usize = usize::MAX - PAGE_SIZE + 1;
pub const TRAP_CONTEXT: usize = TRAMPOLINE - PAGE_SIZE;
pub const CLOCK_FREQ: usize = 12500000;
//需要知道物理内存的哪一部分是可用的
//整块物理内存的终止物理地址（可用内存大小实际为8MB，原因：与开发板保持一致）
pub const MEMORY_END: usize = 0x8800_0000;

//内存映射 I/O (MMIO, Memory-Mapped I/O) 指的是外设的设备寄存器可以通过特定的物理内存地址来访问，每个外设的设备寄存器都分布在没有交集的一个或数个物理地址区间中，
//不同外设的设备寄存器所占的物理地址空间也不会产生交集，且这些外设物理地址区间也不会和RAM的物理内存所在的区间存在交集
pub const MMIO: &[(usize, usize)] = &[
    (0x0010_0000, 0x00_2000), 
    (0x1000_1000, 0x00_1000),
];
pub type BlockDeviceImpl = crate::drivers::block::VirtIOBlock;