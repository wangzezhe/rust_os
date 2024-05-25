//虚拟地址空间需要有一个数据结构管理
use super::{frame_alloc, FrameTracker};
use super::{PTEFlags, PageTable, PageTableEntry};
use super::{PhysAddr, PhysPageNum, VirtAddr, VirtPageNum};
use super::{StepByOne, VPNRange};
use crate::config::{MEMORY_END, MMIO, PAGE_SIZE, TRAMPOLINE, TRAP_CONTEXT, USER_STACK_SIZE};
use crate::sync::UPSafeCell;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::arch::asm;
use lazy_static::*;
use riscv::register::satp;


//内核地址空间中需要存在一个恒等映射到内核数据段之外的可用物理页帧的逻辑段，这样才能在启用页表机制之后，内核仍能以纯软件的方式读写这些物理页帧。
extern "C" {
    fn stext();
    fn etext();
    fn srodata();
    fn erodata();
    fn sdata();
    fn edata();
    fn sbss_with_stack();
    fn ebss();
    fn ekernel();
    fn strampoline();
}
//内核的页表和虚拟空间的建立
lazy_static! {
    pub static ref KERNEL_SPACE: Arc<UPSafeCell<MemorySet>> =
        Arc::new(unsafe { UPSafeCell::new(MemorySet::new_kernel()) });
}

pub fn kernel_token() -> usize {
    KERNEL_SPACE.exclusive_access().token()
}

//地址空间 是一系列有关联的不一定连续的逻辑段，
//这种关联一般是指这些逻辑段组成的虚拟内存空间与一个运行的程序绑定，
//即这个运行的程序对代码和数据的直接访问范围限制在它关联的虚拟地址空间之内
pub struct MemorySet {
    //PageTable 下挂着所有多级页表的节点所在的物理页帧
    page_table: PageTable,
    //每个 MapArea 下则挂着对应逻辑段中的数据所在的物理页帧
    areas: Vec<MapArea>,
}

impl MemorySet {
    //新建一个空的地址空间
    pub fn new_bare() -> Self {
        Self {
            page_table: PageTable::new(),
            areas: Vec::new(),
        }
    }
    pub fn token(&self) -> usize {
        self.page_table.token()
    }

    //方法调用 push ，可以在当前地址空间插入一个 Framed 方式映射到物理内存的逻辑段。
    pub fn insert_framed_area(
        &mut self,
        start_va: VirtAddr,
        end_va: VirtAddr,
        permission: MapPermission,
    ) {
        self.push(
            MapArea::new(start_va, end_va, MapType::Framed, permission),
            None,
        );
    }
    pub fn remove_area_with_start_vpn(&mut self,start_vpn:VirtPageNum){
        if let Some((idx,area)) = self
            .areas
            .iter_mut()
            .enumerate()
            .find(|(_,area)| area.vpn_range.get_start() == start_vpn)
            {
                area.unmap(&mut self.page_table);
                self.areas.remove(idx);
            }
    }

    //可以在当前地址空间插入一个新的逻辑段 map_area
    fn push(&mut self, mut map_area: MapArea, data: Option<&[u8]>) {
        map_area.map(&mut self.page_table);
        if let Some(data) = data {
            map_area.copy_data(&mut self.page_table, data);
        }
        self.areas.push(map_area);
    }
    
    //直接在多级页表中插入一个从地址空间的最高虚拟页面映射到跳板汇编代码所在的物理页帧的键值对，访问权限与代码段相同
    fn map_trampoline(&mut self) {
        self.page_table.map(
            VirtAddr::from(TRAMPOLINE).into(),
            PhysAddr::from(strampoline as usize).into(),
            PTEFlags::R | PTEFlags::X,
        );
    }

