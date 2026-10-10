use super::*;
use crate::sync::Mutex;
use crate::wait;
use alloc::rc::Rc;
use alloc::vec::Vec;
use sf::hipc::IHipcManager;
use sf::hipc::IMitmQueryServiceServer;

/// This flags, but is actually necessary
#[allow(unused_imports)]
use sf::sm::IUserInterfaceClient;

#[cfg(feature = "services")]
use crate::service;

#[cfg(not(feature = "services"))]
use crate::ipc::sf::sm;
#[cfg(feature = "services")]
use crate::service::sm;

pub mod rc;

// TODO: TIPC support, implement remaining control commands

const MAX_COUNT: usize = wait::MAX_OBJECT_COUNT as usize;

pub struct ServerContext<'ctx> {
    pub ctx: &'ctx mut CommandContext,
    pub raw_data_walker: DataWalker,
    pub domain_table: Option<Rc<Mutex<DomainTable>>>,
    pub new_sessions: &'ctx mut Vec<ServerHolder>,
}

impl<'ctx> ServerContext<'ctx> {
    pub const fn new(
        ctx: &'ctx mut CommandContext,
        raw_data_walker: DataWalker,
        domain_table: Option<Rc<Mutex<DomainTable>>>,
        new_sessions: &'ctx mut Vec<ServerHolder>,
    ) -> Self {
        Self {
            ctx,
            raw_data_walker,
            domain_table,
            new_sessions,
        }
    }
}

pub trait RequestCommandParameter<'ctx, O> {
    fn after_request_read(ctx: &mut ServerContext<'ctx>) -> Result<O>;
}

pub trait ResponseCommandParameter {
    type CarryState: 'static;
    fn before_response_write(var: &Self, ctx: &mut ServerContext) -> Result<Self::CarryState>;
    fn after_response_write(
        var: Self,
        carry_state: Self::CarryState,
        ctx: &mut ServerContext,
    ) -> Result<()>;
}

impl<
    'buf,
    'ctx: 'buf,
    const IN: bool,
    const OUT: bool,
    const MAP_ALIAS: bool,
    const POINTER: bool,
    const FIXED_SIZE: bool,
    const AUTO_SELECT: bool,
    const ALLOW_NON_SECURE: bool,
    const ALLOW_NON_DEVICE: bool,
    T,
>
    RequestCommandParameter<
        'ctx,
        sf::Buffer<
            'buf,
            IN,
            OUT,
            MAP_ALIAS,
            POINTER,
            FIXED_SIZE,
            AUTO_SELECT,
            ALLOW_NON_SECURE,
            ALLOW_NON_DEVICE,
            T,
        >,
    >
    for sf::Buffer<
        'buf,
        IN,
        OUT,
        MAP_ALIAS,
        POINTER,
        FIXED_SIZE,
        AUTO_SELECT,
        ALLOW_NON_SECURE,
        ALLOW_NON_DEVICE,
        T,
    >
{
    fn after_request_read(ctx: &mut ServerContext<'ctx>) -> Result<Self> {
        let (addr, size) = ctx.ctx.pop_buffer::<IN,
        OUT,
        MAP_ALIAS,
        POINTER,
        FIXED_SIZE,
        AUTO_SELECT,
        ALLOW_NON_SECURE,
        ALLOW_NON_DEVICE,
        T>(&mut ctx.raw_data_walker)?;

        if OUT && POINTER {
            // For Out(Fixed)Pointer buffers, we need to send them back as InPointer
            // Note: since buffers can't be out params in this command param system, we need to send them back this way

            // SAFETY - This should be safe as we're only copying the buffer back into the context and not duplicating access to the buffer.
            // If we ever actually access that cloned buffer, it's instant UB
            let in_ptr_buf = unsafe { sf::InPointerBuffer::<u8>::new(addr, size) };
            ctx.ctx.add_buffer(&in_ptr_buf)?;
        }

        Ok(unsafe { sf::Buffer::new(addr, size) })
    }
}

//impl<const A: BufferAttribute, T> !ResponseCommandParameter for sf::Buffer<A, T> {}

impl<const MOVE: bool> RequestCommandParameter<'_, sf::Handle<MOVE>> for sf::Handle<MOVE> {
    fn after_request_read(ctx: &mut ServerContext) -> Result<Self> {
        ctx.ctx.in_params.pop_handle::<MOVE>()
    }
}

impl<const MOVE: bool> ResponseCommandParameter for sf::Handle<MOVE> {
    type CarryState = ();

    fn before_response_write(handle: &Self, ctx: &mut ServerContext) -> Result<()> {
        ctx.ctx.out_params.push_handle(handle.clone())
    }

