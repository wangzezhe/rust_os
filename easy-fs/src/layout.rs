use super::{get_block_cache, BlockDevice, BLOCK_SZ};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt::{Debug, Formatter, Result};


const EFS_MAGIC: u32 = 0x3b800001;

const INODE_DIRECT_COUNT: usize = 28;

const NAME_LENGTH_LIMIT: usize = 27;

const INODE_INDIRECT1_COUNT: usize = BLOCK_SZ / 4;

const INODE_INDIRECT2_COUNT: usize = INODE_INDIRECT1_COUNT * INODE_INDIRECT1_COUNT;

const DIRECT_BOUND: usize = INODE_DIRECT_COUNT;

const INDIRECT1_BOUND: usize = DIRECT_BOUND + INODE_INDIRECT1_COUNT;

#[allow(unused)]
const INDIRECT2_BOUND: usize = INDIRECT1_BOUND + INODE_INDIRECT2_COUNT;


//存放在磁盘上编号为 0 的块的起始处
#[repr(C)]
pub struct SuperBlock {
    magic: u32,
    //给出文件系统的总块数
    pub total_blocks: u32,
    //索引节点
    pub inode_bitmap_blocks: u32,
    //索引节点区域
    pub inode_area_blocks: u32,
    //数据块位图
    pub data_bitmap_blocks: u32,
    //数据块区域
    pub data_area_blocks: u32,
}

impl Debug for SuperBlock {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.debug_struct("SuperBlock")
            .field("total_blocks", &self.total_blocks)
            .field("inode_bitmap_blocks", &self.inode_bitmap_blocks)
            .field("inode_area_blocks", &self.inode_area_blocks)
            .field("data_bitmap_blocks", &self.data_bitmap_blocks)
            .field("data_area_blocks", &self.data_area_blocks)
            .finish()
    }
}

impl SuperBlock {
    //在创建一个 easy-fs 的时候对超级块进行初始化
    pub fn initialize(
        &mut self,
        total_blocks: u32,
        inode_bitmap_blocks: u32,
        inode_area_blocks: u32,
        data_bitmap_blocks: u32,
        data_area_blocks: u32,
    ) {
        *self = Self {
            magic: EFS_MAGIC,
            total_blocks,
            inode_bitmap_blocks,
            inode_area_blocks,
            data_bitmap_blocks,
            data_area_blocks,
        }
    }
    //判断超级块所在的文件系统是否合法
    pub fn is_valid(&self) -> bool {
        self.magic == EFS_MAGIC
    }
}

#[derive(PartialEq)]
pub enum DiskInodeType {
    File,
    Directory,
}


type IndirectBlock = [u32; BLOCK_SZ / 4];

type DataBlock = [u8; BLOCK_SZ];

//磁盘上的索引节点区域
#[repr(C)]
pub struct DiskInode {
    //表示文件/目录内容的字节数
    pub size: u32,
    //存储文件内容/目录内容的数据块的索引
    //文件很小的时候，只需用到直接索引，通过直接索引可以找到 14KiB 的内容
    pub direct: [u32; INODE_DIRECT_COUNT],
    //文件比较大的时候，不仅直接索引的 direct 数组装满，还需要用到一级间接索引
    pub indirect1: u32,
    //文件大小超过直接索引和一级索引支持的容量上限 78KiB 的时候，就需要用到二级间接索引 indirect2
    pub indirect2: u32,
    //表示索引节点的类型
    type_: DiskInodeType,
}

impl DiskInode {
    //通过 initialize 方法可以初始化一个 DiskInode 为一个文件或目录
    pub fn initialize(&mut self, type_: DiskInodeType) {
        self.size = 0;
        self.direct.iter_mut().for_each(|v| *v = 0);
        self.indirect1 = 0;
        self.indirect2 = 0;
        self.type_ = type_;
    }
    //is_file 和 is_dir 两个方法可以用来确认 DiskInode 的类型为文件还是目录
    pub fn is_dir(&self) -> bool {
        self.type_ == DiskInodeType::Directory
    }
    
