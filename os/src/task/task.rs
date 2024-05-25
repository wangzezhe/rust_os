use super::TaskContext;
use super::{pid_alloc, KernelStack, PidHandle};
use crate::config::TRAP_CONTEXT;
use crate::fs::{File, Stdin, Stdout};
use crate::mm::{MemorySet, PhysPageNum, VirtAddr, KERNEL_SPACE};
use crate::sync::UPSafeCell;
use crate::trap::{trap_handler, TrapContext};
use alloc::sync::{Arc, Weak};
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefMut;

pub struct TaskControlBlock {
    //初始化之后就不再变化的元数据：直接放在任务控制块
    pub pid: PidHandle,
    pub kernel_stack: KernelStack,
    inner: UPSafeCell<TaskControlBlockInner>,
}

pub struct TaskControlBlockInner {
    //应用地址空间中的 Trap 上下文被放在的物理页帧的物理页号
    pub trap_cx_ppn: PhysPageNum,
    //应用数据仅有可能出现在应用地址空间低于 base_size 字节的区域中
    pub base_size: usize,
    //暂停的任务的任务上下文保存在任务控制块
    pub task_cx: TaskContext,
    //维护当前进程的执行状态
    pub task_status: TaskStatus,
    //应用地址空间
    pub memory_set: MemorySet,
    //指向当前进程的父进程
    pub parent: Option<Weak<TaskControlBlock>>,
    //当前进程的所有子进程的任务控制块以 Arc 智能指针的形式保存在一个向量中，这样才能够更方便的找到它们
    pub children: Vec<Arc<TaskControlBlock>>,
    //进程调用 exit 系统调用主动退出或者执行出错由内核终止的时候，它的退出码 exit_code 会被内核保存在它的任务控制块中，并等待它的父进程通过 waitpid 回收它的资源的同时也收集它的 PID 以及退出码
    pub exit_code: i32,
    //加入文件描述符表的相应字段
    pub fd_table: Vec<Option<Arc<dyn File + Send + Sync>>>,
}

//主要是对于它内部的字段的快捷访问
impl TaskControlBlockInner {
    pub fn get_trap_cx(&self) -> &'static mut TrapContext {
        self.trap_cx_ppn.get_mut()
    }
    pub fn get_user_token(&self) -> usize {
        self.memory_set.token()
    }
    fn get_status(&self) -> TaskStatus {
        self.task_status
    }
    pub fn is_zombie(&self) -> bool {
        self.get_status() == TaskStatus::Zombie
    }
    pub fn alloc_fd(&mut self) -> usize {
        if let Some(fd) = (0..self.fd_table.len()).find(|fd| self.fd_table[*fd].is_none()) {
            fd
        } else {
            self.fd_table.push(None);
            self.fd_table.len() - 1
        }
    }
}