    fn after_response_write(
        _handle: Self,
        _carry_state: (),
        _ctx: &mut ServerContext,
    ) -> Result<()> {
        Ok(())
    }
}

impl RequestCommandParameter<'_, sf::ProcessId> for sf::ProcessId {
    fn after_request_read(ctx: &mut ServerContext) -> Result<Self> {
        if ctx.ctx.in_params.send_process_id {
            if ctx.ctx.object_info.uses_cmif_protocol() {
                let _ = ctx.raw_data_walker.advance_get::<u64>();
            }

            Ok(sf::ProcessId::from(ctx.ctx.in_params.process_id))
        } else {
            sf::hipc::rc::ResultUnsupportedOperation::make_err()
        }
    }
}

//impl !ResponseCommandParameter for sf::ProcessId {}

impl RequestCommandParameter<'_, sf::AppletResourceUserId> for sf::AppletResourceUserId {
    fn after_request_read(ctx: &mut ServerContext) -> Result<Self> {
        result_return_unless!(
            ctx.ctx.object_info.uses_cmif_protocol(),
            sf::hipc::rc::ResultUnsupportedOperation
        );

        if ctx.ctx.in_params.send_process_id {
            Ok(sf::AppletResourceUserId::from(
                ctx.ctx.in_params.process_id,
                ctx.raw_data_walker.advance_get::<u64>(),
            ))
        } else {
            sf::hipc::rc::ResultUnsupportedOperation::make_err()
        }
    }
}

//impl !ResponseCommandParameter for sf::AppletResourceUserId {}

impl<S: ISessionObject + 'static> ResponseCommandParameter for S {
    type CarryState = u32;
    fn before_response_write(_session: &Self, ctx: &mut ServerContext) -> Result<u32> {
        if ctx.ctx.object_info.is_domain() {
            let domain_object_id = ctx
                .domain_table
                .as_mut()
                .ok_or(rc::ResultDomainNotFound::make())?
                .lock()
                .allocate_id()?;
            ctx.ctx.out_params.push_domain_object(domain_object_id)?;
            Ok(domain_object_id)
        } else {
            let (server_handle, client_handle) = svc::create_session(false, 0)?;
            ctx.ctx
                .out_params
                .push_handle(sf::MoveHandle::from(client_handle))?;
            Ok(server_handle)
        }
    }

    fn after_response_write(
        session: Self,
        carry_state: u32,
        ctx: &mut ServerContext,
    ) -> Result<()> {
        if ctx.ctx.object_info.is_domain() {
            ctx.domain_table
                .as_mut()
                .ok_or(rc::ResultDomainNotFound::make())?
                .lock()
                .domains
                .push(ServerHolder::new_domain_session(
                    0,
                    carry_state,
                    Rc::new(Mutex::new(session)),
                ))
        } else {
            ctx.new_sessions.push(ServerHolder::new_session(
                carry_state,
                Rc::new(Mutex::new(session)),
            ));
        }
        Ok(())
    }
}

pub trait ISessionObject {
    fn try_handle_request_by_id(
        &mut self,
        req_id: u32,
        protocol: CommandProtocol,
        server_ctx: &mut ServerContext,
    ) -> Option<Result<()>>;
}

pub trait IServerObject: ISessionObject {
    fn new() -> Self
    where
        Self: Sized;
}

pub trait IMitmServerObject: ISessionObject {
    fn new(info: sm::mitm::MitmProcessInfo) -> Self
    where
        Self: Sized;
}

pub type NewServerFn = fn() -> Rc<Mutex<dyn ISessionObject>>;

fn create_server_object_impl<S: IServerObject + 'static>() -> Rc<Mutex<dyn ISessionObject>> {
    Rc::new(Mutex::new(S::new()))
}

pub type NewMitmServerFn = fn(sm::mitm::MitmProcessInfo) -> Rc<Mutex<dyn ISessionObject>>;

