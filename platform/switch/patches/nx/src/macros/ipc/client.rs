#![macro_use]

/// Simplifies the creation of a (client-side IPC) type implementing an IPC interface (and implementing the trait)
///
/// # Examples
///
/// ```
///
/// // This already creates the client-IPC type and implements IObject and the SF trait below
/// ipc_sf_define_default_client_for_interface!(ExampleInterface);
///
/// ipc_sf_define_interface_trait! {
///     trait ExampleInterface {
///         command_1 [1, VersionInterval::all()]: (in_32: u32) => (out_16: u16);
///         #[ipc_rid(20)] #[version(VersionInterval::all())] fn command_2(&self, in_8: u8);
///     }
/// }
/// ```
#[macro_export]
macro_rules! ipc_sf_define_default_client_for_interface {
    ($t:ident) => {
        paste::paste! {
            #[doc = concat!("The default client for the `", stringify!($t), "` trait. All implementors of the trait need to read their session in accordance with this Types IPC Parameter traits.")]
            pub struct $t {
                #[doc(hidden)]
                pub (crate) session: $crate::ipc::sf::Session
            }

            impl $crate::ipc::client::IClientObject for $t {
                fn new(session: $crate::ipc::sf::Session) -> Self {
                    Self { session }
                }

                fn get_session(&self) -> & $crate::ipc::sf::Session {
                    &self.session
                }

                fn get_session_mut(&mut self) -> &mut $crate::ipc::sf::Session {
                    &mut self.session
                }
            }

            unsafe impl ::core::marker::Sync for $t {}
            unsafe impl ::core::marker::Send for $t {}

            impl $crate::ipc::client::RequestCommandParameter for $t {
                fn before_request_write(session: &Self, _walker: &mut $crate::ipc::DataWalker, ctx: &mut $crate::ipc::CommandContext) -> $crate::result::Result<()> {
                    ctx.in_params.add_object(session.session.object_info)
                }

                fn before_send_sync_request(_session: &Self, _walker: &mut $crate::ipc::DataWalker, _ctx: &mut $crate::ipc::CommandContext) -> $crate::result::Result<()> {
                    Ok(())
                }
            }

            impl $crate::ipc::client::ResponseCommandParameter<$t> for $t {
                fn after_response_read(_walker: &mut $crate::ipc::DataWalker, ctx: &mut $crate::ipc::CommandContext) -> $crate::result::Result<Self> {
                    let object_info = ctx.pop_object()?;
                    Ok(Self { session: $crate::ipc::sf::Session::from(object_info)})
                }
            }
            impl $crate::ipc::server::RequestCommandParameter<'_, $t> for $t {
                fn after_request_read(_ctx: &mut $crate::ipc::server::ServerContext) -> $crate::result::Result<Self> {
                    use $crate::result::ResultBase;
                    // TODO: determine if we need to do this, since this is a server side operation of a client object?
                    // probably needs to be supported right?
                    $crate::ipc::sf::hipc::rc::ResultUnsupportedOperation::make_err()
                }
            }

            impl $crate::ipc::server::ResponseCommandParameter for $t {
                type CarryState = ();
                fn before_response_write(_session: &Self, _ctx: &mut $crate::ipc::server::ServerContext) -> $crate::result::Result<()> {
                    use $crate::result::ResultBase;
                    // TODO: determine if we need to do this, since this is a server side operation of a client object?
                    // probably needs to be supported right?
                    $crate::ipc::sf::hipc::rc::ResultUnsupportedOperation::make_err()
                }

                fn after_response_write(_session: Self, _carry_state: (), _ctx: &mut $crate::ipc::server::ServerContext) -> $crate::result::Result<()> {
                    use $crate::result::ResultBase;
                    // TODO: determine if we need to do this, since this is a server side operation of a client object?
                    // probably needs to be supported right?
                    $crate::ipc::sf::hipc::rc::ResultUnsupportedOperation::make_err()
                }
            }

            impl [<I $t Client>] for $t {}
        }
    }
}

/// Simplifies the creation of a (client-side IPC) type implementing an IPC interface, without implementing the trait or IPC serialization
///
/// # Examples
///
/// ```
/// // Let's suppose a "IExampleInterface" IPC interface trait exists
///
/// // This already creates the client-IPC type and impls IObject
/// ipc_client_define_client_default!(ExampleInterface);
///
/// impl IExampleInterface for ExampleInterface {
///     (...)
/// }
/// ```
#[macro_export]
macro_rules! ipc_client_define_client_default {
    ($t:ident) => {
        #[doc = concat!("Default client object for the `", stringify!($t), "` service")]
        #[allow(missing_docs)]
        pub struct $t {
            pub(crate) session: $crate::ipc::sf::Session,
        }

        impl $crate::ipc::client::IClientObject for $t {
            fn new(session: $crate::ipc::sf::Session) -> Self {
                Self { session }
            }

            fn get_session(&self) -> &$crate::ipc::sf::Session {
                &self.session
            }
            fn get_session_mut(&mut self) -> &mut $crate::ipc::sf::Session {
                &mut self.session
            }
        }

        unsafe impl Sync for $t {}
        unsafe impl Send for $t {}
    };
}

