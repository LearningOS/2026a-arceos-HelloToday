#![no_std]


use allocator::{BaseAllocator, ByteAllocator, PageAllocator};

/// Early memory allocator
/// Use it before formal bytes-allocator and pages-allocator can work!
/// This is a double-end memory range:
/// - Alloc bytes forward
/// - Alloc pages backward
///
/// [ bytes-used | avail-area | pages-used ]
/// |            | -->    <-- |            |
/// start       b_pos        p_pos       end
///
/// For bytes area, 'count' records number of allocations.
/// When it goes down to ZERO, free bytes-used area.
/// For pages area, it will never be freed!
///
pub struct EarlyAllocator<const SIZE: usize> {
    start: usize,
    end: usize,
    b_pos: usize,
    p_pos: usize,
    count: usize
}

impl<const SIZE: usize> EarlyAllocator<SIZE> {
    pub const fn new() -> Self {
        Self {
            start: 0,
            end: 0,
            b_pos: 0,
            p_pos: 0,
            count: 0
        }
    }
}

impl<const SIZE: usize> BaseAllocator for EarlyAllocator<SIZE> {
    fn init(&mut self, start: usize, size: usize) {
        //记录整段内存的起点和结束边界
        self.start = start;
        self.end = start + size;

        //尚未分配，两个分配位置分别位于区域两端
        self.b_pos = self.start;
        self.p_pos = self.end;

        //当前没有尚未释放的字节分配
        self.count = 0;
    }

    fn add_memory(&mut self, start: usize, size: usize) -> allocator::AllocResult {
        if size == 0 {
            return Err(allocator::AllocError::InvalidParam);
        }

        let new_end = start
            .checked_add(size)
            .ok_or(allocator::AllocError::InvalidParam)?;

        //新增区域不能与当前管理区域重叠
        if start < self.end && new_end > self.start {
            return Err(allocator::AllocError::MemoryOverlap);
        }

        //当前只支持从右侧连续扩展，且右侧尚未分配页
        if start != self.end || self.p_pos != self.end {
            return Err(allocator::AllocError::InvalidParam);
        }

        self.end = new_end;
        self.p_pos = new_end;
        Ok(())
    }
}

impl<const SIZE: usize> ByteAllocator for EarlyAllocator<SIZE> {
    fn alloc(
        &mut self,
        layout: core::alloc::Layout,
    ) -> allocator::AllocResult<core::ptr::NonNull<u8>> {
        let size = layout.size();
        let align = layout.align();

        if size == 0 {
            return Err(allocator::AllocError::InvalidParam);
        }

        //向上找到满足对齐要求的起始地址
        let alloc_start = self
            .b_pos
            .checked_add(align - 1)
            .ok_or(allocator::AllocError::NoMemory)?
            & !(align - 1);

        let alloc_end = alloc_start
            .checked_add(size)
            .ok_or(allocator::AllocError::NoMemory)?;

        //不能越过页分配一侧的边界
        if alloc_end > self.p_pos {
            return Err(allocator::AllocError::NoMemory);
        }

        let ptr = core::ptr::NonNull::new(alloc_start as *mut u8)
            .ok_or(allocator::AllocError::NoMemory)?;

        self.b_pos = alloc_end;
        self.count += 1;
        Ok(ptr)
    }

    fn dealloc(&mut self, _pos: core::ptr::NonNull<u8>, _layout: core::alloc::Layout) {
        //归还一笔字节分配
        self.count -= 1;

        //全部归还后，整体回收字节区域
        if self.count == 0 {
            self.b_pos = self.start;
        }
    }

    fn total_bytes(&self) -> usize {
        self.end - self.start
    }

    fn used_bytes(&self) -> usize {
        self.b_pos - self.start
    }

    fn available_bytes(&self) -> usize {
        self.p_pos - self.b_pos
    }
}

impl<const SIZE: usize> PageAllocator for EarlyAllocator<SIZE> {
    const PAGE_SIZE: usize = SIZE;

    fn alloc_pages(
        &mut self,
        num_pages: usize,
        align_pow2: usize,
    ) -> allocator::AllocResult<usize> {
        if num_pages == 0 
            || !SIZE.is_power_of_two()
            || !align_pow2.is_power_of_two()
            ||  align_pow2 < SIZE
        {
            return Err(allocator::AllocError::InvalidParam);
        }

        //将页数换算成字节数
        let size = num_pages
            .checked_mul(SIZE)
            .ok_or(allocator::AllocError::NoMemory)?;

        //从高地址一侧预留空间，再向下对齐
        let alloc_start = self
            .p_pos
            .checked_sub(size)
            .ok_or(allocator::AllocError::NoMemory)?
            & !(align_pow2 - 1);

        //不能进入字节分配已经占用的区域
        if alloc_start < self.b_pos {
            return  Err(allocator::AllocError::NoMemory);
        }

        self.p_pos = alloc_start;
        Ok(alloc_start)
    }

    fn dealloc_pages(&mut self, _pos: usize, _num_pages: usize) {
         //当前分配器不回收页区域
    }

    fn total_pages(&self) -> usize {
        (self.end - self.start) / SIZE
    }

    fn used_pages(&self) -> usize {
        (self.end - self.p_pos) / SIZE
    }

    fn available_pages(&self) -> usize {
        (self.p_pos - self.b_pos) / SIZE
    }
}