    #[allow(unused)]
    pub fn is_file(&self) -> bool {
        self.type_ == DiskInodeType::File
    }
    //计算为了容纳自身 size 字节的内容需要多少个数据块
    pub fn data_blocks(&self) -> u32 {
        Self::_data_blocks(self.size)
    }
    fn _data_blocks(size: u32) -> u32 {
        (size + BLOCK_SZ as u32 - 1) / BLOCK_SZ as u32
    }
    //不仅包含数据块，还需要统计索引块
    pub fn total_blocks(size: u32) -> u32 {
        //调用 data_blocks 得到需要多少数据块，再根据数据块数目所处的区间统计索引块即可
        let data_blocks = Self::_data_blocks(size) as usize;
        let mut total = data_blocks as usize;
        
        if data_blocks > INODE_DIRECT_COUNT {
            total += 1;
        }
        
        if data_blocks > INDIRECT1_BOUND {
            total += 1;
            
            total +=
                (data_blocks - INDIRECT1_BOUND + INODE_INDIRECT1_COUNT - 1) / INODE_INDIRECT1_COUNT;
        }
        total as u32
    }
    //将一个 DiskInode 的 size 扩容到 new_size 需要额外多少个数据和索引块
    pub fn blocks_num_needed(&self, new_size: u32) -> u32 {
        assert!(new_size >= self.size);
        Self::total_blocks(new_size) - Self::total_blocks(self.size)
    }
    //从索引中查到它自身用于保存文件内容的第 block_id 个数据块的块编号
    pub fn get_block_id(&self, inner_id: u32, block_device: &Arc<dyn BlockDevice>) -> u32 {
        let inner_id = inner_id as usize;
        //具体选用哪种索引方式取决于 block_id 所在的区间
        if inner_id < INODE_DIRECT_COUNT {
            //利用直接索引
            self.direct[inner_id]
        } else if inner_id < INDIRECT1_BOUND {
            //一级索引
            get_block_cache(self.indirect1 as usize, Arc::clone(block_device))
                .lock()
                .read(0, |indirect_block: &IndirectBlock| {
                    indirect_block[inner_id - INODE_DIRECT_COUNT]
                })
        } else {
            //二级索引的情况，需要先查二级索引块找到挂在它下面的一级索引块，再通过一级索引块找到数据块
            let last = inner_id - INDIRECT1_BOUND;
            let indirect1 = get_block_cache(self.indirect2 as usize, Arc::clone(block_device))
                .lock()
                .read(0, |indirect2: &IndirectBlock| {
                    indirect2[last / INODE_INDIRECT1_COUNT]
                });
            get_block_cache(indirect1 as usize, Arc::clone(block_device))
                .lock()
                .read(0, |indirect1: &IndirectBlock| {
                    indirect1[last % INODE_INDIRECT1_COUNT]
                })
        }
    }
    //
    pub fn increase_size(
        &mut self,
        new_size: u32,
        new_blocks: Vec<u32>,
        block_device: &Arc<dyn BlockDevice>,
    ) {
        let mut current_blocks = self.data_blocks();
        self.size = new_size;
        let mut total_blocks = self.data_blocks();
        let mut new_blocks = new_blocks.into_iter();
       
        while current_blocks < total_blocks.min(INODE_DIRECT_COUNT as u32) {
            self.direct[current_blocks as usize] = new_blocks.next().unwrap();
            current_blocks += 1;
        }
        
        if total_blocks > INODE_DIRECT_COUNT as u32 {
            if current_blocks == INODE_DIRECT_COUNT as u32 {
                self.indirect1 = new_blocks.next().unwrap();
            }
            current_blocks -= INODE_DIRECT_COUNT as u32;
            total_blocks -= INODE_DIRECT_COUNT as u32;
        } else {
            return;
        }
        
        get_block_cache(self.indirect1 as usize, Arc::clone(block_device))
            .lock()
            .modify(0, |indirect1: &mut IndirectBlock| {
                while current_blocks < total_blocks.min(INODE_INDIRECT1_COUNT as u32) {
                    indirect1[current_blocks as usize] = new_blocks.next().unwrap();
                    current_blocks += 1;
                }
            });
        
        if total_blocks > INODE_INDIRECT1_COUNT as u32 {
            if current_blocks == INODE_INDIRECT1_COUNT as u32 {
                self.indirect2 = new_blocks.next().unwrap();
            }
            current_blocks -= INODE_INDIRECT1_COUNT as u32;
            total_blocks -= INODE_INDIRECT1_COUNT as u32;
        } else {
            return;
        }
        
        let mut a0 = current_blocks as usize / INODE_INDIRECT1_COUNT;
        let mut b0 = current_blocks as usize % INODE_INDIRECT1_COUNT;
        let a1 = total_blocks as usize / INODE_INDIRECT1_COUNT;
        let b1 = total_blocks as usize % INODE_INDIRECT1_COUNT;
       
        get_block_cache(self.indirect2 as usize, Arc::clone(block_device))
            .lock()
            .modify(0, |indirect2: &mut IndirectBlock| {
                while (a0 < a1) || (a0 == a1 && b0 < b1) {
                    if b0 == 0 {
                        indirect2[a0] = new_blocks.next().unwrap();
                    }
                    
                    get_block_cache(indirect2[a0] as usize, Arc::clone(block_device))
                        .lock()
                        .modify(0, |indirect1: &mut IndirectBlock| {
                            indirect1[b0] = new_blocks.next().unwrap();
                        });
                    
                    b0 += 1;
                    if b0 == INODE_INDIRECT1_COUNT {
                        b0 = 0;
                        a0 += 1;
                    }
                }
            });
    }

