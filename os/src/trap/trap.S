.altmacro
.macro SAVE_GP n
    sd x\n, \n*8(sp)
.endm
.macro LOAD_GP n
    ld x\n, \n*8(sp)
.endm
    .section .text
    .globl __alltraps
    .globl __restore
    .align 2
__alltraps:#把trap上下文保存在内核栈上
    csrrw sp, sscratch, sp

    addi sp, sp, -34*8  #在内核栈保存上下文

    sd x1, 1*8(sp)

    sd x3, 3*8(sp)

    .set n, 5
    .rept 27
        SAVE_GP %n
        .set n, n+1
    .endr

    csrr t0, sstatus #这俩寄存器是特权级切换恢复的核心,思考：为什么不能直接把特权寄存器的值赋值给内存单元？
    csrr t1, sepc    #riscv中常规的数据处理和访存类指令只能操作通用寄存器不能操作csr
    sd t0, 32*8(sp)
    sd t1, 33*8(sp)
 
    csrr t2, sscratch #sscratch：进入trap之前的sp的值，指向用户栈，现在sp指向内核栈
    sd t2, 2*8(sp) 
    mv a0, sp
    call trap_handler #trap分类器，要知道一些上下文比如syscall id和syscall参数

__restore: #上下文恢复
    mv sp, a0 
    ld t0, 32*8(sp)
    ld t1, 33*8(sp)
    ld t2, 2*8(sp)
    csrw sstatus, t0 #把通用寄存器的值重写到对应的csr中
    csrw sepc, t1
    csrw sscratch, t2
    ld x1, 1*8(sp)
    ld x3, 3*8(sp)
    .set n, 5
    .rept 27
        LOAD_GP %n
        .set n, n+1
    .endr
 
    addi sp, sp, 34*8
    
    csrrw sp, sscratch, sp #交换当前的sp（在sscratch中）和用户栈sp
    sret