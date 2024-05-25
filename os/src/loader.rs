//将应用加载到内存并进行管理
//获取一个应用的 ELF 执行文件数据
use alloc::vec::Vec;
use lazy_static::*;

// 获取链接到内核内的应用的数目
pub fn get_num_app() -> usize {
    extern "C" {
        fn _num_app();
    }
    unsafe { (_num_app as usize as *const usize).read_volatile() }
}

//根据传入的应用编号取出对应应用的 ELF 格式可执行文件数据。它们和之前一样仍是基于 build.rs 生成的 link_app.S 给出的符号来确定其位置，并实际放在内核的数据段中
pub fn get_app_data(app_id: usize) -> &'static [u8] {
    extern "C" {
        fn _num_app();
    }
    let num_app_ptr = _num_app as usize as *const usize;
    let num_app = get_num_app();
    let app_start = unsafe { core::slice::from_raw_parts(num_app_ptr.add(1), num_app + 1) };
    assert!(app_id < num_app);
    unsafe {
        core::slice::from_raw_parts(
            app_start[app_id] as *const u8,
            app_start[app_id + 1] - app_start[app_id],
        )
    }
}


//exec系统调用的先期准备
//分析 link_app.S 中的内容，并用一个全局可见的 只读 向量 APP_NAMES 来按照顺序将所有应用的名字保存在内存中
lazy_static! {
    //只读向量APP_NAMES按照顺序将所有应用的名字保存在内存中
    static ref APP_NAMES: Vec<&'static str> = {
        let num_app = get_num_app();
        //分析 link_app.S 中的内容
        extern "C" {
            fn _app_names();
        }
        let mut start = _app_names as usize as *const u8;
        let mut v = Vec::new();
        unsafe {
            for _ in 0..num_app {
                let mut end = start;
                //使用read_volatile 方法来确保每次读取的值都是从内存中读取的，而不是从 CPU 缓存中获取的。
                while end.read_volatile() != b'\0' {
                    end = end.add(1);
                }
                let slice = core::slice::from_raw_parts(start, end as usize - start as usize);
                let str = core::str::from_utf8(slice).unwrap();
                v.push(str);
                //更新起始指针 start，使其指向下一个字符串的起始位置
                start = end.add(1);
            }
        }
        v
    };
}

//按照应用的名字来查找获得应用的 ELF 数据
#[allow(unused)]
pub fn get_app_data_by_name(name: &str) -> Option<&'static [u8]> {
    let num_app = get_num_app();
    (0..num_app)
        .find(|&i| APP_NAMES[i] == name)
        .map(get_app_data)
}

//打印出所有可用的应用的名字。
pub fn list_apps() {
    println!("/**** APPS ****");
    for app in APP_NAMES.iter() {
        println!("{}", app);
    }
    println!("**************/");
}