    //会将回收的所有块的编号保存在一个向量中返回给磁盘块管理器
    pub fn clear_size(&mut self, block_device: &Arc<dyn BlockDevice>) -> Vec<u32> {
        let mut v: Vec<u32> = Vec::new();
        let mut data_blocks = self.data_blocks() as usize;
        self.size = 0;
        let mut current_blocks = 0usize;

        while current_blocks < data_blocks.min(INODE_DIRECT_COUNT) {
            v.push(self.direct[current_blocks]);
            self.direct[current_blocks] = 0;
            current_blocks += 1;
        }
        
        if data_blocks > INODE_DIRECT_COUNT {
            v.push(self.indirect1);
            data_blocks -= INODE_DIRECT_COUNT;
            current_blocks = 0;
        } else {
            return v;
        }
        
        get_block_cache(self.indirect1 as usize, Arc::clone(block_device))
            .lock()
            .modify(0, |indirect1: &mut IndirectBlock| {
                while current_blocks < data_blocks.min(INODE_INDIRECT1_COUNT) {
                    v.push(indirect1[current_blocks]);
                    current_blocks += 1;
                }
            });
        self.indirect1 = 0;
        
        if data_blocks > INODE_INDIRECT1_COUNT {
            v.push(self.indirect2);
            data_blocks -= INODE_INDIRECT1_COUNT;
        } else {
            return v;
        }
        
        assert!(data_blocks <= INODE_INDIRECT2_COUNT);
        let a1 = data_blocks / INODE_INDIRECT1_COUNT;
        let b1 = data_blocks % INODE_INDIRECT1_COUNT;
        get_block_cache(self.indirect2 as usize, Arc::clone(block_device))
            .lock()
            .modify(0, |indirect2: &mut IndirectBlock| {
                
                for entry in indirect2.iter_mut().take(a1) {
                    v.push(*entry);
                    get_block_cache(*entry as usize, Arc::clone(block_device))
                        .lock()
                        .modify(0, |indirect1: &mut IndirectBlock| {
                            for entry in indirect1.iter() {
                                v.push(*entry);
                            }
                        });
                }
                
                if b1 > 0 {
                    v.push(indirect2[a1]);
                    get_block_cache(indirect2[a1] as usize, Arc::clone(block_device))
                        .lock()
                        .modify(0, |indirect1: &mut IndirectBlock| {
                            for entry in indirect1.iter().take(b1) {
                                v.push(*entry);
                            }
                        });
                    
                }
            });
        self.indirect2 = 0;
        v
    }
    //考虑通过 DiskInode 来读写它索引的那些数据块中的数据
    pub fn read_at(
        &self,
        offset: usize,
        buf: &mut [u8],
        block_device: &Arc<dyn BlockDevice>,
    ) -> usize {
        //将文件内容从 offset 字节开始的部分读到内存中的缓冲区 buf 中，并返回实际读到的字节数
        let mut start = offset;
        let end = (offset + buf.len()).min(self.size as usize);
        if start >= end {
            return 0;
        }
        let mut start_block = start / BLOCK_SZ;
        let mut read_size = 0usize;
        loop {
            
            let mut end_current_block = (start / BLOCK_SZ + 1) * BLOCK_SZ;
            end_current_block = end_current_block.min(end);
            
            let block_read_size = end_current_block - start;
            let dst = &mut buf[read_size..read_size + block_read_size];
            get_block_cache(
                self.get_block_id(start_block as u32, block_device) as usize,
                Arc::clone(block_device),
            )
            .lock()
            .read(0, |data_block: &DataBlock| {
                let src = &data_block[start % BLOCK_SZ..start % BLOCK_SZ + block_read_size];
                dst.copy_from_slice(src);
            });
            read_size += block_read_size;
            
            if end_current_block == end {
                break;
            }
            start_block += 1;
            start = end_current_block;
        }
        read_size
    }
    //不会出现失败的情况；只要 Inode 管理的数据块的大小足够，传入的整个缓冲区的数据都必定会被写入到文件中
    pub fn write_at(
        &mut self,
        offset: usize,
        buf: &[u8],
        block_device: &Arc<dyn BlockDevice>,
    ) -> usize {
        let mut start = offset;
        let end = (offset + buf.len()).min(self.size as usize);
        assert!(start <= end);
        let mut start_block = start / BLOCK_SZ;
        let mut write_size = 0usize;
        loop {
            
            let mut end_current_block = (start / BLOCK_SZ + 1) * BLOCK_SZ;
            end_current_block = end_current_block.min(end);
           
            let block_write_size = end_current_block - start;
            get_block_cache(
                self.get_block_id(start_block as u32, block_device) as usize,
                Arc::clone(block_device),
            )
            .lock()
            .modify(0, |data_block: &mut DataBlock| {
                let src = &buf[write_size..write_size + block_write_size];
                let dst = &mut data_block[start % BLOCK_SZ..start % BLOCK_SZ + block_write_size];
                dst.copy_from_slice(src);
            });
            write_size += block_write_size;
            
            if end_current_block == end {
                break;
            }
            start_block += 1;
            start = end_current_block;
        }
        write_size
    }
}

