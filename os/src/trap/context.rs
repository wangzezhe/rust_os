use riscv::register::sstatus::{self, Sstatus, SPP};
#[repr(C)]
pub struct TrapContext {
    // 通用寄存器 x0~x31，为什么要保存全部寄存器？因为控制流进行的时候无法确定具体哪些register需要或者不需要保存
    //内核和用户程序是两条控制流（甚至可能是不同的语言编写的）
    pub x: [usize; 32],
    /// CSR sstatus（ssp字段存储的是特权级状态）      
    pub sstatus: Sstatus,
    /// CSR sepc(pc中的内容)
    pub sepc: usize,
}

//在 RISC-V 架构中，唯一一种能够使得 CPU 特权级下降的方法就是执行 Trap 返回的特权指令，如 sret 、mret
impl TrapContext {
    pub fn set_sp(&mut self, sp: usize) {
        self.x[2] = sp;
    }
    pub fn app_init_context(entry: usize, sp: usize) -> Self {
        let mut sstatus = sstatus::read(); // CSR sstatus
        sstatus.set_spp(SPP::User); //特权级设置user
        let mut cx = Self {
            x: [0; 32],
            sstatus,
            sepc: entry, //把pc修改为应用程序的入口
        };
        cx.set_sp(sp); //获取app的用户栈指针
        cx 
    }
}