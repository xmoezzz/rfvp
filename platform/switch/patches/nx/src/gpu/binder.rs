//! Binder support and utils

use super::*;
use crate::gpu::parcel;
use crate::ipc::sf;
use crate::service::dispdrv;

pub mod rc;

/// Represents the interface token used for parcel transactions
pub const INTERFACE_TOKEN: &str = "android.gui.IGraphicBufferProducer";

/// Represents binder error code values.
/// See: 🤷 TODO: Find where these values come from. They're different than libnx.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(i32)]
#[allow(missing_docs)]
pub enum ErrorCode {
    #[default]
    Success = 0,
    PermissionDenied = -1,
    NameNotFound = -2,
    WouldBlock = -11,
    NoMemory = -12,
    AlreadyExists = -17,
    NoInit = -19,
    BadValue = -22,
    DeadObject = -32,
    InvalidOperation = -38,
    NotEnoughData = -61,
    UnknownTransaction = -74,
    BadIndex = -75,
    TimeOut = -110,
    FdsNotAllowed = -2147483641,
    FailedTransaction = -2147483646,
    BadType = -2147483647,
}

/// Converts [`ErrorCode`]s to result values
#[allow(unreachable_patterns)]
pub fn convert_nv_error_code(err: ErrorCode) -> Result<()> {
    match err {
        ErrorCode::Success => Ok(()),
        ErrorCode::PermissionDenied => rc::ResultErrorCodePermissionDenied::make_err(),
        ErrorCode::NameNotFound => rc::ResultErrorCodeNameNotFound::make_err(),
        ErrorCode::WouldBlock => rc::ResultErrorCodeWouldBlock::make_err(),
        ErrorCode::NoMemory => rc::ResultErrorCodeNoMemory::make_err(),
        ErrorCode::AlreadyExists => rc::ResultErrorCodeAlreadyExists::make_err(),
        ErrorCode::NoInit => rc::ResultErrorCodeNoInit::make_err(),
        ErrorCode::BadValue => rc::ResultErrorCodeBadValue::make_err(),
        ErrorCode::DeadObject => rc::ResultErrorCodeDeadObject::make_err(),
        ErrorCode::InvalidOperation => rc::ResultErrorCodeInvalidOperation::make_err(),
        ErrorCode::NotEnoughData => rc::ResultErrorCodeNotEnoughData::make_err(),
        ErrorCode::UnknownTransaction => rc::ResultErrorCodeUnknownTransaction::make_err(),
        ErrorCode::BadIndex => rc::ResultErrorCodeBadIndex::make_err(),
        ErrorCode::TimeOut => rc::ResultErrorCodeTimeOut::make_err(),
        ErrorCode::FdsNotAllowed => rc::ResultErrorCodeFdsNotAllowed::make_err(),
        ErrorCode::FailedTransaction => rc::ResultErrorCodeFailedTransaction::make_err(),
        ErrorCode::BadType => rc::ResultErrorCodeBadType::make_err(),
        _ => rc::ResultErrorCodeInvalid::make_err(),
    }
}

/// Represents a binder object, wrapping transaction functionality
pub struct Binder {
    handle: dispdrv::BinderHandle,
    hos_binder_driver: Arc<dyn dispdrv::IHOSBinderDriverClient>,
}

impl Binder {
    /// Creates a new [`Binder`]
    ///
    /// # Arguments
    ///
    /// * `handle`: Binder handle to use
    /// * `hos_binder_driver`: [`IHOSBinderDriver`][`dispdrv::IHOSBinderDriverClient`] object
    #[inline]
    pub const fn new(
        handle: dispdrv::BinderHandle,
        hos_binder_driver: Arc<dyn dispdrv::IHOSBinderDriverClient>,
    ) -> Result<Self> {
        Ok(Self {
            handle,
            hos_binder_driver,
        })
    }

    fn transact_parcel_begin(&self, parcel: &mut parcel::Parcel) -> Result<()> {
        parcel.write_interface_token(INTERFACE_TOKEN)
    }