    //将映射跳板和地址空间中最低256GiB中的内核逻辑段
    pub fn new_kernel() -> Self {
        //低地址到高地址依次创建 5 个逻辑段并通过 push 方法将它们插入到内核地址空间中
        //内核的四个逻辑段 .text/.rodata/.data/.bss 被恒等映射到物理内存
        /*四个逻辑段的 U 标志位均未被设置，使得 CPU 只能在处于 S 特权级（或以上）时访问它们；
          代码段 .text 不允许被修改；
          只读数据段 .rodata 不允许被修改，也不允许从它上面取指执行；
          .data/.bss 均允许被读写，但是不允许从它上面取指执行。 */
        let mut memory_set = Self::new_bare();
        memory_set.map_trampoline();
        println!(".text [{:#x}, {:#x})", stext as usize, etext as usize);
        println!(".rodata [{:#x}, {:#x})", srodata as usize, erodata as usize);
        println!(".data [{:#x}, {:#x})", sdata as usize, edata as usize);
        println!(
            ".bss [{:#x}, {:#x})",
            sbss_with_stack as usize, ebss as usize
        );
        println!("mapping .text section");
        memory_set.push(
            MapArea::new(
                (stext as usize).into(),
                (etext as usize).into(),
                MapType::Identical,
                MapPermission::R | MapPermission::X,
            ),
            None,
        );
        println!("mapping .rodata section");
        memory_set.push(
            MapArea::new(
                (srodata as usize).into(),
                (erodata as usize).into(),
                MapType::Identical,
                MapPermission::R,
            ),
            None,
        );
        println!("mapping .data section");
        memory_set.push(
            MapArea::new(
                (sdata as usize).into(),
                (edata as usize).into(),
                MapType::Identical,
                MapPermission::R | MapPermission::W,
            ),
            None,
        );
        println!("mapping .bss section");
        memory_set.push(
            MapArea::new(
                (sbss_with_stack as usize).into(),
                (ebss as usize).into(),
                MapType::Identical,
                MapPermission::R | MapPermission::W,
            ),
            None,
        );
        println!("mapping physical memory");
        memory_set.push(
            MapArea::new(
                (ekernel as usize).into(),
                MEMORY_END.into(),
                MapType::Identical,
                MapPermission::R | MapPermission::W,
            ),
            None,
        );
        //
        println!("mapping memory-mapped registers");
        for pair in MMIO {
            memory_set.push(
                MapArea::new(
                    (*pair).0.into(),
                    ((*pair).0 + (*pair).1).into(),
                    MapType::Identical,
                    MapPermission::R | MapPermission::W,
                ),
                None,
            );
        }
        memory_set
    }
    //创建应用地址空间的时候，我们需要对 get_app_data 得到的 ELF 格式数据进行解析，找到各个逻辑段所在位置和访问限制并插入进来，最终得到一个完整的应用地址空间
     pub fn from_elf(elf_data: &[u8]) -> (Self, usize, usize) {
        let mut memory_set = Self::new_bare();
        //将跳板插入到应用地址空间
        memory_set.map_trampoline();
        //外部 crate xmas_elf 来解析传入的应用 ELF 数据并可以轻松取出各个部分
        let elf = xmas_elf::ElfFile::new(elf_data).unwrap();
        //直接得到 program header 的数目，然后遍历所有的 program header 并将合适的区域加入到应用地址空间中。
        let elf_header = elf.header;
        let magic = elf_header.pt1.magic;
        assert_eq!(magic, [0x7f, 0x45, 0x4c, 0x46], "invalid elf!");
        let ph_count = elf_header.pt2.ph_count();
        let mut max_end_vpn = VirtPageNum(0);
        for i in 0..ph_count {
            let ph = elf.program_header(i).unwrap();
            if ph.get_type().unwrap() == xmas_elf::program::Type::Load {
                let start_va: VirtAddr = (ph.virtual_addr() as usize).into();
                let end_va: VirtAddr = ((ph.virtual_addr() + ph.mem_size()) as usize).into();
                let mut map_perm = MapPermission::U;
                let ph_flags = ph.flags();
                if ph_flags.is_read() {
                    map_perm |= MapPermission::R;
                }
                if ph_flags.is_write() {
                    map_perm |= MapPermission::W;
                }
                if ph_flags.is_execute() {
                    map_perm |= MapPermission::X;
                }
                let map_area = MapArea::new(start_va, end_va, MapType::Framed, map_perm);
                max_end_vpn = map_area.vpn_range.get_end();
                memory_set.push(
                    map_area,
                    Some(&elf.input[ph.offset() as usize..(ph.offset() + ph.file_size()) as usize]),
                );
            }
        }
        //开始处理用户栈
        let max_end_va: VirtAddr = max_end_vpn.into();
        let mut user_stack_bottom: usize = max_end_va.into();
        user_stack_bottom += PAGE_SIZE;
        let user_stack_top = user_stack_bottom + USER_STACK_SIZE;
        
        //在应用地址空间中映射次高页面来存放 Trap 上下文
        memory_set.push(
            MapArea::new(
                user_stack_bottom.into(),
                user_stack_top.into(),
                MapType::Framed,
                MapPermission::R | MapPermission::W | MapPermission::U,
            ),
            None,
        );
        memory_set.push(
            MapArea::new(
                user_stack_top.into(),
                user_stack_top.into(),
                MapType::Framed,
                MapPermission::R | MapPermission::W | MapPermission::U,
            ),
            None,
        );
        memory_set.push(
            MapArea::new(
                TRAP_CONTEXT.into(),
                TRAMPOLINE.into(),
                MapType::Framed,
                MapPermission::R | MapPermission::W,
            ),
            None,
        );
        //我们不仅返回应用地址空间 memory_set ，也同时返回用户栈虚拟地址 user_stack_top 以及从解析 ELF 得到的该应用入口点地址，它们将被我们用来创建应用的任务控制块。
        (
            memory_set,
            user_stack_top,
            //应用入口点地址
            elf.header.pt2.entry_point() as usize,
        )
    }
    //可以复制一个完全相同的地址空间
    pub fn from_existed_user(user_space:&Self) -> Self {
        //通过 new_bare 新创建一个空的地址空间
        let mut memory_set = Self::new_bare();
        //为这个地址空间映射上跳板页面
        //因为我们解析 ELF 创建地址空间的时候，并没有将跳板页作为一个单独的逻辑段插入到地址空间的逻辑段向量 areas 中，所以这里需要单独映射上
        memory_set.map_trampoline();
        for area in user_space.areas.iter()  {
            let new_area = MapArea::from_another(area);
            memory_set.push(new_area,None);
            for vpn in area.vpn_range {
                let src_ppn = user_space.translate(vpn).unwrap().ppn();
                let dst_ppn = memory_set.translate(vpn).unwrap().ppn();
                dst_ppn
                    .get_bytes_array()
                    .copy_from_slice(src_ppn.get_bytes_array());
            }
        }
        memory_set
    }
    //将token值写入当前 CPU 的 satp CSR ，从这一刻开始 SV39 分页模式就被启用了，而且 MMU 会使用内核地址空间的多级页表进行地址转换。
    pub fn activate(&self) {
        let satp = self.page_table.token();
        unsafe {
            satp::write(satp);
            //为了确保 MMU 的地址转换能够及时与 satp 的修改同步，我们需要立即使用 sfence.vma 指令将快表清空
            asm!("sfence.vma");
        }
    }
    pub fn translate(&self, vpn: VirtPageNum) -> Option<PageTableEntry> {
        self.page_table.translate(vpn)
    }
    pub fn recycle_data_pages(&mut self) {
        self.areas.clear();
    }
    
}