impl TaskControlBlock {
    //被看成一个内层 TaskControlBlockInner 的可变引用并可以对它指向的内容进行修改
    pub fn inner_exclusive_access(&self) -> RefMut<'_, TaskControlBlockInner> {
        self.inner.exclusive_access()
    }
    //用来创建一个新的进程，目前仅用于内核中手动创建唯一一个初始进程 initproc
    pub fn new(elf_data: &[u8]) -> Self {
        //解析应用的 ELF 执行文件得到应用地址空间 memory_set ，用户栈在应用地址空间中的位置 user_sp 以及应用的入口点 entry_point 
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        //手动查页表找到位于应用地址空间中新创建的Trap 上下文被实际放在哪个物理页帧上，用来做后续的初始化
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT).into())
            .unwrap()
            .ppn();
        //为该进程分配 PID 以及内核栈，并记录下内核栈在内核地址空间的位置 kernel_stack_top
        let pid_handle = pid_alloc();
        let kernel_stack = KernelStack::new(&pid_handle);
        let kernel_stack_top = kernel_stack.get_top();
        //在该进程的内核栈上压入初始化的任务上下文，使得第一次任务切换到它的时候可以跳转到 trap_return 并进入用户态开始执行
        let task_control_block = Self {
            pid: pid_handle,
            kernel_stack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    trap_cx_ppn,
                    base_size: user_sp,
                    task_cx: TaskContext::goto_trap_return(kernel_stack_top),
                    task_status: TaskStatus::Ready,
                    memory_set,
                    parent: None,
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: vec![
                        // 0 -> stdin
                        Some(Arc::new(Stdin)),
                        // 1 -> stdout
                        Some(Arc::new(Stdout)),
                        // 2 -> stderr
                        Some(Arc::new(Stdout)),
                    ],
                })
            },
        };
        //查找该应用的 Trap 上下文的内核虚地址
        let trap_cx = task_control_block.inner_exclusive_access().get_trap_cx();
        //初始化位于该进程应用地址空间中的 Trap 上下文，使得第一次进入用户态的时候时候能正确跳转到应用入口点并设置好用户栈，同时也保证在 Trap 的时候用户态能正确进入内核态
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            kernel_stack_top,
            trap_handler as usize,
        );
        task_control_block
    }
    //系统调用使得一个进程能够加载一个新应用的 ELF 可执行文件中的代码和数据替换原有的应用地址空间中的内容，并开始执行
    pub fn exec(&self, elf_data: &[u8]) {
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT).into())
            .unwrap()
            .ppn();
        let mut inner = self.inner_exclusive_access();
        //从 ELF 文件生成一个全新的地址空间并直接替换进来，这将导致原有的地址空间生命周期结束，里面包含的全部物理页帧都会被回收
        inner.memory_set = memory_set;
        inner.trap_cx_ppn = trap_cx_ppn;
        let trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            self.kernel_stack.get_top(),
            trap_handler as usize,
        );
        *inner.get_trap_cx() = trap_cx;
    }

    //用来实现 fork 系统调用，即当前进程 fork 出来一个与之几乎相同的子进程
    //基本上和新建进程控制块的 TaskControlBlock::new 是相同子进程的地址空间不是通过解析 ELF 文件，而是通过在第 8 行调用 MemorySet::from_existed_user 复制父进程地址空间得到的
    pub fn fork(self: &Arc<TaskControlBlock>) -> Arc<TaskControlBlock> {
        let mut parent_inner = self.inner_exclusive_access();
        let memory_set = MemorySet::from_existed_user(&parent_inner.memory_set);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT).into())
            .unwrap()
            .ppn();
        let pid_handle = pid_alloc();
        let kernel_stack = KernelStack::new(&pid_handle);
        let kernel_stack_top = kernel_stack.get_top();
        let mut new_fd_table: Vec<Option<Arc<dyn File + Send + Sync>>> = Vec::new();
        for fd in parent_inner.fd_table.iter() {
            if let Some(file) = fd {
                new_fd_table.push(Some(file.clone()));
            } else {
                new_fd_table.push(None);
            }
        }
        let task_control_block = Arc::new(TaskControlBlock {
            pid: pid_handle,
            kernel_stack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    trap_cx_ppn,
                    //应用数据的大小保持一致
                    base_size: parent_inner.base_size,
                    task_cx: TaskContext::goto_trap_return(kernel_stack_top),
                    task_status: TaskStatus::Ready,
                    memory_set,
                    //在 fork 的时候需要注意父子进程关系的维护
                    parent: Some(Arc::downgrade(self)),
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: new_fd_table,
                })
            },
        });
        parent_inner.children.push(task_control_block.clone());
        let trap_cx = task_control_block.inner_exclusive_access().get_trap_cx();
        trap_cx.kernel_sp = kernel_stack_top;
        task_control_block
    }
    pub fn getpid(&self) -> usize {
        self.pid.0
    }
}

#[derive(Copy, Clone, PartialEq)]
pub enum TaskStatus {
    Ready,
    Running,
    Zombie,
}