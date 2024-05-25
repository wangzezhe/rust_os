use super::TaskControlBlock;
use crate::sync::UPSafeCell;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use lazy_static::*;

//将所有的任务控制块用引用计数 Arc 智能指针包裹后放在一个双端队列 VecDeque 中
pub struct TaskManager {
    ready_queue: VecDeque<Arc<TaskControlBlock>>,
}


impl TaskManager {
    //使用的是普通的先进先出的队列
    pub fn new() -> Self {
        Self {
            ready_queue: VecDeque::new(),
        }
    }
    //将一个任务加入队尾
    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        self.ready_queue.push_back(task);
    }
    //表示从队头中取出一个任务来执行
    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.ready_queue.pop_front()
    }
}

lazy_static! {
    pub static ref TASK_MANAGER: UPSafeCell<TaskManager> =
        unsafe { UPSafeCell::new(TaskManager::new()) };
}

pub fn add_task(task: Arc<TaskControlBlock>) {
    TASK_MANAGER.exclusive_access().add(task);
}

pub fn fetch_task() -> Option<Arc<TaskControlBlock>> {
    TASK_MANAGER.exclusive_access().fetch()
}