fn create_mitm_server_object_impl<S: IMitmServerObject + 'static>(
    info: sm::mitm::MitmProcessInfo,
) -> Rc<Mutex<dyn ISessionObject>> {
    Rc::new(Mutex::new(S::new(info)))
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum WaitHandleType {
    Server,
    Session,
}

#[derive(Default)]
pub struct DomainTable {
    pub table: Vec<cmif::DomainObjectId>,
    pub domains: Vec<ServerHolder>,
}

impl DomainTable {
    pub fn new() -> Self {
        Self {
            table: Vec::new(),
            domains: Vec::new(),
        }
    }

    pub fn allocate_id(&mut self) -> Result<cmif::DomainObjectId> {
        let mut current_id: cmif::DomainObjectId = 1;
        loop {
            // Note: fix potential infinite loops here?
            if !self.table.contains(&current_id) {
                self.table.push(current_id);
                return Ok(current_id);
            }
            current_id += 1;
        }
    }

    pub fn allocate_specific_id(
        &mut self,
        specific_domain_object_id: cmif::DomainObjectId,
    ) -> Result<cmif::DomainObjectId> {
        if !self.table.contains(&specific_domain_object_id) {
            self.table.push(specific_domain_object_id);
            return Ok(specific_domain_object_id);
        }

        rc::ResultObjectIdAlreadyAllocated::make_err()
    }

    pub fn find_domain(
        &mut self,
        id: cmif::DomainObjectId,
    ) -> Result<Rc<Mutex<dyn ISessionObject>>> {
        for holder in &self.domains {
            if holder.info.domain_object_id == id {
                return holder
                    .server
                    .clone()
                    .ok_or(rc::ResultDomainNotFound::make());
            }
        }

        rc::ResultDomainNotFound::make_err()
    }

    pub fn deallocate_domain(&mut self, domain_object_id: cmif::DomainObjectId) {
        self.table.retain(|&id| id != domain_object_id);
        self.domains
            .retain(|holder| holder.info.domain_object_id != domain_object_id);
    }
}

pub struct ServerHolder {
    pub server: Option<Rc<Mutex<dyn ISessionObject>>>,
    pub info: ObjectInfo,
    pub new_server_fn: Option<NewServerFn>,
    pub new_mitm_server_fn: Option<NewMitmServerFn>,
    pub handle_type: WaitHandleType,
    pub mitm_forward_info: ObjectInfo,
    pub is_mitm_service: bool,
    pub service_name: sm::ServiceName,
    pub domain_table: Option<Rc<Mutex<DomainTable>>>,
}

impl ServerHolder {
    pub fn new_session(handle: svc::Handle, object: Rc<Mutex<dyn ISessionObject>>) -> Self {
        Self {
            server: Some(object),
            info: ObjectInfo::from_handle(handle),
            new_server_fn: None,
            new_mitm_server_fn: None,
            handle_type: WaitHandleType::Session,
            mitm_forward_info: ObjectInfo::new(),
            is_mitm_service: false,
            service_name: sm::ServiceName::empty(),
            domain_table: None,
        }
    }

    pub fn new_domain_session(
        handle: svc::Handle,
        domain_object_id: cmif::DomainObjectId,
        object: Rc<Mutex<dyn ISessionObject>>,
    ) -> Self {
        Self {
            server: Some(object),
            info: ObjectInfo::from_domain_object_id(handle, domain_object_id),
            new_server_fn: None,
            new_mitm_server_fn: None,
            handle_type: WaitHandleType::Session,
            mitm_forward_info: ObjectInfo::new(),
            is_mitm_service: false,
            service_name: sm::ServiceName::empty(),
            domain_table: None,
        }
    }

    pub fn new_server<S: IServerObject + 'static>(
        handle: svc::Handle,
        service_name: sm::ServiceName,
    ) -> Self {
        Self {
            server: None,
            info: ObjectInfo::from_handle(handle),
            new_server_fn: Some(create_server_object_impl::<S>),
            new_mitm_server_fn: None,
            handle_type: WaitHandleType::Server,
            mitm_forward_info: ObjectInfo::new(),
            is_mitm_service: false,
            service_name,
            domain_table: None,
        }
    }

    pub fn new_mitm_server<S: IMitmServerObject + 'static>(
        handle: svc::Handle,
        service_name: sm::ServiceName,
    ) -> Self {
        Self {
            server: None,
            info: ObjectInfo::from_handle(handle),
            new_server_fn: None,
            new_mitm_server_fn: Some(create_mitm_server_object_impl::<S>),
            handle_type: WaitHandleType::Server,
            mitm_forward_info: ObjectInfo::new(),
            is_mitm_service: true,
            service_name,
            domain_table: None,
        }
    }

    pub fn make_new_session(&self, handle: svc::Handle) -> Result<Self> {
        let new_fn = self.get_new_server_fn()?;
        Ok(Self {
            server: Some((new_fn)()),
            info: ObjectInfo::from_handle(handle),
            new_server_fn: self.new_server_fn,
            new_mitm_server_fn: self.new_mitm_server_fn,
            handle_type: WaitHandleType::Session,
            mitm_forward_info: ObjectInfo::new(),
            is_mitm_service: self.is_mitm_service,
            service_name: sm::ServiceName::empty(),
            domain_table: None,
        })
    }

    pub fn make_new_mitm_session(
        &self,
        handle: svc::Handle,
        forward_handle: svc::Handle,
        info: sm::mitm::MitmProcessInfo,
        service_name: sm::ServiceName,
    ) -> Result<Self> {
        let new_mitm_fn = self.get_new_mitm_server_fn()?;
        Ok(Self {
            server: Some((new_mitm_fn)(info)),
            info: ObjectInfo::from_handle(handle),
            new_server_fn: self.new_server_fn,
            new_mitm_server_fn: self.new_mitm_server_fn,
            handle_type: WaitHandleType::Session,
            mitm_forward_info: ObjectInfo::from_handle(forward_handle),
            is_mitm_service: self.is_mitm_service,
            service_name,
            domain_table: None,
        })
    }

    pub fn clone_self(&self, handle: svc::Handle, forward_handle: svc::Handle) -> Result<Self> {
        let mut object_info = self.info;
        object_info.handle = handle;
        let mut mitm_fwd_info = self.mitm_forward_info;
        mitm_fwd_info.handle = forward_handle;
        Ok(Self {
            server: self.server.clone(),
            info: object_info,
            new_server_fn: self.new_server_fn,
            new_mitm_server_fn: self.new_mitm_server_fn,
            handle_type: WaitHandleType::Session,
            mitm_forward_info: mitm_fwd_info,
            is_mitm_service: forward_handle != svc::INVALID_HANDLE,
            service_name: sm::ServiceName::empty(),
            domain_table: self.domain_table.clone(),
        })
    }

    pub fn get_new_server_fn(&self) -> Result<NewServerFn> {
        match self.new_server_fn {
            Some(new_server_fn) => Ok(new_server_fn),
            None => sf::hipc::rc::ResultSessionClosed::make_err(),
        }
    }

    pub fn get_new_mitm_server_fn(&self) -> Result<NewMitmServerFn> {
        match self.new_mitm_server_fn {
            Some(new_mitm_server_fn) => Ok(new_mitm_server_fn),
            None => sf::hipc::rc::ResultSessionClosed::make_err(),
        }
    }

    pub fn convert_to_domain(&mut self) -> Result<cmif::DomainObjectId> {
        // Check that we're not already a domain
        result_return_if!(self.info.is_domain(), rc::ResultAlreadyDomain);

        // Since we're a base domain object now, create a domain table
        let dom_table = Rc::new(Mutex::new(DomainTable::new()));
        self.domain_table = Some(dom_table.clone());

        let domain_object_id = match self.is_mitm_service {
            true => {
                let forward_object_id =
                    self.mitm_forward_info.convert_current_object_to_domain()?;
                self.mitm_forward_info.domain_object_id = forward_object_id;
                dom_table.lock().allocate_specific_id(forward_object_id)?
            }
            false => dom_table.lock().allocate_id()?,
        };

        self.info.domain_object_id = domain_object_id;
        Ok(domain_object_id)
    }

    pub fn close(&mut self) -> Result<()> {
        if !self.service_name.is_empty() {
            #[cfg(feature = "services")]
            {
                if self.handle_type == WaitHandleType::Server {
                    let sm = service::new_named_port_object::<sm::UserInterface>()?;
                    if self.is_mitm_service {
                        debug_assert!(
                            self.info.owns_handle,
                            "MitM server objects should always own their handles."
                        );
                        sm.atmosphere_uninstall_mitm(self.service_name)?;
                    } else {
                        sm.unregister_service(self.service_name)?;
                    }
                    sm.detach_client(sf::ProcessId::new())?;
                }
            }
        }

        // Forward info may be valid despite having an empty service name (like a cloned session)
        if self.is_mitm_service {
            sf::Session::from(self.mitm_forward_info).close();
        }

        // Don't close our session like a normal one (like the forward session below) as we allocated the object IDs ourselves, the only thing we do have to close is the handle
        if self.info.owns_handle {
            svc::close_handle(self.info.handle)?;
        }
        Ok(())
    }
}

