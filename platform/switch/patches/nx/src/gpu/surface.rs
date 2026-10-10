//! Surface (gfx wrapper) implementation

use ::alloc::string::String;
use ::alloc::sync::Arc;
use service::vi::{IManagerDisplayClient, ISystemDisplayClient};

use super::*;
use crate::gpu::binder;
use crate::gpu::ioctl;
use crate::ipc::sf;
use crate::mem::alloc;
use crate::service::dispdrv;
use crate::svc;
use crate::wait;
use core::mem as cmem;

const MAX_BUFFERS: usize = 8;

/// Configures the scaling mode for managed layers
pub enum ScaleMode {
    // Don't scale the layer
    None,
    // Native framebuffer/canvas will be scaled and stretched to fit the provided layer size
    FitToLayer { width: u32, height: u32 },
    // Native framebuffer/canvas will be scaled to fit the provided layer height, but aspect ratio will be respected
    PreseveAspect { height: u32 },
}

#[derive(Debug, Clone, Default)]
pub enum DisplayName {
    #[default]
    Default,
    Special(String),
}

impl From<DisplayName> for vi::DisplayName {
    fn from(var: DisplayName) -> vi::DisplayName {
        match var {
            DisplayName::Default => vi::DisplayName::from_str("Default"),
            DisplayName::Special(special) => vi::DisplayName::from_string(&special),
        }
    }
}

/// Represents a wrapper around layer manipulation
pub struct Surface {
    binder: binder::Binder,
    gpu_ctx: Arc<RwLock<super::Context>>,
    buffer_data: alloc::Buffer<u8>,
    slot_has_requested: [bool; MAX_BUFFERS],
    graphic_buf: GraphicBuffer,
    display_id: vi::DisplayId,
    layer_id: vi::LayerId,
    vsync_event_handle: svc::Handle,
    buffer_event_handle: svc::Handle,
    managed: bool,
}

#[allow(clippy::too_many_arguments)]
impl Surface {
    /// Create a new stray layer covering the full screen (modeled as a 1280x720 pixel surface)
    ///
    /// # Arguments:
    /// * gpu_ctx: a handle to a gpu shared context we can use to control the propeties of our layer.
    /// * display_name: a name for the spawned layer
    /// * buffer_count: the number of buffers to use. using a value of 0 will error, and a value of 1 will hang at runtime as you cannot dequeue the active buffer.
    /// * block_config: configuration value for the GPU buffer tiling height. Applications with row based rendering should set a low value and applications that draw mainly in blocks (e.g. image decoding or text) should set medium or higher values.
    /// * color_format: The color format set on the layer
    /// * pixel_format: The pixel format for the buffer. Must match the color format to run without graphical issues.
    pub fn new_stray(
        gpu_ctx: Arc<RwLock<Context>>,
        display_name: DisplayName,
        buffer_count: u32,
        block_config: BlockLinearHeights,
        color_fmt: ColorFormat,
        pixel_fmt: PixelFormat,
    ) -> Result<Self> {
        let mut gpu_guard = gpu_ctx.write();
        let display_id = gpu_guard
            .application_display_service
            .open_display(display_name.into())?;

        let (binder_handle, layer_id) = {
            let mut native_window = parcel::ParcelPayload::new();
            let (layer_id, _) = gpu_guard.application_display_service.create_stray_layer(
                vi::LayerFlags::Default(),
                display_id,
                sf::Buffer::from_other_mut_var(&mut native_window),
            )?;
            let mut parcel = parcel::Parcel::new();
            parcel.load_from(native_window);
            let binder_handle = parcel
                .read::<parcel::ParcelData>()
                .expect("ParcelData should always be readable if create_stray_layer succeeds.")
                .handle;
            (binder_handle, layer_id)
        };

        drop(gpu_guard);
        Self::new_from_parts(
            binder_handle,
            gpu_ctx,
            buffer_count,
            display_id,
            layer_id,
            SCREEN_WIDTH,
            SCREEN_HEIGHT,
            block_config,
            color_fmt,
            pixel_fmt,
            false,
        )
    }

