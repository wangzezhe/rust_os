use super::File;
use crate::drivers::BLOCK_DEVICE;
use crate::mm::UserBuffer;
use crate::sync::UPSafeCell;
use alloc::sync::Arc;
use alloc::vec::Vec;
use bitflags::*;
use easy_fs::{EasyFileSystem, Inode};
use lazy_static::*;
pub struct OSInode{
    readable:bool,
    writable:bool,
    inner:UPSafeCell<OSInodeInner>,
}

pub struct OSInodeInner{
    offset:usize,
    inode:Arc<Inode>,
}

//进程中也存在着一个文件读写的当前偏移量，它也随着文件读写的进行而被不断更新。
//这些用户视角中的文件系统抽象特征需要内核来实现，与进程有很大的关系，而 easy-fs 文件系统不必涉及这些与进程结合紧密的属性。因此，我们需要将 easy-fs 提供的 Inode 加上上述信息，进一步封装为 OS 中的索引节点 OSInode
impl OSInode{
    pub fn new(readable:bool,writable:bool,inode:Arc<Inode>) -> Self{
        Self{
            readable,
            writable,
            inner:unsafe{UPSafeCell::new(OSInodeInner{offset:0,inode})},
        }
    }
    //该文件的数据全部读到一个向量 all_data 中
    pub fn read_all(&self) -> Vec<u8> {
        let mut inner = self.inner.exclusive_access();
        let mut buffer = [0u8;512];
        let mut v:Vec<u8> = Vec::new();
        loop {
            let len = inner.inode.read_at(inner.offset,&mut buffer);
            if len == 0 {
                break;
            }
            inner.offset += len;
            v.extend_from_slice(&buffer[..len]);
        }
        v
    }
}


//从块设备 BLOCK_DEVICE 上打开文件系统；从文件系统中获取根目录的 inode 。
lazy_static!{
    pub static ref ROOT_INODE:Arc<Inode> = {
        let efs = EasyFileSystem::open(BLOCK_DEVICE.clone());
        Arc::new(EasyFileSystem::root_inode(&efs))
    };
}

//列举文件系统中可用的应用的文件名
pub fn list_apps(){
    println!("/**** APPS ****");
    for app in ROOT_INODE.ls(){
        println!("{}", app);
    }
    println!("**************/");
}

//在内核中也定义一份打开文件的标志 OpenFlags
bitflags!{
    pub struct OpenFlags:u32{
        const RDONLY = 0;
        const WRONLY = 1 << 0;
        const RAWR = 1 << 1;
        const CREATE = 1 << 9;
        const TRUNC = 1 << 10;
    }
}

impl OpenFlags{
    pub fn read_write(&self) -> (bool,bool) {
        if self.is_empty(){
            (true,false)
        }else if self.contains(Self::WRONLY) {
            (false,true)
        }else{
            (true,true)
        }
    }
}

//实现 open_file 内核函数，可根据文件名打开一个根目录下的文件
pub fn open_file(name: &str, flags: OpenFlags) -> Option<Arc<OSInode>> {
    let (readable, writable) = flags.read_write();
    if flags.contains(OpenFlags::CREATE) {
        if let Some(inode) = ROOT_INODE.find(name) {
            inode.clear();
            Some(Arc::new(OSInode::new(readable, writable, inode)))
        } else {
            ROOT_INODE
                .create(name)
                .map(|inode| Arc::new(OSInode::new(readable, writable, inode)))
        }
    } else {
        ROOT_INODE.find(name).map(|inode| {
            if flags.contains(OpenFlags::TRUNC) {
                inode.clear();
            }
            Arc::new(OSInode::new(readable, writable, inode))
        })
    }
}


//在 sys_read/sys_write 的时候进行简单的访问权限检查
impl File for OSInode {
    fn readable(&self) -> bool {
        self.readable
    }
    fn writable(&self) -> bool {
        self.writable
    }
    fn read(&self, mut buf: UserBuffer) -> usize {
        let mut inner = self.inner.exclusive_access();
        let mut total_read_size = 0usize;
        for slice in buf.buffers.iter_mut() {
            let read_size = inner.inode.read_at(inner.offset, *slice);
            if read_size == 0 {
                break;
            }
            inner.offset += read_size;
            total_read_size += read_size;
        }
        total_read_size
    }
    fn write(&self, buf: UserBuffer) -> usize {
        let mut inner = self.inner.exclusive_access();
        let mut total_write_size = 0usize;
        for slice in buf.buffers.iter() {
            let write_size = inner.inode.write_at(inner.offset, *slice);
            assert_eq!(write_size, slice.len());
            inner.offset += write_size;
            total_write_size += write_size;
        }
        total_write_size
    }
}