//以逻辑段 MapArea 为单位描述一段连续地址的虚拟内存
//区间内包含的所有虚拟页面都以一种相同的方式映射到物理页帧，具有可读/可写/可执行
pub struct MapArea {
    //VPNRange 描述一段虚拟页号的连续区间，表示该逻辑段在地址区间中的位置和长度
    vpn_range: VPNRange,
    data_frames: BTreeMap<VirtPageNum, FrameTracker>,
    map_type: MapType,
    map_perm: MapPermission,
}

impl MapArea {
    //方法可以新建一个逻辑段结构体，注意传入的起始/终止虚拟地址会分别被下取整/上取整为虚拟页号并传入迭代器 vpn_range
    pub fn new(
        start_va: VirtAddr,
        end_va: VirtAddr,
        map_type: MapType,
        map_perm: MapPermission,
    ) -> Self {
        let start_vpn: VirtPageNum = start_va.floor();
        let end_vpn: VirtPageNum = end_va.ceil();
        Self {
            vpn_range: VPNRange::new(start_vpn, end_vpn),
            data_frames: BTreeMap::new(),
            map_type,
            map_perm,
        }
    }
    //从一个逻辑段复制得到一个虚拟地址区间、映射方式和权限控制均相同的逻辑段，不同的是由于它还没有真正被映射到物理页帧上，所以 data_frames 字段为空
    pub fn from_another(another:&Self) -> Self{
        Self{
            vpn_range:VPNRange::new(another.vpn_range.get_start(),another.vpn_range.get_end()),
            data_frames:BTreeMap::new(),
            map_type:another.map_type,
            map_perm:another.map_perm,
        }
    }