    /// Create a new stray layer covering the full screen (modeled as a 1280x720 pixel surface)
    ///
    /// # Arguments:
    /// * gpu_ctx: a handle to a gpu shared context we can use to control the propeties of our layer.
    /// * display_name: a name for the spawned layer
    /// * aruid: the applet resource user ID for the running app (usually 0, but can be retrieved by initializing the applet module).
    /// * layer_flags: the `vi` service flags to set for the layer (usually 0).
    /// * x: the offset of the layer from the left edge of the screen.
    /// * y: the offset of the layer from the top edge of the screen.
    /// * width: width of the canvas.
    /// * height: height of the canvas.
    /// * buffer_count: the number of buffers to use. using a value of 0 will error, and a value of 1 will hang at runtime as you cannot dequeue the active buffer.
    /// * block_config: configuration value for the GPU buffer tiling height. Applications with row based rendering should set a low value and applications that draw mainly in blocks (e.g. image decoding or text) should set medium or higher values.
    /// * color_format: The color format set on the layer
    /// * pixel_format: The pixel format for the buffer. Must match the color format to run without graphical issues.
    /// * scaling: The configuration for mapping the framebuffer/canvas onto the spawned layer which may be a larger/smaller size.
    pub fn new_managed(
        gpu_ctx: Arc<RwLock<Context>>,
        display_name: DisplayName,
        aruid: applet::AppletResourceUserId,
        layer_flags: vi::LayerFlags,
        x: f32,
        y: f32,
        z: LayerZ,
        framebuffer_width: u32,
        framebuffer_height: u32,
        buffer_count: u32,
        block_config: BlockLinearHeights,
        color_fmt: ColorFormat,
        pixel_fmt: PixelFormat,
        scaling: ScaleMode,
    ) -> Result<Self> {
        let mut gpu_guard = gpu_ctx.write();

        let display_name_v = display_name.into();
        let display_id = gpu_guard
            .application_display_service
            .open_display(display_name_v)?;
        let mut system_display_service = gpu_guard
            .application_display_service
            .get_system_display_service()?;
        let mut manager_display_service = gpu_guard
            .application_display_service
            .get_manager_display_service()?;
        let mut native_window = parcel::ParcelPayload::new();

        let layer_id =
            manager_display_service.create_managed_layer(layer_flags, display_id, aruid.aruid)?;
        gpu_guard.application_display_service.open_layer(
            display_name_v,
            layer_id,
            aruid,
            sf::Buffer::from_other_mut_var(&mut native_window),
        )?;

        let binder_handle = match try {
            let mut binder_parcel = parcel::Parcel::new();
            binder_parcel.load_from(native_window);
            let binder_handle = binder_parcel.read::<parcel::ParcelData>()?.handle;
            match scaling {
                ScaleMode::None => {
                    gpu_guard
                        .application_display_service
                        .set_scaling_mode(vi::ScalingMode::None, layer_id)?;
                    system_display_service.set_layer_size(
                        layer_id,
                        framebuffer_width as u64,
                        framebuffer_height as u64,
                    )?;
                }
                ScaleMode::FitToLayer { width, height } => {
                    gpu_guard
                        .application_display_service
                        .set_scaling_mode(vi::ScalingMode::FitToLayer, layer_id)?;
                    system_display_service.set_layer_size(layer_id, width as u64, height as u64)?;
                }
                ScaleMode::PreseveAspect { height } => {
                    gpu_guard
                        .application_display_service
                        .set_scaling_mode(vi::ScalingMode::PreserveAspectRatio, layer_id)?;
                    system_display_service.set_layer_size(
                        layer_id,
                        (framebuffer_width as f32 * (height as f32) / (framebuffer_height as f32))
                            as u64,
                        height as u64,
                    )?;
                }
            }

            system_display_service.set_layer_position(x, y, layer_id)?;

            let z_value = match z {
                LayerZ::Max => system_display_service.get_z_order_count_max(display_id)?,
                LayerZ::Min => system_display_service.get_z_order_count_min(display_id)?,
                LayerZ::Value(z_val) => z_val,
            };
            system_display_service.set_layer_z(layer_id, z_value)?;

            binder_handle
        } {
            Err(e) => {
                manager_display_service.destroy_managed_layer(layer_id)?;
                return Err(e);
            }
            Ok(handle) => handle,
        };

        drop(gpu_guard);
        Self::new_from_parts(
            binder_handle,
            gpu_ctx,
            buffer_count,
            display_id,
            layer_id,
            framebuffer_width,
            framebuffer_height,
            block_config,
            color_fmt,
            pixel_fmt,
            true,
        )
    }