    fn transact_parcel_check_err(&mut self, parcel: &mut parcel::Parcel) -> Result<()> {
        let err: ErrorCode = parcel.read()?;
        convert_nv_error_code(err)?;
        Ok(())
    }

    fn transact_parcel_impl(
        &mut self,
        transaction_id: dispdrv::ParcelTransactionId,
        payload: parcel::ParcelPayload,
    ) -> Result<parcel::Parcel> {
        let mut response_payload = parcel::ParcelPayload::new();
        self.hos_binder_driver.transact_parcel(
            self.handle,
            transaction_id,
            0,
            sf::Buffer::from_other_var(&payload),
            sf::Buffer::from_other_mut_var(&mut response_payload),
        )?;

        let mut parcel = parcel::Parcel::new();
        parcel.load_from(response_payload);
        Ok(parcel)
    }

    fn transact_parcel(
        &mut self,
        transaction_id: dispdrv::ParcelTransactionId,
        parcel: &mut parcel::Parcel,
    ) -> Result<parcel::Parcel> {
        let (payload, _payload_size) = parcel.end_write()?;
        self.transact_parcel_impl(transaction_id, payload)
    }

    /// Gets this [`Binder`]'s handle
    #[inline]
    pub fn get_handle(&self) -> dispdrv::BinderHandle {
        self.handle
    }

    /// Gets this [`Binder`]'s underlying [`IHOSBinderDriverClient`][`dispdrv::IHOSBinderDriverClient`] object
    pub fn get_hos_binder_driver(&self) -> Arc<dyn dispdrv::IHOSBinderDriverClient> {
        self.hos_binder_driver.clone()
    }

    /// Increases the [`Binder`]'s reference counts
    pub fn increase_refcounts(&mut self) -> Result<()> {
        self.hos_binder_driver
            .adjust_refcount(self.handle, 1, dispdrv::RefcountType::Weak)?;
        self.hos_binder_driver
            .adjust_refcount(self.handle, 1, dispdrv::RefcountType::Strong)
    }

    /// Decreases the [`Binder`]'s reference counts
    pub fn decrease_refcounts(&mut self) -> Result<()> {
        self.hos_binder_driver
            .adjust_refcount(self.handle, -1, dispdrv::RefcountType::Weak)?;
        self.hos_binder_driver
            .adjust_refcount(self.handle, -1, dispdrv::RefcountType::Strong)
    }

    /// Performs a connection
    ///
    /// # Arguments
    ///
    /// * `api`: The connection API to use
    /// * `producer_controlled_by_app`: Whether the producer is controlled by the process itself
    pub fn connect(
        &mut self,
        api: ConnectionApi,
        producer_controlled_by_app: bool,
    ) -> Result<QueueBufferOutput> {
        let mut parcel = parcel::Parcel::new();
        self.transact_parcel_begin(&mut parcel)?;

        let producer_listener: u32 = 0;
        parcel.write(producer_listener)?;
        parcel.write(api)?;
        parcel.write(producer_controlled_by_app as u32)?;

        let mut response_parcel =
            self.transact_parcel(dispdrv::ParcelTransactionId::Connect, &mut parcel)?;
        let qbo: QueueBufferOutput = response_parcel.read()?;

        self.transact_parcel_check_err(&mut response_parcel)?;
        Ok(qbo)
    }

    /// Performs a disconnection
    ///
    /// # Arguments
    ///
    /// * `api`: The connection API
    /// * `mode`: The disconnection mode
    pub fn disconnect(&mut self, api: ConnectionApi, mode: DisconnectMode) -> Result<()> {
        let mut parcel = parcel::Parcel::new();
        self.transact_parcel_begin(&mut parcel)?;

        parcel.write(api)?;
        parcel.write(mode)?;

        let mut response_parcel =
            self.transact_parcel(dispdrv::ParcelTransactionId::Disconnect, &mut parcel)?;

        self.transact_parcel_check_err(&mut response_parcel)?;
        Ok(())
    }

