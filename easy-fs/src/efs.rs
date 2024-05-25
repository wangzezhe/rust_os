//easy-fs 为简易文件系统的核心部分，它是一个库形式 crate，实现一种简单的文件系统磁盘布局
use super::{
    block_cache_sync_all, get_block_cache, Bitmap, BlockDevice, DiskInode, DiskInodeType, Inode,
    SuperBlock,
};
use crate::BLOCK_SZ;
use alloc::sync::Arc;
use spin::Mutex;
//实现 easy-fs 的整体磁盘布局，将各段区域及上面的磁盘数据结构结构整合起来就是简易文件系统 EasyFileSystem 的职责
pub struct EasyFileSystem {
    //该指针会被拷贝并传递给下层的数据结构，让它们也能够直接访问块设备
    pub block_device: Arc<dyn BlockDevice>,
    //索引节点和数据块的两个位图
    pub inode_bitmap: Bitmap,
    pub data_bitmap: Bitmap,
    inode_area_start_block: u32,
    data_area_start_block: u32,
}

type DataBlock = [u8; BLOCK_SZ];

impl EasyFileSystem {
    //通过 create 方法可以在块设备上创建并初始化一个 easy-fs 文件系统
    pub fn create(
        block_device: Arc<dyn BlockDevice>,
        total_blocks: u32,
        inode_bitmap_blocks: u32,
    ) -> Arc<Mutex<Self>> {
        //根据传入的参数计算每个区域各应该包含多少块
        //根据 inode 位图的大小计算 inode 区域至少需要多少个块才能够使得 inode 位图中的每个bit都能够有一个实际的 inode 可以对应
        let inode_bitmap = Bitmap::new(1, inode_bitmap_blocks as usize);
        let inode_num = inode_bitmap.maximum();
        let inode_area_blocks =
            ((inode_num * core::mem::size_of::<DiskInode>() + BLOCK_SZ - 1) / BLOCK_SZ) as u32;
        let inode_total_blocks = inode_bitmap_blocks + inode_area_blocks;
        let data_total_blocks = total_blocks - 1 - inode_total_blocks;
        let data_bitmap_blocks = (data_total_blocks + 4096) / 4097;
        let data_area_blocks = data_total_blocks - data_bitmap_blocks;
        let data_bitmap = Bitmap::new(
            (1 + inode_bitmap_blocks + inode_area_blocks) as usize,
            data_bitmap_blocks as usize,
        );
        //创建 EasyFileSystem 实例 efs
        let mut efs = Self {
            block_device: Arc::clone(&block_device),
            inode_bitmap,
            data_bitmap,
            inode_area_start_block: 1 + inode_bitmap_blocks,
            data_area_start_block: 1 + inode_total_blocks + data_bitmap_blocks,
        };
        //将块设备的前 total_blocks 个块清零，为初始化做准备
        for i in 0..total_blocks {
            get_block_cache(i as usize, Arc::clone(&block_device))
                .lock()
                .modify(0, |data_block: &mut DataBlock| {
                    for byte in data_block.iter_mut() {
                        *byte = 0;
                    }
                });
        }
        //位于块设备编号为 0 块上的超级块进行初始化，只需传入之前计算得到的每个区域的块数就行了
        get_block_cache(0, Arc::clone(&block_device)).lock().modify(
            0,
            |super_block: &mut SuperBlock| {
                super_block.initialize(
                    total_blocks,
                    inode_bitmap_blocks,
                    inode_area_blocks,
                    data_bitmap_blocks,
                    data_area_blocks,
                );
            },
        );
        //创建根目录，需要调用 alloc_inode 在 inode 位图中分配一个 inode ，由于这是第一次分配，它的编号固定是 0 
        assert_eq!(efs.alloc_inode(), 0);
        //将分配到的 inode 初始化为 easy-fs 中的唯一一个目录
        let (root_inode_block_id, root_inode_offset) = efs.get_disk_inode_pos(0);
        get_block_cache(root_inode_block_id as usize, Arc::clone(&block_device))
            .lock()
            .modify(root_inode_offset, |disk_inode: &mut DiskInode| {
                disk_inode.initialize(DiskInodeType::Directory);
            });
        block_cache_sync_all();
        Arc::new(Mutex::new(efs))
    }

    //通过 open 方法可以从一个已写入了 easy-fs 镜像的块设备上打开我们的 easy-fs
    pub fn open(block_device: Arc<dyn BlockDevice>) -> Arc<Mutex<Self>> {
        //只需将块设备编号为 0 的块作为超级块读取进来，就可以从中知道 easy-fs 的磁盘布局，由此可以构造 efs 实例
        get_block_cache(0, Arc::clone(&block_device))
            .lock()
            .read(0, |super_block: &SuperBlock| {
                assert!(super_block.is_valid(), "Error loading EFS!");
                let inode_total_blocks =
                    super_block.inode_bitmap_blocks + super_block.inode_area_blocks;
                let efs = Self {
                    block_device,
                    inode_bitmap: Bitmap::new(1, super_block.inode_bitmap_blocks as usize),
                    data_bitmap: Bitmap::new(
                        (1 + inode_total_blocks) as usize,
                        super_block.data_bitmap_blocks as usize,
                    ),
                    inode_area_start_block: 1 + super_block.inode_bitmap_blocks,
                    data_area_start_block: 1 + inode_total_blocks + super_block.data_bitmap_blocks,
                };
                Arc::new(Mutex::new(efs))
            })
    }
    pub fn root_inode(efs: &Arc<Mutex<Self>>) -> Inode {
        let block_device = Arc::clone(&efs.lock().block_device);
        
        let (block_id, block_offset) = efs.lock().get_disk_inode_pos(0);
        
        Inode::new(block_id, block_offset, Arc::clone(efs), block_device)
    }
    // 知道整个磁盘布局，即可以从 inode位图 或数据块位图上分配的 bit 编号，来算出各个存储inode和数据块的磁盘块在磁盘上的实际位置
    pub fn get_disk_inode_pos(&self, inode_id: u32) -> (u32, usize) {
        let inode_size = core::mem::size_of::<DiskInode>();
        let inodes_per_block = (BLOCK_SZ / inode_size) as u32;
        let block_id = self.inode_area_start_block + inode_id / inodes_per_block;
        (
            block_id,
            (inode_id % inodes_per_block) as usize * inode_size,
        )
    }
    pub fn get_data_block_id(&self, data_block_id: u32) -> u32 {
        self.data_area_start_block + data_block_id
    }

    //inode 和数据块的分配/回收也由 EasyFileSystem 负责，现在还不支持文件删除
    pub fn alloc_inode(&mut self) -> u32 {
        self.inode_bitmap.alloc(&self.block_device).unwrap() as u32
    }

    pub fn alloc_data(&mut self) -> u32 {
        self.data_bitmap.alloc(&self.block_device).unwrap() as u32 + self.data_area_start_block
    }
    
    pub fn dealloc_data(&mut self, block_id: u32) {
        get_block_cache(block_id as usize, Arc::clone(&self.block_device))
            .lock()
            .modify(0, |data_block: &mut DataBlock| {
                data_block.iter_mut().for_each(|p| {
                    *p = 0;
                })
            });
        self.data_bitmap.dealloc(
            &self.block_device,
            (block_id - self.data_area_start_block) as usize,
        )
    }
}