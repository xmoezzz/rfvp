use sf::hipc;

use super::*;

pub trait RequestCommandParameter {
    fn before_request_write(
        var: &Self,
        walker: &mut DataWalker,
        ctx: &mut CommandContext,
    ) -> Result<()>;
    fn before_send_sync_request(
        var: &Self,
        walker: &mut DataWalker,
        ctx: &mut CommandContext,
    ) -> Result<()>;
}

pub trait ResponseCommandParameter<O> {
    fn after_response_read(walker: &mut DataWalker, ctx: &mut CommandContext) -> Result<O>;
}

impl<
    const IN: bool,
    const OUT: bool,
    const MAP_ALIAS: bool,
    const POINTER: bool,
    const FIXED_SIZE: bool,
    const AUTO_SELECT: bool,
    const ALLOW_NON_SECURE: bool,
    const ALLOW_NON_DEVICE: bool,
    T,
> RequestCommandParameter
    for sf::Buffer<
        '_,
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
    fn before_request_write(
        buffer: &Self,
        _walker: &mut DataWalker,
        ctx: &mut CommandContext,
    ) -> Result<()> {
        ctx.add_buffer(buffer)
    }

    fn before_send_sync_request(
        _buffer: &Self,
        _walker: &mut DataWalker,
        _ctx: &mut CommandContext,
    ) -> Result<()> {
        Ok(())
    }
}

//impl<const A: BufferAttribute, T> !ResponseCommandParameter<sf::Buffer<A, T>> for sf::Buffer<A, T> {}

impl<const MOVE: bool> RequestCommandParameter for sf::Handle<MOVE> {
    fn before_request_write(
        handle: &Self,
        _walker: &mut DataWalker,
        ctx: &mut CommandContext,
    ) -> Result<()> {
        ctx.in_params.add_handle(handle.clone())
    }

    fn before_send_sync_request(
        _handle: &Self,
        _walker: &mut DataWalker,
        _ctx: &mut CommandContext,
    ) -> Result<()> {
        Ok(())
    }
}

impl<const MOVE: bool> ResponseCommandParameter<sf::Handle<MOVE>> for sf::Handle<MOVE> {
    fn after_response_read(_walker: &mut DataWalker, ctx: &mut CommandContext) -> Result<Self> {
        ctx.out_params.pop_handle()
    }
}

impl RequestCommandParameter for sf::ProcessId {
    fn before_request_write(
        _process_id: &Self,
        walker: &mut DataWalker,
        ctx: &mut CommandContext,
    ) -> Result<()> {
        // signal to the kernel that we need a PID injected into the request
        ctx.in_params.send_process_id = true;
        if ctx.object_info.uses_cmif_protocol() {
            // TIPC doesn't set this placeholder space for process IDs
            walker.advance::<u64>();
        }
        Ok(())
    }

    fn before_send_sync_request(
        process_id: &Self,
        walker: &mut DataWalker,
        ctx: &mut CommandContext,
    ) -> Result<()> {
        // Same as above
        if ctx.object_info.uses_cmif_protocol() {
            walker.advance_set(process_id.process_id);
        }
        Ok(())
    }
}

//impl !ResponseCommandParameter<sf::ProcessId> for sf::ProcessId {}

impl RequestCommandParameter for sf::AppletResourceUserId {
    fn before_request_write(
        _aruid: &Self,
        walker: &mut DataWalker,
        ctx: &mut CommandContext,
    ) -> Result<()> {
        result_return_unless!(
            ctx.object_info.uses_cmif_protocol(),
            hipc::rc::ResultUnsupportedOperation
        );

        // signal to the kernel that we need a PID injected into the request
        ctx.in_params.send_process_id = true;
        walker.advance::<u64>();
        Ok(())
    }

    fn before_send_sync_request(
        aruid: &Self,
        walker: &mut DataWalker,
        ctx: &mut CommandContext,
    ) -> Result<()> {
        result_return_unless!(
            ctx.object_info.uses_cmif_protocol(),
            hipc::rc::ResultUnsupportedOperation
        );
        // write the aruid into the slot
        walker.advance_set(aruid.aruid);
        Ok(())
    }
}

//impl !ResponseCommandParameter<sf::AppletResourceUserId> for sf::AppletResourceUserId {}

pub trait IClientObject {
    fn new(session: sf::Session) -> Self
    where
        Self: Sized;

    fn get_session(&self) -> &sf::Session;

    fn get_session_mut(&mut self) -> &mut sf::Session;

    fn get_info(&self) -> ObjectInfo {
        self.get_session().object_info
    }

    /// # Safety
    ///
    /// The `ObjectInfo` being passed in must be for the same implementation of `IClientObject`,
    /// or the object will cease to function after this call.
    unsafe fn set_info(&mut self, info: ObjectInfo) {
        self.get_session_mut().set_info(info);
    }

    fn convert_to_domain(&mut self) -> Result<()> {
        self.get_session_mut().convert_to_domain()
    }

    fn query_own_pointer_buffer_size(&self) -> Result<u16> {
        self.get_info().query_pointer_buffer_size()
    }

    fn close_session(&mut self) {
        self.get_session_mut().close()
    }

    fn is_valid(&self) -> bool {
        self.get_info().is_valid()
    }

    fn is_domain(&self) -> bool {
        self.get_info().is_domain()
    }

    fn clone(&self) -> Result<Self>
    where
        Self: Sized,
    {
        let handle = self.get_info().clone_current_object()?;
        Ok(Self::new(sf::Session {
            object_info: ObjectInfo {
                handle: handle.handle,
                ..self.get_info()
            },
        }))
    }
}