impl Drop for ServerHolder {
    fn drop(&mut self) {
        self.close().unwrap();
    }
}

pub struct HipcManager<'a> {
    server_holder: &'a mut ServerHolder,
    pointer_buf_size: usize,
    pub cloned_object_server_handle: svc::Handle,
    pub cloned_object_forward_handle: svc::Handle,
}

impl<'a> HipcManager<'a> {
    pub fn new(server_holder: &'a mut ServerHolder, pointer_buf_size: usize) -> Self {
        Self {
            server_holder,
            pointer_buf_size,
            cloned_object_server_handle: svc::INVALID_HANDLE,
            cloned_object_forward_handle: svc::INVALID_HANDLE,
        }
    }

    pub fn has_cloned_object(&self) -> bool {
        self.cloned_object_server_handle != 0
    }

    pub fn clone_object(&self) -> Result<ServerHolder> {
        self.server_holder.clone_self(
            self.cloned_object_server_handle,
            self.cloned_object_forward_handle,
        )
    }
}

impl IHipcManager for HipcManager<'_> {
    fn convert_current_object_to_domain(&mut self) -> Result<cmif::DomainObjectId> {
        self.server_holder.convert_to_domain()
    }

    fn copy_from_current_domain(
        &mut self,
        _domain_object_id: cmif::DomainObjectId,
    ) -> Result<sf::MoveHandle> {
        // TODO
        crate::rc::ResultNotImplemented::make_err()
    }

    fn clone_current_object(&mut self) -> Result<sf::MoveHandle> {
        let (server_handle, client_handle) = svc::create_session(false, 0)?;

        let mut forward_handle: svc::Handle = 0;
        if self.server_holder.is_mitm_service {
            let fwd_handle = self
                .server_holder
                .mitm_forward_info
                .clone_current_object()?;
            forward_handle = fwd_handle.handle;
        }

        self.cloned_object_server_handle = server_handle;
        self.cloned_object_forward_handle = forward_handle;
        Ok(sf::Handle::from(client_handle))
    }

    fn query_pointer_buffer_size(&mut self) -> Result<u16> {
        Ok(self.pointer_buf_size as u16)
    }

    fn clone_current_object_ex(&mut self, _tag: u32) -> Result<sf::MoveHandle> {
        // The tag value is unused anyways :P
        self.clone_current_object()
    }
}

