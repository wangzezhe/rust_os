.altmacro
.macro SAVE_GP n
    sd x\n, \n*8(sp)
.endm
.macro LOAD_GP n
    ld x\n, \n*8(sp)
.endm
    .section .text.trampoline
    .globl __alltraps
    .globl __restore
    .align 2

#保存trap之前的上下文
__alltraps:
    csrrw sp, sscratch, sp
    sd x1, 1*8(sp)
    sd x3, 3*8(sp)
    .set n, 5
    .rept 27
        SAVE_GP %n
        .set n, n+1
    .endr
    csrr t0, sstatus
    csrr t1, sepc
    sd t0, 32*8(sp)
    sd t1, 33*8(sp)
    csrr t2, sscratch
    sd t2, 2*8(sp)

    #切换到内核地址空间并跳转到 trap handler 
    #将内核地址空间的 token 载入到 t0 寄存器中
    ld t0, 34*8(sp)
    #将 trap handler 入口点的虚拟地址载入到 t1 寄存器中
    ld t1, 36*8(sp)
    #将 sp 修改为应用内核栈顶的地址
    ld sp, 35*8(sp)
    #将 satp 修改为内核地址空间的 token
    csrw satp, t0
    #使用 sfence.vma 刷新快表
    sfence.vma
    #通过 jr 指令跳转到 t1 寄存器所保存的trap handler 入口点的地址
    jr t1

#恢复之前的上下文
__restore:
    #切换回应用地址空间,Trap 上下文是保存在应用地址空间中
    csrw satp, a1
    sfence.vma
    #传入的 Trap 上下文位置保存在 sscratch 寄存器中
    csrw sscratch, a0
    #sp 修改为 Trap 上下文的位置，后面基于它恢复各通用寄存器和 CSR
    mv sp, a0
    ld t0, 32*8(sp)
    ld t1, 33*8(sp)
    csrw sstatus, t0
    csrw sepc, t1
    ld x1, 1*8(sp)
    ld x3, 3*8(sp)
    .set n, 5
    .rept 27
        LOAD_GP %n
        .set n, n+1
    .endr
    ld sp, 2*8(sp)
    #返回用户态
    sret