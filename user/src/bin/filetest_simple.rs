#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::{close, open, read, write, OpenFlags};

#[no_mangle]
pub fn main() -> i32 {
    let test_str = "Hello, world!";
    //打开文件 filea ，向其中写入字符串 Hello, world! 而后关闭文件
    let filea = "filea\0";
    let fd = open(filea, OpenFlags::CREATE | OpenFlags::WRONLY);
    assert!(fd > 0);
    let fd = fd as usize;
    write(fd, test_str.as_bytes());
    close(fd);
    //我们以只读 RDONLY 的方式将文件 filea 的内容读取到缓冲区 buffer 中
    let fd = open(filea, OpenFlags::RDONLY);
    assert!(fd > 0);
    let fd = fd as usize;
    let mut buffer = [0u8; 100];
    let read_len = read(fd, &mut buffer) as usize;
    close(fd);
    //确认从 filea 读取到的内容和之前写入的一致，则测试通过
    assert_eq!(test_str, core::str::from_utf8(&buffer[..read_len]).unwrap(),);
    println!("file_test passed!");
    0
}