impl ISessionObject for HipcManager<'_> {
    fn try_handle_request_by_id(
        &mut self,
        req_id: u32,
        protocol: CommandProtocol,
        server_ctx: &mut ServerContext,
    ) -> Option<Result<()>> {
        <Self as IHipcManager>::try_handle_request_by_id(self, req_id, protocol, server_ctx)
    }
}

pub struct MitmQueryService<S: IMitmService> {
    _phantom: core::marker::PhantomData<S>,
}

/// This is safe as we're only calling associated functions and not trait methods.
unsafe impl<S: IMitmService> Sync for MitmQueryService<S> {}

impl<S: IMitmService> MitmQueryService<S> {
    pub fn new() -> Self {
        Self {
            _phantom: core::marker::PhantomData,
        }
    }
}

impl<S: IMitmService> Default for MitmQueryService<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: IMitmService> IMitmQueryServiceServer for MitmQueryService<S> {
    fn should_mitm(&mut self, info: sm::mitm::MitmProcessInfo) -> Result<bool> {
        Ok(S::should_mitm(info))
    }
}

impl<S: IMitmService> ISessionObject for MitmQueryService<S> {
    fn try_handle_request_by_id(
        &mut self,
        req_id: u32,
        protocol: CommandProtocol,
        server_ctx: &mut ServerContext,
    ) -> Option<Result<()>> {
        <Self as IMitmQueryServiceServer>::try_handle_request_by_id(
            self, req_id, protocol, server_ctx,
        )
    }
}

pub trait INamedPort: IServerObject {
    fn get_port_name() -> &'static core::ffi::CStr;
    fn get_max_sesssions() -> i32;
}

pub trait IService: IServerObject {
    fn get_name() -> sm::ServiceName;
    fn get_max_sesssions() -> i32;
}

pub trait IMitmService: IMitmServerObject {
    fn get_name() -> sm::ServiceName;
    fn should_mitm(info: sm::mitm::MitmProcessInfo) -> bool;
}

// TODO: use const generics to reduce memory usage, like libstratosphere does?

pub struct ServerManager<const P: usize> {
    server_holders: Vec<ServerHolder>,
    wait_handles: [svc::Handle; MAX_COUNT],
    pointer_buffer: [u8; P],
}

impl<const P: usize> ServerManager<P> {
    pub fn new() -> Result<Self> {
        Ok(Self {
            server_holders: Vec::new(),
            wait_handles: [0; MAX_COUNT],
            pointer_buffer: [0; P],
        })
    }

    #[inline(always)]
    fn prepare_wait_handles(&mut self) -> &[svc::Handle] {
        let mut handles_index: usize = 0;
        for server_holder in &mut self.server_holders {
            let server_info = server_holder.info;
            if server_info.handle != svc::INVALID_HANDLE {
                self.wait_handles[handles_index] = server_info.handle;
                handles_index += 1;
            }
        }
        &self.wait_handles[..handles_index]
    }