/// Sends an IPC "Request" command
///
/// # Examples
///
/// ```
/// use nx::ipc::sf::Session;
///
/// fn demo(session: Session) -> $crate::result::Result<()> {
///     let in_32: u32 = 69;
///     let in_16: u16 = 420;
///
///     // Calls command with request ID 123 and with an input-u32 and an input-u16 expecting an output-u64, Will yield a Result<u64>
///     let _out = ipc_client_send_request_command!([session.object_info; 123] (in_32, in_16) => (out: u64))?;
///
///     Ok(())
/// }
/// ```
#[macro_export]
macro_rules! ipc_client_send_request_command {
    ([$obj_info:expr; $rq_id:expr] ( $( $in_param:expr ),* ) => ( $( $out_param:ident: $out_param_type:ty ),* )) => {{
        let mut ctx = $crate::ipc::CommandContext::new_client($obj_info);

        let mut walker = $crate::ipc::DataWalker::new(core::ptr::null_mut());
        $( $crate::ipc::client::RequestCommandParameter::before_request_write(&$in_param, &mut walker, &mut ctx)?; )*
        ctx.in_params.data_size = walker.get_offset() as u32;

        match $obj_info.protocol {
            $crate::ipc::CommandProtocol::Cmif => $crate::ipc::cmif::client::write_request_command_on_msg_buffer(&mut ctx, Some($rq_id), $crate::ipc::cmif::DomainCommandType::SendMessage),
            $crate::ipc::CommandProtocol::Tipc => $crate::ipc::tipc::client::write_request_command_on_msg_buffer(&mut ctx, $rq_id)
        };

        walker.reset_with(ctx.in_params.data_offset);
        $( $crate::ipc::client::RequestCommandParameter::before_send_sync_request(&$in_param, &mut walker, &mut ctx)?; )*

        $crate::svc::send_sync_request($obj_info.handle)?;

        match $obj_info.protocol {
            $crate::ipc::CommandProtocol::Cmif => $crate::ipc::cmif::client::read_request_command_response_from_msg_buffer(&mut ctx)?,
            $crate::ipc::CommandProtocol::Tipc => $crate::ipc::tipc::client::read_request_command_response_from_msg_buffer(&mut ctx)?
        };

        walker.reset_with(ctx.out_params.data_offset);
        $( let $out_param = <$out_param_type as $crate::ipc::client::ResponseCommandParameter<_>>::after_response_read(&mut walker, &mut ctx)?; )*

        Ok(( $( $out_param as _ ),* ))
    }};
}

/// Identical to [`ipc_client_send_request_command`] but for a "Control" command
///
/// See <https://switchbrew.org/wiki/IPC_Marshalling#Control>
#[macro_export]
macro_rules! ipc_client_send_control_command {
    ([$obj_info:expr; $rq_id:expr] ( $( $in_param:expr ),* ) => ( $( $out_param:ident: $out_param_type:ty ),* )) => {{
        $crate::result_return_if!($obj_info.uses_tipc_protocol(), $crate::ipc::rc::ResultInvalidProtocol);

        let mut ctx = $crate::ipc::CommandContext::new_client($obj_info);

        let mut walker = $crate::ipc::DataWalker::new(core::ptr::null_mut());
        $( $crate::ipc::client::RequestCommandParameter::before_request_write(&$in_param, &mut walker, &mut ctx)?; )*
        ctx.in_params.data_size = walker.get_offset() as u32;

        $crate::ipc::cmif::client::write_control_command_on_msg_buffer(&mut ctx, $rq_id);

        walker.reset_with(ctx.in_params.data_offset);
        $( $crate::ipc::client::RequestCommandParameter::before_send_sync_request(&$in_param, &mut walker, &mut ctx)?; )*

        $crate::svc::send_sync_request($obj_info.handle)?;

        $crate::ipc::cmif::client::read_control_command_response_from_msg_buffer(&mut ctx)?;

        walker.reset_with(ctx.out_params.data_offset);
        $( let $out_param = <$out_param_type as $crate::ipc::client::ResponseCommandParameter<_>>::after_response_read(&mut walker, &mut ctx)?; )*

        Ok(( $( $out_param as _ ),* ))
    }};
}

#[macro_export]
macro_rules! client_mark_request_command_parameters_types_as_copy {
    ($($t:ty),*) => {
        $(
        //const_assert!($t::is_pod());
        impl $crate::ipc::client::RequestCommandParameter for $t {
            fn before_request_write(_raw: &Self, walker: &mut $crate::ipc::DataWalker, _ctx: &mut $crate::ipc::CommandContext) -> $crate::result::Result<()> {
                walker.advance::<Self>();
                Ok(())
            }

            fn before_send_sync_request(raw: &Self, walker: &mut $crate::ipc::DataWalker, _ctx: &mut $crate::ipc::CommandContext) -> $crate::result::Result<()> {
                walker.advance_set(*raw);
                Ok(())
            }
        }


        impl $crate::ipc::client::ResponseCommandParameter<$t> for $t {
            fn after_response_read(walker: &mut $crate::ipc::DataWalker, _ctx: &mut $crate::ipc::CommandContext) -> $crate::result::Result<Self> {
                Ok(walker.advance_get())
            }
        })*
    };
}

impl<T: Copy, const N: usize> crate::ipc::client::RequestCommandParameter for [T; N] {
    fn before_request_write(
        _raw: &Self,
        walker: &mut crate::ipc::DataWalker,
        _ctx: &mut crate::ipc::CommandContext,
    ) -> crate::result::Result<()> {
        walker.advance::<Self>();
        Ok(())
    }

    fn before_send_sync_request(
        raw: &Self,
        walker: &mut crate::ipc::DataWalker,
        _ctx: &mut crate::ipc::CommandContext,
    ) -> crate::result::Result<()> {
        walker.advance_set(*raw);
        Ok(())
    }
}

impl<T: Copy, const N: usize> crate::ipc::client::ResponseCommandParameter<[T; N]> for [T; N] {
    fn after_response_read(
        walker: &mut crate::ipc::DataWalker,
        _ctx: &mut crate::ipc::CommandContext,
    ) -> crate::result::Result<Self> {
        Ok(walker.advance_get())
    }
}