    /// Creates a new  [`Surface`]
    ///
    /// This is not meant to really be used manually, see [`Context`][`super::Context`]
    pub fn new_from_parts(
        binder_handle: dispdrv::BinderHandle,
        gpu_ctx: Arc<RwLock<Context>>,
        buffer_count: u32,
        display_id: vi::DisplayId,
        layer_id: vi::LayerId,
        width: u32,
        height: u32,
        block_height_config: BlockLinearHeights,
        color_fmt: ColorFormat,
        pixel_fmt: PixelFormat,
        managed: bool,
    ) -> Result<Self> {
        let mut binder =
            binder::Binder::new(binder_handle, gpu_ctx.read().hos_binder_driver.clone())?;
        binder.increase_refcounts()?;
        let _ = binder.connect(ConnectionApi::Cpu, false)?;
        let vsync_event_handle = gpu_ctx
            .read()
            .application_display_service
            .get_display_vsync_event(display_id)?;
        let buffer_event_handle =
            binder.get_native_handle(dispdrv::NativeHandleType::BufferEvent)?;
        let mut surface = Self {
            binder,
            gpu_ctx,
            buffer_data: alloc::Buffer::empty(),
            slot_has_requested: [false; MAX_BUFFERS],
            graphic_buf: Default::default(),
            display_id,
            layer_id,
            vsync_event_handle: vsync_event_handle.handle,
            buffer_event_handle: buffer_event_handle.handle,
            managed,
        };

        // Initialization
        {
            let pitch = align_up!(width * color_fmt.bytes_per_pixel(), 64u32);
            let stride = pitch / color_fmt.bytes_per_pixel();
            let aligned_height = align_up!(height, block_height_config.block_height_bytes());
            let single_buffer_size = align_up!(
                pitch as usize * aligned_height as usize,
                alloc::PAGE_ALIGNMENT
            );
            let total_framebuffer_size = buffer_count as usize * single_buffer_size;

            surface.buffer_data =
                alloc::Buffer::new(alloc::PAGE_ALIGNMENT, total_framebuffer_size)?;

            let mut ioctl_create = ioctl::NvMapCreate {
                size: total_framebuffer_size as u32,
                ..Default::default()
            };
            surface.do_ioctl(&mut ioctl_create)?;

            let mut ioctl_getid = ioctl::NvMapGetId {
                handle: ioctl_create.handle,
                ..Default::default()
            };
            surface.do_ioctl(&mut ioctl_getid)?;

            let mut ioctl_alloc = ioctl::NvMapAlloc {
                handle: ioctl_create.handle,
                heap_mask: 0,
                flags: ioctl::AllocFlags::ReadOnly,
                align: alloc::PAGE_ALIGNMENT as u32,
                kind: Kind::Pitch,
                address: surface.buffer_data.ptr.expose_provenance(),
                ..Default::default()
            };
            surface.do_ioctl(&mut ioctl_alloc)?;

            unsafe {
                nx::arm::cache_flush(surface.buffer_data.ptr, total_framebuffer_size);
                svc::set_memory_attribute(surface.buffer_data.ptr, total_framebuffer_size, true)?;
            }

            let usage = GraphicsAllocatorUsage::HardwareComposer()
                | GraphicsAllocatorUsage::HardwareRender()
                | GraphicsAllocatorUsage::HardwareTexture();

            surface.graphic_buf.header.magic = GraphicBufferHeader::MAGIC;
            surface.graphic_buf.header.width = width;
            surface.graphic_buf.header.height = height;
            surface.graphic_buf.header.stride = stride;
            surface.graphic_buf.header.pixel_format = pixel_fmt;
            surface.graphic_buf.header.gfx_alloc_usage = usage;
            surface.graphic_buf.header.pid = 42;
            surface.graphic_buf.header.buffer_size =
                ((cmem::size_of::<GraphicBuffer>() - cmem::size_of::<GraphicBufferHeader>())
                    / cmem::size_of::<u32>()) as u32;
            surface.graphic_buf.unknown = -1;
            surface.graphic_buf.map_id = ioctl_getid.id;
            surface.graphic_buf.magic = GraphicBuffer::MAGIC;
            surface.graphic_buf.pid = 42;
            surface.graphic_buf.gfx_alloc_usage = usage;
            surface.graphic_buf.pixel_format = pixel_fmt;
            surface.graphic_buf.external_pixel_format = pixel_fmt;
            surface.graphic_buf.stride = stride;
            surface.graphic_buf.full_size = single_buffer_size as u32;
            surface.graphic_buf.plane_count = 1;
            surface.graphic_buf.unk2 = 0;
            surface.graphic_buf.planes[0].width = width;
            surface.graphic_buf.planes[0].height = height;
            surface.graphic_buf.planes[0].color_format = color_fmt;
            surface.graphic_buf.planes[0].layout = Layout::BlockLinear;
            surface.graphic_buf.planes[0].pitch = pitch;
            surface.graphic_buf.planes[0].map_handle = ioctl_create.handle;
            surface.graphic_buf.planes[0].kind = Kind::Generic_16BX2;
            surface.graphic_buf.planes[0].block_height_log2 = block_height_config;
            surface.graphic_buf.planes[0].display_scan_format = DisplayScanFormat::Progressive;
            surface.graphic_buf.planes[0].size = single_buffer_size;

            for i in 0..buffer_count {
                let mut graphic_buf_copy = surface.graphic_buf;
                graphic_buf_copy.planes[0].offset = i * graphic_buf_copy.full_size;
                surface
                    .binder
                    .set_preallocated_buffer(i as i32, graphic_buf_copy)?;
            }
        }

        Ok(surface)
    }

