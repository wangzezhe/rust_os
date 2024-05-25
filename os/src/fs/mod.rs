//文件 (File) 就是一系列的字节组合。操作系统不关心文件内容，只关心如何对文件按字节流进行读写的机制，
//这就意味着任何程序可以读写任何文件（即字节流），对文件具体内容的解析是应用程序的任务，操作系统对此不做任何干涉
mod inode;
mod stdio;
//应用地址空间中的一段缓冲区（即内存）的抽象
use crate::mm::UserBuffer;

//有了文件这样的抽象后，操作系统内核就可把能读写并持久存储的数据按文件来进行管理，并把文件分配给进程，让进程以很简洁的统一抽象接口 File 来读写数据
pub trait File: Send + Sync {
    fn readable(&self) -> bool;
    fn writable(&self) -> bool;
    fn read(&self, buf: UserBuffer) -> usize;
    fn write(&self, buf: UserBuffer) -> usize;
}

pub use inode::{list_apps, open_file, OSInode, OpenFlags};
pub use stdio::{Stdin, Stdout};