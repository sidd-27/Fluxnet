use crate::umem::layout::UmemLayout;
use memmap2::{MmapMut, MmapOptions};
use std::io;

pub struct UmemRegion {
    mmap: MmapMut,
    layout: UmemLayout,
}

impl UmemRegion {
    pub fn new(layout: UmemLayout) -> io::Result<Self> {
        let len = layout.size();
        let mmap = MmapOptions::new().len(len).map_anon()?;

        Ok(Self { mmap, layout })
    }

    pub fn as_ptr(&self) -> *mut u8 {
        self.mmap.as_ptr() as *mut u8
    }

    pub fn len(&self) -> usize {
        self.layout.size()
    }
    
    pub fn layout(&self) -> UmemLayout {
        self.layout
    }
}