    //在虚拟页号 vpn 已经确定的情况下，它需要知道要将一个怎么样的页表项插入多级页表
    pub fn map_one(&mut self, page_table: &mut PageTable, vpn: VirtPageNum) {
        let ppn: PhysPageNum;
        match self.map_type {
            //当以恒等映射 Identical 方式映射的时候，物理页号就等于虚拟页号
            MapType::Identical => {
                ppn = PhysPageNum(vpn.0);
            }
            //以 Framed 方式映射时，需要分配一个物理页帧让当前的虚拟页面可以映射过去，此时页表项中的物理页号自然就是 这个被分配的物理页帧的物理页号。此时还需要将这个物理页帧挂在逻辑段的 data_frames 字段下。
            MapType::Framed => {
                let frame = frame_alloc().unwrap();
                ppn = frame.ppn;
                self.data_frames.insert(vpn, frame);
            }
        }
        let pte_flags = PTEFlags::from_bits(self.map_perm.bits).unwrap();
        page_table.map(vpn, ppn, pte_flags);
    }

    //调用 PageTable 的 unmap 接口删除以传入的虚拟页号为键的键值对即可
    #[allow(unused)]
    pub fn unmap_one(&mut self, page_table: &mut PageTable, vpn: VirtPageNum) {
        if self.map_type == MapType::Framed {
            self.data_frames.remove(&vpn);
        }
        page_table.unmap(vpn);
    }
    //可以将当前逻辑段到物理内存的映射从传入的该逻辑段所属的地址空间的多级页表中加入或删除
    pub fn map(&mut self, page_table: &mut PageTable) {
        for vpn in self.vpn_range {
            self.map_one(page_table, vpn);
        }
    }
    #[allow(unused)]
    pub fn unmap(&mut self, page_table: &mut PageTable) {
        for vpn in self.vpn_range {
            self.unmap_one(page_table, vpn);
        }
    }
    #[allow(unused)]
    pub fn shrink_to(&mut self, page_table: &mut PageTable, new_end: VirtPageNum) {
        for vpn in VPNRange::new(new_end, self.vpn_range.get_end()) {
            self.unmap_one(page_table, vpn)
        }
        self.vpn_range = VPNRange::new(self.vpn_range.get_start(), new_end);
    }
    #[allow(unused)]
    pub fn append_to(&mut self, page_table: &mut PageTable, new_end: VirtPageNum) {
        for vpn in VPNRange::new(self.vpn_range.get_end(), new_end) {
            self.map_one(page_table, vpn)
        }
        self.vpn_range = VPNRange::new(self.vpn_range.get_start(), new_end);
    }

    //方法将切片 data 中的数据拷贝到当前逻辑段实际被内核放置在的各物理页帧上，从而在地址空间中通过该逻辑段就能访问这些数据。
    pub fn copy_data(&mut self, page_table: &mut PageTable, data: &[u8]) {
        assert_eq!(self.map_type, MapType::Framed);
        let mut start: usize = 0;
        let mut current_vpn = self.vpn_range.get_start();
        let len = data.len();

        //遍历每一个需要拷贝数据的虚拟页面
        loop {
            let src = &data[start..len.min(start + PAGE_SIZE)];
            let dst = &mut page_table
                .translate(current_vpn)
                .unwrap()
                .ppn()
                .get_bytes_array()[..src.len()];
            dst.copy_from_slice(src);
            start += PAGE_SIZE;
            if start >= len {
                break;
            }
            //
            current_vpn.step();
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum MapType {
    //Identical 表示提到的恒等映射方式
    Identical,
    //表示对于每个虚拟页面都有一个新分配的物理页帧与之对应，虚地址与物理地址的映射关系是相对随机的。
    Framed,
}


//表示控制该逻辑段的访问方式，它是页表项标志位 PTEFlags 的一个子集，仅保留 U/R/W/X 四个标志位
bitflags! {
    pub struct MapPermission: u8 {
        const R = 1 << 1;
        const W = 1 << 2;
        const X = 1 << 3;
        const U = 1 << 4;
    }
}

//检查内核地址空间的多级页表是否被正确设置
#[allow(unused)]
pub fn remap_test() {
    let mut kernel_space = KERNEL_SPACE.exclusive_access();
    let mid_text: VirtAddr = ((stext as usize + etext as usize) / 2).into();
    let mid_rodata: VirtAddr = ((srodata as usize + erodata as usize) / 2).into();
    let mid_data: VirtAddr = ((sdata as usize + edata as usize) / 2).into();
    assert!(!kernel_space
        .page_table
        .translate(mid_text.floor())
        .unwrap()
        .writable(),);
    assert!(!kernel_space
        .page_table
        .translate(mid_rodata.floor())
        .unwrap()
        .writable(),);
    assert!(!kernel_space
        .page_table
        .translate(mid_data.floor())
        .unwrap()
        .executable(),);
    println!("remap_test passed!");
}