    fn do_ioctl<I: ioctl::Ioctl>(&mut self, i: &mut I) -> Result<()> {
        let gpu_guard = self.gpu_ctx.read();
        let fd = match I::get_fd() {
            ioctl::IoctlFd::NvHost => gpu_guard.nvhost_fd,
            ioctl::IoctlFd::NvMap => gpu_guard.nvmap_fd,
            ioctl::IoctlFd::NvHostCtrl => gpu_guard.nvhostctrl_fd,
        };

        let err =
            gpu_guard
                .nvdrv_service
                .ioctl(fd, I::get_id(), sf::Buffer::from_other_mut_var(i))?;
        super::convert_nv_error_code(err)
    }

    pub fn get_block_linear_config(&self) -> BlockLinearHeights {
        self.graphic_buf.planes[0].block_height_log2
    }

    /// Dequeues a buffer, returning the buffer address, its size, its slot, whether it has fences, and those mentioned fences
    ///
    /// # Arguments
    ///
    /// * `is_async`: Whether to dequeue asynchronously
    pub fn dequeue_buffer(
        &mut self,
        is_async: bool,
    ) -> Result<(*mut u8, usize, i32, bool, MultiFence)> {
        let slot: i32;
        let has_fences: bool;
        let fences: MultiFence;
        if is_async {
            self.wait_buffer_event(-1)?;
            loop {
                match self.binder.dequeue_buffer(
                    true,
                    self.width(),
                    self.height(),
                    false,
                    self.graphic_buf.gfx_alloc_usage,
                ) {
                    Ok((_slot, _has_fences, _fences)) => {
                        slot = _slot;
                        has_fences = _has_fences;
                        fences = _fences;
                        break;
                    }
                    Err(rc) => {
                        if binder::rc::ResultErrorCodeWouldBlock::matches(rc) {
                            continue;
                        }
                        return Err(rc);
                    }
                };
            }
        } else {
            let (_slot, _has_fences, _fences) = self.binder.dequeue_buffer(
                false,
                self.width(),
                self.height(),
                false,
                self.graphic_buf.gfx_alloc_usage,
            )?;
            slot = _slot;
            has_fences = _has_fences;
            fences = _fences;
        }

        if !self.slot_has_requested[slot as usize] {
            self.binder.request_buffer(slot)?;
            self.slot_has_requested[slot as usize] = true;
        }

        let buf = unsafe {
            self.buffer_data
                .ptr
                .add(slot as usize * self.graphic_buf.full_size as usize)
        };
        Ok((
            buf,
            self.graphic_buf.full_size as usize,
            slot,
            has_fences,
            fences,
        ))
    }

    /// Queues a buffer
    ///
    /// # Arguments
    ///
    /// * `slot`: The buffer slot
    /// * `fences`: The buffer fences
    pub fn queue_buffer(&mut self, slot: i32, fences: MultiFence) -> Result<()> {
        let qbi = QueueBufferInput {
            swap_interval: 1,
            fences,
            ..Default::default()
        };

        nx::arm::cache_flush(
            unsafe {
                self.buffer_data
                    .ptr
                    .add(self.graphic_buf.full_size as usize * slot as usize)
            },
            self.graphic_buf.full_size as usize,
        );

        self.binder.queue_buffer(slot, qbi)?;
        Ok(())
    }

