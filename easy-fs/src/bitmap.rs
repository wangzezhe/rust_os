use super::{get_block_cache, BlockDevice, BLOCK_SZ};
use alloc::sync::Arc;
type BitmapBlock = [u64; 64];
const BLOCK_BITS: usize = BLOCK_SZ * 8;

//每个 bit 都代表一个索引节点/数据块的分配状态
//通过基于 bit 为单位的分配（寻找一个为 0 的bit位并设置为 1）和回收（将bit位清零）来进行索引节点/数据块的分配和回收
//保存了它所在区域的起始块编号以及区域的长度为多少个块，Bitmap 自身是驻留在内存中的
pub struct Bitmap {
    start_block_id: usize,
    blocks: usize,
}

//分解为区域中的块编号 block_pos 、块内的组编号 bits64_pos 以及组内编号 inner_pos 的三元组
fn decomposition(mut bit: usize) -> (usize, usize, usize) {
    let block_pos = bit / BLOCK_BITS;
    bit %= BLOCK_BITS;
    (block_pos, bit / 64, bit % 64)
}

impl Bitmap {
    pub fn new(start_block_id: usize, blocks: usize) -> Self {
        Self {
            start_block_id,
            blocks,
        }
    }
    // 如何分配一个bit，遍历区域中的每个块，再在每个块中以bit组（每组 64 bits）为单位进行遍历，找到一个尚未被全部分配出去的组，最后在里面分配一个bit
    pub fn alloc(&self, block_device: &Arc<dyn BlockDevice>) -> Option<usize> {
        //枚举区域中的每个块（编号为 block_id ），在循环内部我们需要读写这个块，在块内尝试找到一个空闲的bit并置1
        for block_id in 0..self.blocks {
            let pos = get_block_cache(
                //调用 get_block_cache 获取块缓存，注意我们传入的块编号是区域起始块编号 start_block_id 加上区域内的块编号 block_id 得到的块设备上的块编号
                block_id + self.start_block_id as usize,
                Arc::clone(block_device),
            )
            .lock()
            .modify(0, |bitmap_block: &mut BitmapBlock| {
                //尝试在 bitmap_block 中找到一个空闲的bit并返回其位置，如果不存在的话则返回 None
                if let Some((bits64_pos, inner_pos)) = bitmap_block
                    .iter()
                    .enumerate()
                    .find(|(_, bits64)| **bits64 != u64::MAX)
                    .map(|(bits64_pos, bits64)| (bits64_pos, bits64.trailing_ones() as usize))
                {
                    // modify cache
                    bitmap_block[bits64_pos] |= 1u64 << inner_pos;
                    Some(block_id * BLOCK_BITS + bits64_pos * 64 + inner_pos as usize)
                } else {
                    None
                }
            });
            //一旦在某个块中找到一个空闲的bit并成功分配，就不再考虑后续的块
            if pos.is_some() {
                return pos;
            }
        }
        None
    }
    pub fn dealloc(&self, block_device: &Arc<dyn BlockDevice>, bit: usize) {
        let (block_pos, bits64_pos, inner_pos) = decomposition(bit);
        get_block_cache(block_pos + self.start_block_id, Arc::clone(block_device))
            .lock()
            .modify(0, |bitmap_block: &mut BitmapBlock| {
                assert!(bitmap_block[bits64_pos] & (1u64 << inner_pos) > 0);
                bitmap_block[bits64_pos] -= 1u64 << inner_pos;
            });
    }
    pub fn maximum(&self) -> usize {
        self.blocks * BLOCK_BITS
    }
}