    /// Sets a preallocated buffer
    ///
    /// # Arguments
    ///
    /// * `slot`: The buffer slot
    /// * `buf`: The buffer
    pub fn set_preallocated_buffer(&mut self, slot: i32, buf: GraphicBuffer) -> Result<()> {
        let mut parcel = parcel::Parcel::new();
        self.transact_parcel_begin(&mut parcel)?;

        parcel.write(slot)?;
        let has_input = true;
        parcel.write(has_input as u32)?;
        if has_input {
            parcel.write_sized(buf)?;
        }

        self.transact_parcel(
            dispdrv::ParcelTransactionId::SetPreallocatedBuffer,
            &mut parcel,
        )?;
        Ok(())
    }

    /// Requests a buffer at a given slot
    ///
    /// This also returns whether the buffer is non-null
    ///
    /// # Arguments
    ///
    /// * `slot`: The slot
    pub fn request_buffer(&mut self, slot: i32) -> Result<(bool, GraphicBuffer)> {
        let mut parcel = parcel::Parcel::new();
        self.transact_parcel_begin(&mut parcel)?;

        parcel.write(slot)?;

        let mut response_parcel =
            self.transact_parcel(dispdrv::ParcelTransactionId::RequestBuffer, &mut parcel)?;
        let non_null_v: u32 = response_parcel.read()?;
        let non_null = non_null_v != 0;
        let mut gfx_buf: GraphicBuffer = Default::default();
        if non_null {
            gfx_buf = response_parcel.read_sized()?;
        }

        self.transact_parcel_check_err(&mut response_parcel)?;
        Ok((non_null, gfx_buf))
    }

    /// Dequeues a buffer
    ///
    /// # Arguments
    ///
    /// * `is_async`: Whether the dequeue is asynchronous
    /// * `width`: The width
    /// * `height`: The height
    /// * `get_frame_timestamps`: Whether to get frame timestamps
    /// * `usage`: [`GraphicsAllocatorUsage`] value
    pub fn dequeue_buffer(
        &mut self,
        is_async: bool,
        width: u32,
        height: u32,
        get_frame_timestamps: bool,
        usage: GraphicsAllocatorUsage,
    ) -> Result<(i32, bool, MultiFence)> {
        let mut parcel = parcel::Parcel::new();
        self.transact_parcel_begin(&mut parcel)?;

        parcel.write(is_async as u32)?;
        parcel.write(width)?;
        parcel.write(height)?;
        parcel.write(get_frame_timestamps as u32)?;
        parcel.write(usage)?;

        let mut response_parcel =
            self.transact_parcel(dispdrv::ParcelTransactionId::DequeueBuffer, &mut parcel)?;

        let slot: i32 = response_parcel.read()?;
        let has_fences_v: u32 = response_parcel.read()?;
        let has_fences = has_fences_v != 0;
        let mut fences: MultiFence = Default::default();
        if has_fences {
            fences = response_parcel.read_sized()?;
        }

        self.transact_parcel_check_err(&mut response_parcel)?;
        Ok((slot, has_fences, fences))
    }

    /// Queues a buffer
    ///
    /// # Arguments
    ///
    /// * `slot`: The slot
    /// * `qbi`: The input layout
    pub fn queue_buffer(&mut self, slot: i32, qbi: QueueBufferInput) -> Result<QueueBufferOutput> {
        let mut parcel = parcel::Parcel::new();
        self.transact_parcel_begin(&mut parcel)?;

        parcel.write(slot)?;
        parcel.write_sized(qbi)?;

        let mut response_parcel =
            self.transact_parcel(dispdrv::ParcelTransactionId::QueueBuffer, &mut parcel)?;

        let qbo = response_parcel.read()?;

        self.transact_parcel_check_err(&mut response_parcel)?;
        Ok(qbo)
    }

    /// Gets a native handle of the underlying [`IHOSBinderDriver`][`dispdrv::IHOSBinderDriverClient`] object
    ///
    /// # Arguments
    ///
    /// * `handle_type`: The [`NativeHandleType`][`dispdrv::NativeHandleType`] value
    pub fn get_native_handle(
        &mut self,
        handle_type: dispdrv::NativeHandleType,
    ) -> Result<sf::CopyHandle> {
        self.hos_binder_driver
            .get_native_handle(self.handle, handle_type)
    }
}