    /// Waits for the given fences
    ///
    /// # Arguments
    ///
    /// * `fences`: The fences
    /// * `timeout`: The wait timeout
    pub fn wait_fences(&mut self, fences: MultiFence, timeout: i32) -> Result<()> {
        for fence in fences.fences[..fences.fence_count as usize].iter().cloned() {
            let mut ioctl_syncptwait = ioctl::NvHostCtrlSyncptWait { fence, timeout };

            if self.do_ioctl(&mut ioctl_syncptwait).is_err() {
                // Don't error, but stop waiting for fences
                break;
            }
        }
        Ok(())
    }

    /// Sets whether the surface (its layer) is visible
    ///
    /// # Arguments
    ///
    /// * `visible`: Whether its visible
    pub fn set_visible(&mut self, visible: bool) -> Result<()> {
        self.gpu_ctx
            .read()
            .application_display_service
            .get_system_display_service()?
            .set_layer_visibility(visible, self.layer_id)
    }

    /// Adds a layer visibility stack
    ///
    /// The Management display server maintains a stack of visible layers. This allows us to order our layer relative to other elements (e.g. visible or invisible to screenshots/recordings)
    ///
    /// # Atguments
    ///
    /// * `stack_type_id`: the type of layer to add to the visibility stack
    pub fn push_layer_stack(&mut self, stack_type_id: vi::LayerStackId) -> Result<()> {
        self.gpu_ctx
            .read()
            .application_display_service
            .get_manager_display_service()?
            .add_to_layer_stack(stack_type_id, self.layer_id)
    }

    /// Waits for the buffer event
    ///
    /// # Arguments
    ///
    /// * `timeout`: The wait timeout
    pub fn wait_buffer_event(&self, timeout: i64) -> Result<()> {
        wait::wait_handles(&[self.buffer_event_handle], timeout)?;
        svc::reset_signal(self.buffer_event_handle)
    }

    /// Waits for the vsync event
    ///
    /// # Arguments
    ///
    /// * `timeout`: The wait timeout
    pub fn wait_vsync_event(&self, timeout: i64) -> Result<()> {
        wait::wait_handles(&[self.vsync_event_handle], timeout)?;
        svc::reset_signal(self.vsync_event_handle)
    }

    /// Gets the surface width
    #[inline]
    pub const fn width(&self) -> u32 {
        self.graphic_buf.header.width
    }

    /// Gets the surface height
    #[inline]
    pub const fn height(&self) -> u32 {
        self.graphic_buf.header.height
    }

    /// Gets the surface pitch (in bytes)
    #[inline]
    pub const fn pitch(&self) -> u32 {
        self.graphic_buf.planes[0].pitch
    }

    /// Gets the surface [`ColorFormat`]
    #[inline]
    pub const fn color_format(&self) -> ColorFormat {
        self.graphic_buf.planes[0].color_format
    }

    /// Computes and gets the surface stride (distance between adjacent rows in pixels, incliuding padding).
    pub const fn stride(&self) -> u32 {
        self.pitch() / self.color_format().bytes_per_pixel()
    }

    #[inline]
    pub const fn single_buffer_size(&self) -> u32 {
        self.graphic_buf.full_size
    }
}

impl Drop for Surface {
    /// Destroys the surface, closing everything it internally opened
    #[allow(unused_must_use)] // we can't return from Drop, so just ignore result codes
    fn drop(&mut self) {
        self.binder
            .disconnect(ConnectionApi::Cpu, DisconnectMode::AllLocal);
        self.binder.decrease_refcounts();

        let mut ioctl_free = ioctl::NvMapFree {
            handle: self.graphic_buf.planes[0].map_handle,
            ..Default::default()
        };
        let _ = self.do_ioctl(&mut ioctl_free);

        unsafe {
            svc::set_memory_attribute(self.buffer_data.ptr, self.buffer_data.layout.size(), false)
        };

        let mut gpu_guard = self.gpu_ctx.write();
        let _layer_close_result = if self.managed {
            gpu_guard.application_display_service.get_manager_display_service().expect("We successfully created a managed layer, we should always then be able to recall the manager service").destroy_managed_layer(self.layer_id)
        } else {
            gpu_guard
                .application_display_service
                .destroy_stray_layer(self.layer_id)
        };

        gpu_guard
            .application_display_service
            .close_display(self.display_id);

        svc::close_handle(self.buffer_event_handle);
        svc::close_handle(self.vsync_event_handle);
    }
}