    #[inline(always)]
    fn handle_request_command(
        &mut self,
        ctx: &mut CommandContext,
        rq_id: u32,
        command_type: cmif::CommandType,
        domain_command_type: cmif::DomainCommandType,
        ipc_buf_backup: &[u8],
        domain_table: Option<Rc<Mutex<DomainTable>>>,
    ) -> Result<()> {
        let is_domain = ctx.object_info.is_domain();
        let domain_table_clone = domain_table.clone();
        let do_handle_request = || -> Result<()> {
            let mut new_sessions: Vec<ServerHolder> = Vec::new();
            for server_holder in &mut self.server_holders {
                let server_info = server_holder.info;
                if server_info.handle == ctx.object_info.handle {
                    let send_to_forward_handle = || -> Result<()> {
                        let ipc_buf = get_msg_buffer();
                        unsafe {
                            core::ptr::copy(ipc_buf_backup.as_ptr(), ipc_buf, ipc_buf_backup.len());
                        }
                        // Let the original service take care of the command for us.
                        svc::send_sync_request(server_holder.mitm_forward_info.handle)
                    };

                    let target_server = match is_domain {
                        true => match ctx.object_info.owns_handle {
                            true => server_holder
                                .server
                                .clone()
                                .ok_or(rc::ResultSignaledServerNotFound::make())?,
                            false => domain_table
                                .ok_or(rc::ResultDomainNotFound::make())?
                                .lock()
                                .find_domain(ctx.object_info.domain_object_id)?,
                        },
                        false => server_holder
                            .server
                            .clone()
                            .ok_or(rc::ResultSignaledServerNotFound::make())?,
                    };
                    // Nothing done on success here, as if the command succeeds it will automatically respond by itself.
                    let mut command_found = false;
                    {
                        let protocol = ctx.object_info.protocol;
                        let mut server_ctx = ServerContext::new(
                            ctx,
                            DataWalker::empty(),
                            domain_table_clone.clone(),
                            &mut new_sessions,
                        );
                        if let Some(result) = target_server.lock().try_handle_request_by_id(
                            rq_id,
                            protocol,
                            &mut server_ctx,
                        ) {
                            command_found = true;
                            if let Err(rc) = result {
                                if server_holder.is_mitm_service
                                    && sm::mitm::rc::ResultShouldForwardToSession::matches(rc)
                                {
                                    if let Err(rc) = send_to_forward_handle() {
                                        cmif::server::write_request_command_response_on_msg_buffer(
                                            ctx,
                                            rc,
                                            command_type,
                                        );
                                    }
                                } else {
                                    cmif::server::write_request_command_response_on_msg_buffer(
                                        ctx,
                                        rc,
                                        command_type,
                                    );
                                }
                            }
                        }
                    }
                    if !command_found {
                        if server_holder.is_mitm_service {
                            if let Err(rc) = send_to_forward_handle() {
                                cmif::server::write_request_command_response_on_msg_buffer(
                                    ctx,
                                    rc,
                                    command_type,
                                );
                            }
                        } else {
                            cmif::server::write_request_command_response_on_msg_buffer(
                                ctx,
                                cmif::rc::ResultInvalidCommandRequestId::make(),
                                command_type,
                            );
                        }
                    }
                    break;
                }
            }

            self.server_holders.append(&mut new_sessions);

            Ok(())
        };

        match domain_command_type {
            cmif::DomainCommandType::Invalid => {
                // Invalid command type might mean that the session isn't a domain :P
                match is_domain {
                    false => do_handle_request()?,
                    true => return rc::ResultInvalidDomainCommandType::make_err(),
                };
            }
            cmif::DomainCommandType::SendMessage => do_handle_request()?,
            cmif::DomainCommandType::Close => {
                if !ctx.object_info.owns_handle {
                    domain_table_clone
                        .ok_or(rc::ResultDomainNotFound::make())?
                        .lock()
                        .deallocate_domain(ctx.object_info.domain_object_id);
                } else {
                    // TODO: Abort? Error?
                }
            }
        }

        Ok(())
    }

    #[inline(always)]
    fn handle_control_command(
        &mut self,
        ctx: &mut CommandContext,
        rq_id: u32,
        command_type: cmif::CommandType,
    ) -> Result<()> {
        // Control commands only exist in CMIF...
        result_return_unless!(
            ctx.object_info.uses_cmif_protocol(),
            super::rc::ResultInvalidProtocol
        );

        for server_holder in &mut self.server_holders {
            let server_info = server_holder.info;
            if server_info.handle == ctx.object_info.handle {
                let mut hipc_manager = HipcManager::new(server_holder, P);
                // Nothing done on success here, as if the command succeeds it will automatically respond by itself.
                let mut command_found = false;
                {
                    let mut unused_new_sessions: Vec<ServerHolder> = Vec::new();
                    let mut server_ctx = ServerContext::new(
                        ctx,
                        DataWalker::empty(),
                        None,
                        &mut unused_new_sessions,
                    );
                    if let Some(result) = <HipcManager as ISessionObject>::try_handle_request_by_id(
                        &mut hipc_manager,
                        rq_id,
                        CommandProtocol::Cmif,
                        &mut server_ctx,
                    ) {
                        command_found = true;
                        if let Err(rc) = result {
                            cmif::server::write_control_command_response_on_msg_buffer(
                                ctx,
                                rc,
                                command_type,
                            );
                        }
                    }
                }
                if !command_found {
                    cmif::server::write_control_command_response_on_msg_buffer(
                        ctx,
                        cmif::rc::ResultInvalidCommandRequestId::make(),
                        command_type,
                    );
                }

                if hipc_manager.has_cloned_object() {
                    let cloned_holder = hipc_manager.clone_object()?;
                    self.server_holders.push(cloned_holder);
                }
                break;
            }
        }

        Ok(())
    }