#[repr(C)]
pub struct DirEntry {
    name: [u8; NAME_LENGTH_LIMIT + 1],
    inode_number: u32,
}

pub const DIRENT_SZ: usize = 32;

impl DirEntry {
    //通过 empty 和 new 分别生成一个空的目录项或是一个合法的目录项
    pub fn empty() -> Self {
        Self {
            name: [0u8; NAME_LENGTH_LIMIT + 1],
            inode_number: 0,
        }
    }
   
    pub fn new(name: &str, inode_number: u32) -> Self {
        let mut bytes = [0u8; NAME_LENGTH_LIMIT + 1];
        bytes[..name.len()].copy_from_slice(name.as_bytes());
        Self {
            name: bytes,
            inode_number,
        }
    }

    //在从目录的内容中读取目录项或者是将目录项写入目录的时候，我们需要将目录项转化为缓冲区（即字节切片）的形式来符合索引节点 Inode 数据结构中的 read_at 或 write_at 方法接口的要求
    pub fn as_bytes(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self as *const _ as usize as *const u8, DIRENT_SZ) }
    }
    
    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self as *mut _ as usize as *mut u8, DIRENT_SZ) }
    }
    //通过 name 和 inode_number 方法可以取出目录项中的内容
    pub fn name(&self) -> &str {
        let len = (0usize..).find(|i| self.name[*i] == 0).unwrap();
        core::str::from_utf8(&self.name[..len]).unwrap()
    }
    
    pub fn inode_number(&self) -> u32 {
        self.inode_number
    }
}