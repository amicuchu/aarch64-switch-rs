extern crate alloc;

use crate::gpu::binder;
use crate::gpu::ioctl;
use crate::svc;
use crate::service::nv;
use crate::service::vi;
use crate::service::dispdrv;
use core::mem as cmem;
use core::ptr;
use crate::mem;
use super::*;

const MAX_BUFFERS: usize = 8;

pub type LayerDestroyFn = fn(vi::LayerId, mem::SharedObject<vi::ApplicationDisplayService>) -> Result<()>;

pub struct Surface<NS: nv::INvDrvService> {
    binder: binder::Binder,
    nvdrv_srv: mem::SharedObject<NS>,
    application_display_service: mem::SharedObject<vi::ApplicationDisplayService>,
    width: u32,
    height: u32,
    buffer_data: *mut u8,
    buffer_alloc_layout: alloc::alloc::Layout,
    single_buffer_size: usize,
    buffer_count: u32,
    slot_has_requested: [bool; MAX_BUFFERS],
    graphic_buf: GraphicBuffer,
    color_fmt: ColorFormat,
    pixel_fmt: PixelFormat,
    layout: Layout,
    display_id: vi::DisplayId,
    layer_id: vi::LayerId,
    layer_destroy_fn: LayerDestroyFn,
    nvhost_fd: u32,
    nvmap_fd: u32,
    nvhostctrl_fd: u32,
}

impl<NS: nv::INvDrvService> Surface<NS> {
    pub fn new(binder_handle: i32, nvdrv_srv: mem::SharedObject<NS>, application_display_service: mem::SharedObject<vi::ApplicationDisplayService>, nvhost_fd: u32, nvmap_fd: u32, nvhostctrl_fd: u32, hos_binder_driver: mem::SharedObject<dispdrv::HOSBinderDriver>, buffer_count: u32, display_id: vi::DisplayId, layer_id: vi::LayerId, width: u32, height: u32, color_fmt: ColorFormat, pixel_fmt: PixelFormat, layout: Layout, layer_destroy_fn: LayerDestroyFn) -> Result<Self> {
        let mut binder = binder::Binder::new(binder_handle, hos_binder_driver);
        binder.increase_refcounts()?;
        let _ = binder.connect(ConnectionApi::Cpu, false)?;
        let mut surface = Self { binder: binder, nvdrv_srv: nvdrv_srv, application_display_service: application_display_service, width: width, height: height, buffer_data: ptr::null_mut(), buffer_alloc_layout: alloc::alloc::Layout::new::<u8>(), single_buffer_size: 0, buffer_count: buffer_count, slot_has_requested: [false; MAX_BUFFERS], graphic_buf: unsafe { cmem::zeroed() }, color_fmt: color_fmt, pixel_fmt: pixel_fmt, layout: layout, display_id: display_id, layer_id: layer_id, layer_destroy_fn: layer_destroy_fn, nvhost_fd: nvhost_fd, nvmap_fd: nvmap_fd, nvhostctrl_fd: nvhostctrl_fd };
        surface.initialize()?;
        Ok(surface)
    }

    fn do_ioctl<I: ioctl::Ioctl>(&mut self, i: &mut I) -> Result<()> {
        let fd = match I::get_fd() {
            ioctl::IoctlFd::NvHost => self.nvhost_fd,
            ioctl::IoctlFd::NvMap => self.nvmap_fd,
            ioctl::IoctlFd::NvHostCtrl => self.nvhostctrl_fd,
        };

        let (in_buf, in_size) = match I::get_mode().contains(ioctl::IoctlMode::In) {
            true => (i as *mut I as *const u8, cmem::size_of::<I>()),
            false => (ptr::null::<u8>(), 0usize)
        };
        let (out_buf, out_size) = match I::get_mode().contains(ioctl::IoctlMode::Out) {
            true => (i as *mut I as *const u8, cmem::size_of::<I>()),
            false => (ptr::null::<u8>(), 0usize)
        };

        let err = self.nvdrv_srv.borrow_mut().ioctl(fd, I::get_id(), in_buf, in_size, out_buf, out_size)?;
        nv::convert_error_code(err)
    }