    fn process_signaled_handle(&mut self, handle: svc::Handle) -> Result<()> {
        let mut server_found = false;
        let mut index: usize = 0;
        let mut should_close_session = false;
        let mut new_sessions: Vec<ServerHolder> = Vec::new();

        let mut ctx = CommandContext::empty();
        let mut command_type = cmif::CommandType::Invalid;
        let mut domain_cmd_type = cmif::DomainCommandType::Invalid;
        let mut rq_id: u32 = 0;
        let mut ipc_buf_backup: [u8; 0x100] = [0; 0x100];
        let mut domain_table: Option<Rc<Mutex<DomainTable>>> = None;

        for server_holder in &mut self.server_holders {
            let server_info = server_holder.info;
            if server_info.handle == handle {
                server_found = true;
                match server_holder.handle_type {
                    WaitHandleType::Session => {
                        if P > 0 {
                            // Send our pointer buffer as a C descriptor for kernel - why are Pointer buffers so fucking weird?
                            let mut tmp_ctx = CommandContext::new_client(server_info);
                            tmp_ctx.add_receive_static(ReceiveStaticDescriptor::new(
                                self.pointer_buffer.as_ptr(),
                                P,
                            ))?;
                            cmif::client::write_command_on_msg_buffer(
                                &mut tmp_ctx,
                                cmif::CommandType::Invalid,
                                0,
                            );
                        }

                        if let Err(rc) = unsafe { svc::reply_and_receive(&handle, 1, 0, -1) } {
                            if svc::rc::ResultSessionClosed::matches(rc) {
                                should_close_session = true;
                                break;
                            } else {
                                return Err(rc);
                            }
                        };

                        unsafe {
                            core::ptr::copy(
                                get_msg_buffer(),
                                ipc_buf_backup.as_mut_ptr(),
                                ipc_buf_backup.len(),
                            )
                        };

                        ctx = CommandContext::new_server(
                            server_info,
                            self.pointer_buffer.as_mut_ptr(),
                        );
                        command_type = cmif::server::read_command_from_msg_buffer(&mut ctx);
                        match command_type {
                            cmif::CommandType::Request | cmif::CommandType::RequestWithContext => {
                                match cmif::server::read_request_command_from_msg_buffer(&mut ctx) {
                                    Ok((request_id, domain_command_type, domain_object_id)) => {
                                        let mut base_info = server_info;
                                        if server_info.is_domain() {
                                            // This is a domain request
                                            base_info.domain_object_id = domain_object_id;
                                            base_info.owns_handle =
                                                server_info.domain_object_id == domain_object_id;
                                        }
                                        ctx.object_info = base_info;
                                        domain_cmd_type = domain_command_type;
                                        rq_id = request_id;
                                        domain_table = server_holder.domain_table.clone();
                                    }
                                    Err(rc) => return Err(rc),
                                };
                            }
                            cmif::CommandType::Control | cmif::CommandType::ControlWithContext => {
                                match cmif::server::read_control_command_from_msg_buffer(&mut ctx) {
                                    Ok(control_rq_id) => {
                                        rq_id = control_rq_id as u32;
                                    }
                                    Err(rc) => return Err(rc),
                                };
                            }
                            cmif::CommandType::Close => {
                                should_close_session = true;
                            }
                            _ => return rc::ResultInvalidCommandType::make_err(),
                        }
                    }
                    WaitHandleType::Server => {
                        let new_handle = svc::accept_session(handle)?;

                        if server_holder.is_mitm_service {
                            #[cfg(feature = "services")]
                            {
                                let sm = service::new_named_port_object::<sm::UserInterface>()?;
                                let (info, session_handle) = sm
                                    .atmosphere_acknowledge_mitm_session(
                                        server_holder.service_name,
                                    )?;
                                new_sessions.push(server_holder.make_new_mitm_session(
                                    new_handle,
                                    session_handle.handle,
                                    info,
                                    server_holder.service_name,
                                )?);
                                sm.detach_client(sf::ProcessId::new())?;
                            }
                        } else {
                            new_sessions.push(server_holder.make_new_session(new_handle)?);
                        }
                    }
                };
                break;
            }
            index += 1;
        }

        let reply_impl = || -> Result<()> {
            match unsafe { svc::reply_and_receive(&handle, 0, handle, 0) } {
                Err(rc) => {
                    if svc::rc::ResultTimedOut::matches(rc)
                        || svc::rc::ResultSessionClosed::matches(rc)
                    {
                        Ok(())
                    } else {
                        Err(rc)
                    }
                }
                _ => Ok(()),
            }
        };

        match command_type {
            cmif::CommandType::Request | cmif::CommandType::RequestWithContext => {
                self.handle_request_command(
                    &mut ctx,
                    rq_id,
                    command_type,
                    domain_cmd_type,
                    &ipc_buf_backup,
                    domain_table,
                )?;
                reply_impl()?;
            }
            cmif::CommandType::Control | cmif::CommandType::ControlWithContext => {
                self.handle_control_command(&mut ctx, rq_id, command_type)?;
                reply_impl()?;
            }
            cmif::CommandType::Close => {
                cmif::server::write_close_command_response_on_msg_buffer(&mut ctx);
                reply_impl()?;
            }
            _ => {
                // Do nothing, since it might not be set at all without having failed (for instance, if a new session was accepted)
                // If this actually failed and it reached this point server_found will be false, which is handled below
            }
        };

        if should_close_session {
            self.server_holders.remove(index);
        }

        self.server_holders.append(&mut new_sessions);

        match server_found {
            true => Ok(()),
            false => rc::ResultSignaledServerNotFound::make_err(),
        }
    }

