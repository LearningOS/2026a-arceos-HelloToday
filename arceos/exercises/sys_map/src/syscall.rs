#![allow(dead_code)]

use core::ffi::{c_void, c_char, c_int};
use axhal::arch::TrapFrame;
use axhal::trap::{register_trap_handler, SYSCALL};
use axerrno::LinuxError;
use axtask::current;
use axtask::TaskExtRef;
use axhal::paging::MappingFlags;
use arceos_posix_api as api;

const SYS_IOCTL: usize = 29;
const SYS_OPENAT: usize = 56;
const SYS_CLOSE: usize = 57;
const SYS_READ: usize = 63;
const SYS_WRITE: usize = 64;
const SYS_WRITEV: usize = 66;
const SYS_EXIT: usize = 93;
const SYS_EXIT_GROUP: usize = 94;
const SYS_SET_TID_ADDRESS: usize = 96;
const SYS_MMAP: usize = 222;

const AT_FDCWD: i32 = -100;

/// Macro to generate syscall body
///
/// It will receive a function which return Result<_, LinuxError> and convert it to
/// the type which is specified by the caller.
#[macro_export]
macro_rules! syscall_body {
    ($fn: ident, $($stmt: tt)*) => {{
        #[allow(clippy::redundant_closure_call)]
        let res = (|| -> axerrno::LinuxResult<_> { $($stmt)* })();
        match res {
            Ok(_) | Err(axerrno::LinuxError::EAGAIN) => debug!(concat!(stringify!($fn), " => {:?}"),  res),
            Err(_) => info!(concat!(stringify!($fn), " => {:?}"), res),
        }
        match res {
            Ok(v) => v as _,
            Err(e) => {
                -e.code() as _
            }
        }
    }};
}

bitflags::bitflags! {
    #[derive(Debug)]
    /// permissions for sys_mmap
    ///
    /// See <https://github.com/bminor/glibc/blob/master/bits/mman.h>
    struct MmapProt: i32 {
        /// Page can be read.
        const PROT_READ = 1 << 0;
        /// Page can be written.
        const PROT_WRITE = 1 << 1;
        /// Page can be executed.
        const PROT_EXEC = 1 << 2;
    }
}

impl From<MmapProt> for MappingFlags {
    fn from(value: MmapProt) -> Self {
        let mut flags = MappingFlags::USER;
        if value.contains(MmapProt::PROT_READ) {
            flags |= MappingFlags::READ;
        }
        if value.contains(MmapProt::PROT_WRITE) {
            flags |= MappingFlags::WRITE;
        }
        if value.contains(MmapProt::PROT_EXEC) {
            flags |= MappingFlags::EXECUTE;
        }
        flags
    }
}

bitflags::bitflags! {
    #[derive(Debug)]
    /// flags for sys_mmap
    ///
    /// See <https://github.com/bminor/glibc/blob/master/bits/mman.h>
    struct MmapFlags: i32 {
        /// Share changes
        const MAP_SHARED = 1 << 0;
        /// Changes private; copy pages on write.
        const MAP_PRIVATE = 1 << 1;
        /// Map address must be exactly as requested, no matter whether it is available.
        const MAP_FIXED = 1 << 4;
        /// Don't use a file.
        const MAP_ANONYMOUS = 1 << 5;
        /// Don't check for reservations.
        const MAP_NORESERVE = 1 << 14;
        /// Allocation is for a stack.
        const MAP_STACK = 0x20000;
    }
}

#[register_trap_handler(SYSCALL)]
fn handle_syscall(tf: &TrapFrame, syscall_num: usize) -> isize {
    ax_println!("handle_syscall [{}] ...", syscall_num);
    let ret = match syscall_num {
         SYS_IOCTL => sys_ioctl(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _) as _,
        SYS_SET_TID_ADDRESS => sys_set_tid_address(tf.arg0() as _),
        SYS_OPENAT => sys_openat(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _, tf.arg3() as _),
        SYS_CLOSE => sys_close(tf.arg0() as _),
        SYS_READ => sys_read(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _),
        SYS_WRITE => sys_write(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _),
        SYS_WRITEV => sys_writev(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _),
        SYS_EXIT_GROUP => {
            ax_println!("[SYS_EXIT_GROUP]: system is exiting ..");
            axtask::exit(tf.arg0() as _)
        },
        SYS_EXIT => {
            ax_println!("[SYS_EXIT]: system is exiting ..");
            axtask::exit(tf.arg0() as _)
        },
        SYS_MMAP => sys_mmap(
            tf.arg0() as _,
            tf.arg1() as _,
            tf.arg2() as _,
            tf.arg3() as _,
            tf.arg4() as _,
            tf.arg5() as _,
        ),
        _ => {
            ax_println!("Unimplemented syscall: {}", syscall_num);
            -LinuxError::ENOSYS.code() as _
        }
    };
    ret
}

