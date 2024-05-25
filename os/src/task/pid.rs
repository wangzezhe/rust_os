use crate::config::{KERNEL_STACK_SIZE, PAGE_SIZE, TRAMPOLINE};
use crate::mm::{MapPermission, VirtAddr, KERNEL_SPACE};
use crate::sync::UPSafeCell;
use alloc::vec::Vec;
use lazy_static::*;

//实现一个同样使用简单栈式分配策略的进程标识符分配器 PidAllocator
pub struct PidAllocator {
    current: usize,
    recycled: Vec<usize>,
}

impl PidAllocator {
    pub fn new() -> Self {
        PidAllocator {
            current: 0,
            recycled: Vec::new(),
        }
    }
    //分配出去一个将 usize 包装之后的 PidHandle
    pub fn alloc(&mut self) -> PidHandle {
        if let Some(pid) = self.recycled.pop() {
            PidHandle(pid)
        } else {
            self.current += 1;
            PidHandle(self.current - 1)
        }
    }
    pub fn dealloc(&mut self, pid: usize) {
        assert!(pid < self.current);
        assert!(
            !self.recycled.iter().any(|ppid| *ppid == pid),
            "pid {} has been deallocated!",
            pid
        );
        self.recycled.push(pid);
    }
}

lazy_static! {
    pub static ref PID_ALLOCATOR: UPSafeCell<PidAllocator> =
        unsafe { UPSafeCell::new(PidAllocator::new()) };
}

pub struct PidHandle(pub usize);

impl Drop for PidHandle {
    //需要为 PidHandle 实现 Drop Trait 来允许编译器进行自动的资源回收
    fn drop(&mut self) {
        PID_ALLOCATOR.exclusive_access().dealloc(self.0);
    }
}

pub fn pid_alloc() -> PidHandle {
    PID_ALLOCATOR.exclusive_access().alloc()
}

pub fn kernel_stack_position(app_id: usize) -> (usize, usize) {
    let top = TRAMPOLINE - app_id * (KERNEL_STACK_SIZE + PAGE_SIZE);
    let bottom = top - KERNEL_STACK_SIZE;
    (bottom, top)
}

//保存着它所属进程的 PID
pub struct KernelStack {
    pid: usize,
}

impl KernelStack {
    // new 方法可以从一个 PidHandle ，也就是一个已分配的进程标识符中对应生成一个内核栈 KernelStack 
    pub fn new(pid_handle: &PidHandle) -> Self {
        let pid = pid_handle.0;
        let (kernel_stack_bottom, kernel_stack_top) = kernel_stack_position(pid);
        KERNEL_SPACE.exclusive_access().insert_framed_area(
            kernel_stack_bottom.into(),
            kernel_stack_top.into(),
            MapPermission::R | MapPermission::W,
        );
        KernelStack { pid: pid_handle.0 }
    }
    //将一个类型为 T 的变量压入内核栈顶并返回其裸指针，这也是一个泛型函数
    #[allow(unused)]
    pub fn push_on_top<T>(&self, value: T) -> *mut T
    where
        T: Sized,
    {
        let kernel_stack_top = self.get_top();
        let ptr_mut = (kernel_stack_top - core::mem::size_of::<T>()) as *mut T;
        unsafe {
            *ptr_mut = value;
        }
        ptr_mut
    }
    //获取当前内核栈顶在内核地址空间中的地址
    pub fn get_top(&self) -> usize {
        let (_, kernel_stack_top) = kernel_stack_position(self.pid);
        kernel_stack_top
    }
}

//实际保存它的物理页帧的生命周期与它绑定在一起，当 KernelStack 生命周期结束后，这些物理页帧也将会被编译器自动回收
//生命周期结束则在内核地址空间中将对应的逻辑段删除
impl Drop for KernelStack {
    fn drop(&mut self) {
        let (kernel_stack_bottom, _) = kernel_stack_position(self.pid);
        let kernel_stack_bottom_va: VirtAddr = kernel_stack_bottom.into();
        KERNEL_SPACE
            .exclusive_access()
            .remove_area_with_start_vpn(kernel_stack_bottom_va.into());
    }
}