    pub fn register_server<S: IServerObject + 'static>(
        &mut self,
        handle: svc::Handle,
        service_name: sm::ServiceName,
    ) {
        self.server_holders
            .push(ServerHolder::new_server::<S>(handle, service_name));
    }

    pub fn register_mitm_server<S: IMitmServerObject + 'static>(
        &mut self,
        handle: svc::Handle,
        service_name: sm::ServiceName,
    ) {
        self.server_holders
            .push(ServerHolder::new_mitm_server::<S>(handle, service_name));
    }

    pub fn register_session<S: ISessionObject + 'static>(
        &mut self,
        handle: svc::Handle,
        session_obj: Rc<Mutex<S>>,
    ) {
        self.server_holders
            .push(ServerHolder::new_session(handle, session_obj));
    }

    #[cfg(feature = "services")]
    pub fn register_service_server<S: IService + 'static>(&mut self) -> Result<()> {
        let service_name = S::get_name();

        let sm = service::new_named_port_object::<sm::UserInterface>()?;
        let service_handle = sm.register_service(service_name, false, S::get_max_sesssions())?;
        self.register_server::<S>(service_handle.handle, service_name);
        sm.detach_client(sf::ProcessId::new())?;
        Ok(())
    }

    #[cfg(feature = "services")]
    pub fn register_mitm_service_server<S: IMitmService + 'static>(&mut self) -> Result<()> {
        let service_name = S::get_name();

        let sm = service::new_named_port_object::<sm::UserInterface>()?;
        let (mitm_handle, query_handle) = sm.atmosphere_install_mitm(service_name)?;

        self.register_mitm_server::<S>(mitm_handle.handle, service_name);

        let mitm_query_srv: Rc<Mutex<MitmQueryService<S>>> =
            Rc::new(Mutex::new(MitmQueryService::<S>::new()));
        self.register_session(query_handle.handle, mitm_query_srv);

        sm.atmosphere_clear_future_mitm(service_name)?;
        sm.detach_client(sf::ProcessId::new())?;
        Ok(())
    }

    pub fn register_named_port_server<S: INamedPort + 'static>(&mut self) -> Result<()> {
        let port_handle =
            unsafe { svc::manage_named_port(S::get_port_name(), S::get_max_sesssions())? };

        self.register_server::<S>(port_handle, sm::ServiceName::empty());
        Ok(())
    }

    pub fn process(&mut self) -> Result<()> {
        let handles = self.prepare_wait_handles();
        let index = wait::wait_handles(handles, 100_000)?;

        let signaled_handle = self.wait_handles[index];
        self.process_signaled_handle(signaled_handle)?;

        Ok(())
    }

    pub fn loop_process(&mut self) -> Result<()> {
        loop {
            if let Err(rc) = self.process() {
                if svc::rc::ResultTimedOut::matches(rc) {
                    continue;
                }
                // TODO: handle results properly here
                if svc::rc::ResultCancelled::matches(rc) {
                    break;
                }
                return Err(rc);
            }
        }

        Ok(())
    }
}
