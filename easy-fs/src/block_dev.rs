use core::any::Any;
//块设备的抽象接口
pub trait BlockDevice:Send + Sync + Any {
    //编号为 block_id 的块从磁盘读入内存中的缓冲区 buf
    fn read_block(&self,block_id:usize,buf:&mut [u8]);
    //内存中的缓冲区 buf 中的数据写入磁盘编号为 block_id 的块
    fn write_block(&self,block_id:usize,buf:&[u8]);
}