    fn initialize(&mut self) -> Result<()> {
        let kind = Kind::Generic_16BX2;
        let scan_fmt = DisplayScanFormat::Progressive;
        let pid: u32 = 42;
        let bpp = calculate_bpp(self.color_fmt);
        let aligned_width = align_width(bpp, self.width);
        let aligned_width_bytes = aligned_width * bpp;
        let aligned_height = align_height(self.height);
        let stride = aligned_width;
        self.single_buffer_size = (aligned_width_bytes * aligned_height) as usize;
        let usage: BitFlags<GraphicsAllocatorUsage> = GraphicsAllocatorUsage::HardwareComposer | GraphicsAllocatorUsage::HardwareRender | GraphicsAllocatorUsage::HardwareTexture;
        let buf_size = self.buffer_count as usize * self.single_buffer_size;
        self.buffer_alloc_layout = unsafe { alloc::alloc::Layout::from_size_align_unchecked(buf_size, 0x1000) };

        let mut ioctl_create: ioctl::NvMapCreate = unsafe { cmem::zeroed() };
        ioctl_create.size = buf_size as u32;
        self.do_ioctl(&mut ioctl_create)?;

        let mut ioctl_getid: ioctl::NvMapGetId = unsafe { cmem::zeroed() };
        ioctl_getid.handle = ioctl_create.handle;
        self.do_ioctl(&mut ioctl_getid)?;

        self.buffer_data = unsafe { alloc::alloc::alloc(self.buffer_alloc_layout) };
        svc::set_memory_attribute(self.buffer_data, buf_size, 8, BitFlags::from(svc::MemoryAttribute::Uncached))?;

        let mut ioctl_alloc: ioctl::NvMapAlloc = unsafe { cmem::zeroed() };
        ioctl_alloc.handle = ioctl_create.handle;
        ioctl_alloc.heap_mask = 0;
        ioctl_alloc.flags = ioctl::AllocFlags::ReadOnly;
        ioctl_alloc.align = 0x1000;
        ioctl_alloc.kind = Kind::Pitch;
        ioctl_alloc.address = self.buffer_data;
        self.do_ioctl(&mut ioctl_alloc)?;

        self.graphic_buf.header.magic = GRAPHIC_BUFFER_HEADER_MAGIC;
        self.graphic_buf.header.width = self.width;
        self.graphic_buf.header.height = self.height;
        self.graphic_buf.header.stride = stride;
        self.graphic_buf.header.pixel_format = self.pixel_fmt;
        self.graphic_buf.header.gfx_alloc_usage = usage;
        self.graphic_buf.header.pid = pid;
        self.graphic_buf.header.buffer_size = ((cmem::size_of::<GraphicBuffer>() - cmem::size_of::<GraphicBufferHeader>()) / cmem::size_of::<u32>()) as u32;
        self.graphic_buf.map_id = ioctl_getid.id;
        self.graphic_buf.magic = GRAPHIC_BUFFER_MAGIC;
        self.graphic_buf.pid = pid;
        self.graphic_buf.gfx_alloc_usage = usage;
        self.graphic_buf.pixel_format = self.pixel_fmt;
        self.graphic_buf.external_pixel_format = self.pixel_fmt;
        self.graphic_buf.stride = stride;
        self.graphic_buf.full_size = self.single_buffer_size as u32;
        self.graphic_buf.plane_count = 1;
        self.graphic_buf.planes[0].width = self.width;
        self.graphic_buf.planes[0].height = self.height;
        self.graphic_buf.planes[0].color_format = self.color_fmt;
        self.graphic_buf.planes[0].layout = self.layout;
        self.graphic_buf.planes[0].pitch = aligned_width_bytes;
        self.graphic_buf.planes[0].map_handle = ioctl_create.handle;
        self.graphic_buf.planes[0].kind = kind;
        self.graphic_buf.planes[0].block_height_log2 = BLOCK_HEIGHT_LOG2;
        self.graphic_buf.planes[0].display_scan_format = scan_fmt;
        self.graphic_buf.planes[0].size = self.single_buffer_size;

        for i in 0..self.buffer_count {
            let mut graphic_buf_copy = self.graphic_buf;
            graphic_buf_copy.planes[0].offset = i * self.single_buffer_size as u32;
            self.binder.set_preallocated_buffer(i as i32, graphic_buf_copy)?;
        }

        Ok(())
    }

    fn finalize(&mut self) -> Result<()> {
        self.binder.disconnect(ConnectionApi::Cpu, DisconnectMode::AllLocal)?;
        self.binder.decrease_refcounts()?;

        let buf_size = self.buffer_count as usize * self.single_buffer_size;
        svc::set_memory_attribute(self.buffer_data, buf_size, 0, BitFlags::empty())?;
        
        unsafe { alloc::alloc::dealloc(self.buffer_data, self.buffer_alloc_layout); }
        (self.layer_destroy_fn)(self.layer_id, self.application_display_service.clone())?;

        self.application_display_service.borrow_mut().close_display(self.display_id)?;
        Ok(())
    }

    pub fn dequeue_buffer(&mut self, is_async: bool) -> Result<(*mut u8, usize, i32, bool, MultiFence)> {
        if is_async {
            todo!();
        }
        let (slot, has_fences, fences) = self.binder.dequeue_buffer(is_async, self.width, self.height, false, self.graphic_buf.gfx_alloc_usage)?;
        
        if !self.slot_has_requested[slot as usize] {
            self.binder.request_buffer(slot)?;
            self.slot_has_requested[slot as usize] = true;
        }

        let buf = unsafe { self.buffer_data.offset((slot as usize * self.single_buffer_size) as isize) };
        Ok((buf, self.single_buffer_size, slot, has_fences, fences))
    }

    pub fn queue_buffer(&mut self, slot: i32, fences: MultiFence) -> Result<()> {
        let mut qbi: QueueBufferInput = unsafe { cmem::zeroed() };
        qbi.swap_interval = 1;
        qbi.fences = fences;

        self.binder.queue_buffer(slot, qbi)?;
        Ok(())
    }
}

impl<NS: nv::INvDrvService> Drop for Surface<NS> {
    fn drop(&mut self) {
        let _ = self.finalize();
    }
}