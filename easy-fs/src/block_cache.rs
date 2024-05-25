//操作系统频繁读写速度缓慢的磁盘块会极大降低系统性能，因此常见的手段是先通过 read_block 将一个块上的数据从磁盘读到内存中的一个缓冲区中，这个缓冲区中的内容是可以直接读写的，那么后续对这个数据块的大部分访问就可以在内存中完成了。如果缓冲区中的内容被修改了，那么后续还需要通过 write_block 将缓冲区中的内容写回到磁盘块中
use super::{BlockDevice,BLOCK_SZ};
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use lazy_static::*;
use spin::Mutex;

pub struct BlockCache{
    //表示位于内存中的缓冲区
    cache:[u8;BLOCK_SZ],
    //这个块缓存来自于磁盘中的块的编号
    block_id:usize,
    //底层块设备的引用，可通过它进行块读写
    block_device:Arc<dyn BlockDevice>,
    //块从磁盘载入内存缓存之后，它有没有被修改过
    modified:bool,
}

impl BlockCache{
    //将一个块上的数据从磁盘读到缓冲区cache
    pub fn new(block_id:usize,block_device:Arc<dyn BlockDevice>) -> Self {
        let mut cache = [0u8;BLOCK_SZ];
        block_device.read_block(block_id,&mut cache);
        Self{
            cache,
            block_id,
            block_device,
            modified:false,
        }
    }
    //缓冲区中指定偏移量 offset 的字节地址
    fn addr_of_offset(&self,offset:usize) -> usize {
        &self.cache[offset] as *const _ as usize
    }

    //获取缓冲区中的位于偏移量 offset 的一个类型为 T(泛型) 的磁盘上数据结构的不可变引用，为了读取做准备
    pub fn get_ref<T>(&self,offset:usize) -> &T 
    where
        T:Sized,
    {
        let type_size = core::mem::size_of::<T>();
        assert!(offset + type_size <= BLOCK_SZ);
        let addr = self.addr_of_offset(offset);
        unsafe{&*(addr as *const T)}
    }

    //磁盘上数据结构的可变引用，由此可以对数据结构进行修改，为了修改做准备
    pub fn get_mut<T>(&mut self,offset:usize) -> &mut T 
    where 
        T:Sized
    {
        let type_size = core::mem::size_of::<T>();
        assert!(offset + type_size <= BLOCK_SZ);
        self.modified = true;
        let addr = self.addr_of_offset(offset);
        unsafe{&mut *(addr as *mut T)}
    }

    //在 BlockCache 缓冲区偏移量为 offset 的位置获取一个类型为 T 的磁盘上数据结构的不可变引用
    pub fn read<T, V>(&self, offset: usize, f: impl FnOnce(&T) -> V) -> V {
        f(self.get_ref(offset))
    }

    //在 BlockCache 缓冲区偏移量为 offset 的位置获取一个类型为 T 的磁盘上数据结构的可变引用
    pub fn modify<T, V>(&mut self, offset: usize, f: impl FnOnce(&mut T) -> V) -> V {
        f(self.get_mut(offset))
    }

    pub fn sync(&mut self) {
        if self.modified {
            self.modified = false;
            self.block_device.write_block(self.block_id, &self.cache);
        }
    }
}


impl Drop for BlockCache{
    fn drop(&mut self){
        self.sync()
    }
}

const BLOCK_CACHE_SIZE: usize = 16;

pub struct BlockCacheManager {
    queue: VecDeque<(usize, Arc<Mutex<BlockCache>>)>,
}

//使用一种类 FIFO 的简单缓存替换算法，因此在管理器中只需维护一个队列
impl BlockCacheManager {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }
    //尝试从块缓存管理器中获取一个编号为 block_id 的块的块缓存，如果找不到，会从磁盘读取到内存中，还有可能会发生缓存替换
    pub fn get_block_cache(
        &mut self,
        block_id: usize,
        block_device: Arc<dyn BlockDevice>,
    ) -> Arc<Mutex<BlockCache>> {
        //遍历整个队列试图找到一个编号相同的块缓存，如果找到了，会将块缓存管理器中保存的块缓存的引用复制一份并返回
        if let Some(pair) = self.queue.iter().find(|pair| pair.0 == block_id) {
            Arc::clone(&pair.1)
        } else {
            //对应找不到的情况，此时必须将块从磁盘读入内存中的缓冲区,在实际读取之前，需要判断管理器保存的块缓存数量是否已经达到了上限
            if self.queue.len() == BLOCK_CACHE_SIZE {
                if let Some((idx, _)) = self
                    .queue
                    .iter()
                    .enumerate()
                    .find(|(_, pair)| Arc::strong_count(&pair.1) == 1)
                {
                    self.queue.drain(idx..=idx);
                } else {
                    panic!("Run out of BlockCache!");
                }
            }
            //创建一个新的块缓存（会触发 read_block 进行块读取）并加入到队尾，最后返回给请求者
            let block_cache = Arc::new(Mutex::new(BlockCache::new(
                block_id,
                Arc::clone(&block_device),
            )));
            self.queue.push_back((block_id, Arc::clone(&block_cache)));
            block_cache
        }
    }
}

lazy_static! {
    pub static ref BLOCK_CACHE_MANAGER: Mutex<BlockCacheManager> =
        Mutex::new(BlockCacheManager::new());
}

//对于其他模块而言，就可以直接通过 get_block_cache 方法来请求块缓存了
pub fn get_block_cache(
    block_id: usize,
    block_device: Arc<dyn BlockDevice>,
) -> Arc<Mutex<BlockCache>> {
    BLOCK_CACHE_MANAGER
        .lock()
        .get_block_cache(block_id, block_device)
}

pub fn block_cache_sync_all() {
    let manager = BLOCK_CACHE_MANAGER.lock();
    for (_, cache) in manager.queue.iter() {
        cache.lock().sync();
    }
}