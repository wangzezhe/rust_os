//从其他内核模块的视角看来，物理页帧分配的接口是调用 frame_alloc 函数得到一个 FrameTracker （如果物理内存还有剩余）


// os内核能够以物理页帧为单位分配和回收物理内存
use super::{PhysAddr, PhysPageNum};
use alloc::vec::Vec;
use crate::sync::UPSafeCell;
//可用物理内存区间的左端点是ekernel，区间的右端点是MEMORY_END
use crate::config::MEMORY_END;
use lazy_static::*;
use core::fmt::{self, Debug, Formatter};

//借用了 RAII 的思想，将一个物理页帧的生命周期绑定到一个 FrameTracker 变量上，
//当一个 FrameTracker 被创建的时候，我们需要从 FRAME_ALLOCATOR 中分配一个物理页帧(物理页号)
pub struct FrameTracker {
    pub ppn: PhysPageNum,
}

impl FrameTracker {
    pub fn new(ppn: PhysPageNum) -> Self {
        let bytes_array = ppn.get_bytes_array();
        for i in bytes_array {
            *i = 0;
        }
        Self { ppn }
    }
}


impl Debug for FrameTracker {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("FrameTracker:PPN={:#x}", self.ppn.0))
    }
}

//当一个 FrameTracker 生命周期结束被编译器回收的时候，我们需要将它控制的物理页帧回收到 FRAME_ALLOCATOR
impl Drop for FrameTracker {
    fn drop(&mut self) {
        frame_dealloc(self.ppn);
    }
}

//物理页帧管理器需要提供的功能，以物理页号为单位进行分配和回收
trait FrameAllocator {
    fn new() -> Self;
    fn alloc(&mut self) -> Option<PhysPageNum>; //分配
    fn dealloc(&mut self, ppn: PhysPageNum); //释放
}

//栈式物理页帧管理策略
pub struct StackFrameAllocator {
    //物理页号区间 [ current , end ) 此前均 从未 被分配出去过，而向量 recycled 以后入先出的方式保存了被回收的物理页号
    current: usize, //空闲内存的起始物理页号
    end: usize, //结束物理页号
    recycled: Vec<usize>, //后入先出的方式保存被回收的物理页号
}

impl StackFrameAllocator {
    pub fn init(&mut self, l: PhysPageNum, r: PhysPageNum) {
        self.current = l.0;
        self.end = r.0;
    }
}

impl FrameAllocator for StackFrameAllocator {
    //初始化
    //而在它真正被使用起来之前，需要调用 init 方法将自身[current,end)初始化为可用物理页号区间
    fn new() -> Self {
        Self {
            current: 0,
            end: 0,
            recycled: Vec::new(),
        }
    }
    //栈 recycled 内有没有之前回收的物理页号，如果有的话直接弹出栈顶并返回
    fn alloc(&mut self) -> Option<PhysPageNum> {
        //首先会检查栈 recycled 内有没有之前回收的物理页号，如果有的话直接弹出栈顶并返回
        if let Some(ppn) = self.recycled.pop() {
            Some(ppn.into())
        } else {
            //从之前从未分配过的物理页号区间 [ current , end ) 上进行分配
            if self.current == self.end {
                None
            } else {
                self.current += 1;
                Some((self.current - 1).into())
            }
        }
    }
    //回收需要检查回收页面的合法性，然后将其压入 recycled 栈中
    fn dealloc(&mut self, ppn: PhysPageNum) {
        let ppn = ppn.0;
        //判断待回收的页面是否合法
        /*该页面之前一定被分配出去过，因此它的物理页号一定 < current ；
        该页面没有正处在回收状态，即它的物理页号不能在栈 recycled 中找到。*/
        if ppn >= self.current || self.recycled
            .iter()
            .find(|&v| {*v == ppn})
            .is_some() {
            panic!("Frame ppn={:#x} has not been allocated!", ppn);
        }
        self.recycled.push(ppn);
    }
}

type FrameAllocatorImpl = StackFrameAllocator;

lazy_static! {
    //使用 UPSafeCell<T> 来包裹栈式物理页帧分配器。每次对该分配器进行操作之前，我们都需要先通过 FRAME_ALLOCATOR.exclusive_access() 拿到分配器的可变借用。
    pub static ref FRAME_ALLOCATOR: UPSafeCell<FrameAllocatorImpl> = unsafe {
        UPSafeCell::new(FrameAllocatorImpl::new())
    };
}

//物理帧的分配不占用内核内存区
//正式分配物理页帧之前，我们需要将物理页帧全局管理器 FRAME_ALLOCATOR 初始化
pub fn init_frame_allocator() {
    extern "C" {
        fn ekernel();
    }
    FRAME_ALLOCATOR
        .exclusive_access()
        .init(PhysAddr::from(ekernel as usize).ceil(), PhysAddr::from(MEMORY_END).floor());
}

//公开给其他内核模块调用的分配/回收物理页帧的接口
pub fn frame_alloc() -> Option<FrameTracker> {
    FRAME_ALLOCATOR
        .exclusive_access()
        .alloc()
        .map(|ppn| FrameTracker::new(ppn))
}

pub fn frame_dealloc(ppn: PhysPageNum) {
    FRAME_ALLOCATOR
        .exclusive_access()
        .dealloc(ppn);
}

#[allow(unused)]
pub fn frame_allocator_test() {
    let mut v: Vec<FrameTracker> = Vec::new();
    for i in 0..5 {
        let frame = frame_alloc().unwrap();
        println!("{:?}", frame);
        v.push(frame);
    }
    v.clear();
    for i in 0..5 {
        let frame = frame_alloc().unwrap();
        println!("{:?}", frame);
        v.push(frame);
    }
    drop(v);
    println!("frame_allocator_test passed!");
}