#[allow(unused_variables)]
fn sys_mmap(
    addr: *mut usize,
    length: usize,
    prot: i32,
    flags: i32,
    fd: i32,
    offset: isize,
) -> isize {
    if length == 0 {
        return -LinuxError::EINVAL.code() as isize;
    }
    //将整数 prot 解析为 MmapProt，检查是否包含未定义的权限位
    let mmap_prot = match MmapProt::from_bits(prot) {
        Some(value) => value,
        None => return -LinuxError::EINVAL.code() as isize,
    };
    //将 mmap 的权限表示转换为内核建立页映射时使用的 MappingFlags
    let mapping_flags = MappingFlags::from(mmap_prot);
    //将整数 flags 解析为 MmapFlags，检查是否包含未定义的映射选项位
    let mmap_flags = match MmapFlags::from_bits(flags) {
        Some(value) => value,
        None => return -LinuxError::EINVAL.code() as isize,        
    };

    //当前只支持由内核选择映射地址
    if !addr.is_null() {
        return -LinuxError::ENOSYS.code() as isize;
    }

    //当前只支持私有文件映射，不能接收其他标志组合
    if mmap_flags.bits() != MmapFlags::MAP_PRIVATE.bits() {
        return -LinuxError::ENOSYS.code() as isize;
    }

    //当前只支持从文件开头映射
    if offset != 0 {
        return -LinuxError::ENOSYS.code() as isize;
    }

    if fd < 0 {
        return -LinuxError::EBADF.code() as isize;
    }

    let file = match api::get_file_like(fd) {
        Ok(file) => file,
        Err(err) => return -err.code() as isize,
    };


    //保存文件原来的读取位置：1 表示相对于当前位置
    let saved_offset = api::sys_lseek(fd, 0, 1);
    if saved_offset < 0 {
        return saved_offset as isize;
    }

    //移到映射要求的位置0 表示相对于文件开头
    let seek_result = api::sys_lseek(fd, offset as _, 0);
    if seek_result < 0 {
        return seek_result as isize;
    }

    //准备缓冲区，尚未读入文件内容的位置保持为0
    let mut data = alloc::vec![0u8; length];
    let mut filled = 0;
    let mut read_error = None;

    while filled < data.len() {
        match file.read(&mut data[filled..]) {
            Ok(0) => break,
            Ok(count) => filled += count,
            Err(err) => {
                read_error = Some(err);
                break;
            }
        }
    }

    //即使读取出错，也先尝试恢复文件位置
    let restore_result = api::sys_lseek(fd, saved_offset, 0);
    if let Some(err) = read_error {
        return -err.code() as isize;
    }

    if restore_result < 0 {
        return restore_result as isize;
    }

    //将映射大小向上取整4KiB页的整数倍
    let page_size = memory_addr::PAGE_SIZE_4K;
    let map_size = match length.checked_add(page_size - 1) {
        Some(size) => size & !(page_size - 1),
        None => return -LinuxError::ENOMEM.code() as isize,
    };

    //取得当前任务的用户地址空间，并加锁
    let curr = current();
    let mut aspace = curr.task_ext().aspace.lock();

    //只在当前用户地址空间的范围内寻找空闲区域
    let limit = memory_addr::VirtAddrRange::from_start_size(aspace.base(), aspace.size());

    let start = match aspace.find_free_area(aspace.base(), map_size, limit) {
        Some(start) => start,
        None => return -LinuxError::ENOMEM.code() as isize,
    };

    //分配物理页，并建立用户虚拟地址到物理页的映射
    if let Err(err) = aspace.map_alloc(start, map_size, mapping_flags, true) {
        return -LinuxError::from(err).code() as isize;
    }

    //将内核缓冲区中的文件内容写入映射对应的物理内存
    if let Err(err) = aspace.write(start, &data) {
        let _ = aspace.unmap(start, map_size);
        return -LinuxError::from(err).code() as isize;
    }

    //成功，返回用户程序可以访问的起始虚拟地址
    start.as_usize() as isize
}

fn sys_openat(dfd: c_int, fname: *const c_char, flags: c_int, mode: api::ctypes::mode_t) -> isize {
    assert_eq!(dfd, AT_FDCWD);
    api::sys_open(fname, flags, mode) as isize
}

fn sys_close(fd: i32) -> isize {
    api::sys_close(fd) as isize
}

fn sys_read(fd: i32, buf: *mut c_void, count: usize) -> isize {
    api::sys_read(fd, buf, count)
}

fn sys_write(fd: i32, buf: *const c_void, count: usize) -> isize {
    api::sys_write(fd, buf, count)
}

fn sys_writev(fd: i32, iov: *const api::ctypes::iovec, iocnt: i32) -> isize {
    unsafe { api::sys_writev(fd, iov, iocnt) }
}

fn sys_set_tid_address(tid_ptd: *const i32) -> isize {
    let curr = current();
    curr.task_ext().set_clear_child_tid(tid_ptd as _);
    curr.id().as_u64() as isize
}

fn sys_ioctl(_fd: i32, _op: usize, _argp: *mut c_void) -> i32 {
    ax_println!("Ignore SYS_IOCTL");
    0
}
