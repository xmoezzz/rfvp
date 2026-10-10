#![macro_use]

/// Defines a trait meant to be used for IPC interfaces
///
/// # Examples
///
/// ```
/// use nx::version::{Version, VersionInterval};
///
/// // Define commands with their request ID, allowed version interval and in/out parameters
/// ipc_sf_define_interface_trait! {
///     trait ExampleInterface {
///         command_1 [1, VersionInterval::all()]: (in_32: u32) => (out_16: u16);
///         #[ipc_rid(20)] #[version(VersionInterval::all())] fn command_2(&self, in_8: u8);
///     }
/// }
///
/// // You can impl "IExampleInterface" to create a custom object implementing the commands
/// ```
#[macro_export]
macro_rules! ipc_sf_define_interface_trait {
    (
        trait $intf:ident {
            $(
                $name:ident [$rq_id:expr, $ver_intv:expr $(, $noalias:tt)?]: ( $( $in_param_name:ident: $in_param_type:ty ),* ) => ( $( $client_out_param_name:ident: $client_out_param_type:ty ),* ) ( $( $server_out_param_name:ident: $server_out_param_type:ty ),* )
            );* $(;)* // Note: trick to allow last trailing ';' for proper styling
        }
    ) => {
        paste::paste! {
            /// The IPC server trait for the [`<I $intf Server>`] trait. All methods are provided and overriding the implementations is not recommended.
            pub trait [<I $intf Client>]: $crate::ipc::client::IClientObject + Sync {
                $(
                    #[allow(unused_parens)]
                    #[allow(clippy::too_many_arguments)]
                    #[allow(missing_docs)]
                    fn $name(& $($noalias)? self, $( $in_param_name: $in_param_type ),* ) -> $crate::result::Result<( $( $client_out_param_type ),* )> {
                        let mut ctx = $crate::ipc::CommandContext::new_client(self.get_session().object_info);

                        let mut walker = $crate::ipc::DataWalker::new(core::ptr::null_mut());
                        $( $crate::ipc::client::RequestCommandParameter::before_request_write(&$in_param_name, &mut walker, &mut ctx)?; )*
                        ctx.in_params.data_size = walker.get_offset() as u32;

                        match self.get_session().object_info.protocol {
                            $crate::ipc::CommandProtocol::Cmif => $crate::ipc::cmif::client::write_request_command_on_msg_buffer(&mut ctx, Some($rq_id), $crate::ipc::cmif::DomainCommandType::SendMessage),
                            $crate::ipc::CommandProtocol::Tipc => $crate::ipc::tipc::client::write_request_command_on_msg_buffer(&mut ctx, $rq_id)
                        };

                        walker.reset_with(ctx.in_params.data_offset);
                        $( $crate::ipc::client::RequestCommandParameter::before_send_sync_request(&$in_param_name, &mut walker, &mut ctx)?; )*

                        $crate::svc::send_sync_request(self.get_session().object_info.handle)?;

                        match self.get_session().object_info.protocol {
                            $crate::ipc::CommandProtocol::Cmif => $crate::ipc::cmif::client::read_request_command_response_from_msg_buffer(&mut ctx)?,
                            $crate::ipc::CommandProtocol::Tipc => $crate::ipc::tipc::client::read_request_command_response_from_msg_buffer(&mut ctx)?
                        };

                        walker.reset_with(ctx.out_params.data_offset);
                        $( let $client_out_param_name = <$client_out_param_type as $crate::ipc::client::ResponseCommandParameter<_>>::after_response_read(&mut walker, &mut ctx)?; )*

                        Ok(( $( $client_out_param_name as _ ),* ))
                    }
                )*
            }

            /// The IPC server trait for the [`<I $intf Server>`] trait. The methods not provided must all be implemented, but overriding the IPC wrappers is not recommended.
            pub trait [<I $intf Server>]: $crate::ipc::server::ISessionObject + Sync {
                $(
                    #[allow(unused_parens)]
                    #[allow(clippy::too_many_arguments)]
                    #[allow(missing_docs)]
                    fn $name(&mut self, $( $in_param_name: $in_param_type ),* ) -> $crate::result::Result<( $( $server_out_param_type ),* )>;

                    #[allow(unused_assignments)]
                    #[allow(unused_parens)]
                    #[allow(unused_mut)]
                    #[doc(hidden)]
                    fn [<sf_server_impl_ $name>](&mut self, protocol: $crate::ipc::CommandProtocol, mut ctx: &mut $crate::ipc::server::ServerContext) -> $crate::result::Result<()> {
                        use $crate::result::ResultBase;

                        ctx.raw_data_walker = $crate::ipc::DataWalker::new(ctx.ctx.in_params.data_offset);
                        $( let $in_param_name = <$in_param_type as $crate::ipc::server::RequestCommandParameter<_>>::after_request_read(&mut ctx)?; )*

                        let ( $( $server_out_param_name ),* ) = self.$name( $( $in_param_name ),* )?;

                        ctx.raw_data_walker = $crate::ipc::DataWalker::new(core::ptr::null_mut());
                        $( let [<$server_out_param_name _carry_state>] = $crate::ipc::server::ResponseCommandParameter::before_response_write(&$server_out_param_name, &mut ctx)?; )*
                        ctx.ctx.out_params.data_size = ctx.raw_data_walker.get_offset() as u32;

                        match protocol {
                            $crate::ipc::CommandProtocol::Cmif => {
                                $crate::ipc::cmif::server::write_request_command_response_on_msg_buffer(&mut ctx.ctx, $crate::result::ResultSuccess::make(), $crate::ipc::cmif::CommandType::Request);
                            },
                            $crate::ipc::CommandProtocol::Tipc => {
                                $crate::ipc::tipc::server::write_request_command_response_on_msg_buffer(&mut ctx.ctx, $crate::result::ResultSuccess::make(), 16); // TODO: is this command type actually read/used/relevant?
                            }
                        };

                        ctx.raw_data_walker = $crate::ipc::DataWalker::new(ctx.ctx.out_params.data_offset);
                        $( $crate::ipc::server::ResponseCommandParameter::after_response_write($server_out_param_name, [<$server_out_param_name _carry_state>], &mut ctx)?; )*

                        Ok(())
                    }
                )*

                /// The dynamic dispatch function that calls into the IPC server functions. This should only be called from the [`$crate::ipc::server::ServerManager`] and not from client code.
                /// Examples for implementing [`ISessionObject`][`$crate::ipc::server::ISessionObject`] or [`IMitmServerOject`][`$crate::ipc::server::IMitmServerObject`] can be found in the [`nx`] crate.
                fn try_handle_request_by_id(&mut self, req_id: u32, protocol: $crate::ipc::CommandProtocol, ctx: &mut $crate::ipc::server::ServerContext) -> Option<$crate::result::Result<()>> {
                    let version = $crate::version::get_version();
                    match req_id {
                        $(
                            $rq_id if $ver_intv.contains(version) => {
                                Some(self.[<sf_server_impl_ $name>](protocol, ctx))
                            }
                        ),*
                        _ => None
                    }
                }
            }
        }
    };
}

#[macro_export]
macro_rules! session_type {
    ($t:ty) => {
        paste::paste! {
            impl [<I $t Server>] + 'static
        }
    };
}

/// Identical to [`ipc_sf_define_interface_trait`] but for "Control" IPC interfaces (inner trait functionality differs)
///
/// This shouldn't really be used unless you really know what you're doing
#[macro_export]
macro_rules! ipc_sf_define_control_interface_trait {
    (
        trait $intf:ident {
            $(
                $name:ident [$rq_id:expr, $ver_intv:expr]: ( $( $in_param_name:ident: $in_param_type:ty ),* ) => ( $( $out_param_name:ident: $out_param_type:ty ),* )
            );* $(;)* // Same as above
        }
    ) => {
        paste::paste! {
            pub trait $intf {
                $(
                    #[allow(unused_parens)]
                    fn $name(&mut self, $( $in_param_name: $in_param_type ),* ) -> $crate::result::Result<( $( $out_param_type ),* )>;

                    #[allow(unused_assignments)]
                    #[allow(unused_parens)]
                    #[doc(hidden)]
                    fn [<_sf_server_impl_ $name>](&mut self, _protocol: $crate::ipc::CommandProtocol, mut ctx: &mut $crate::ipc::server::ServerContext) -> $crate::result::Result<()> {
                        // TODO: tipc support, for now force cmif
                        $crate::result_return_if!(ctx.ctx.object_info.uses_tipc_protocol(), $crate::ipc::rc::ResultInvalidProtocol);

                        ctx.raw_data_walker = $crate::ipc::DataWalker::new(ctx.ctx.in_params.data_offset);
                        $( let $in_param_name = <$in_param_type as $crate::ipc::server::RequestCommandParameter<_>>::after_request_read(&mut ctx)?; )*

                        let ( $( $out_param_name ),* ) = self.$name( $( $in_param_name ),* )?;

                        ctx.raw_data_walker = $crate::ipc::DataWalker::new(core::ptr::null_mut());
                        $( let [<$out_param_name _carry_state>] = $crate::ipc::server::ResponseCommandParameter::before_response_write(&$out_param_name, &mut ctx)?; )*
                        ctx.ctx.out_params.data_size = ctx.raw_data_walker.get_offset() as u32;

                        $crate::ipc::cmif::server::write_control_command_response_on_msg_buffer(&mut ctx.ctx, $crate::result::ResultSuccess::make(), $crate::ipc::cmif::CommandType::Control);

                        ctx.raw_data_walker = $crate::ipc::DataWalker::new(ctx.ctx.out_params.data_offset);
                        $( $crate::ipc::server::ResponseCommandParameter::after_response_write($out_param_name, [<$out_param_name _carry_state>], &mut ctx)?; )*

                        Ok(())
                    }
                )*

                fn try_handle_request_by_id(&mut self, rq_id: u32, protocol: $crate::ipc::CommandProtocol, ctx: &mut $crate::ipc::server::ServerContext) -> Option<$crate::result::Result<()>> {
                    match rq_id {
                        $(
                            $rq_id if $ver_intv.contains($crate::version::get_version()) => {
                                Some(self.[<_sf_server_impl_ $name>](protocol, ctx))
                            }
                        ),*
                        _ => None
                    }
                }
            }
        }
    };
}

#[macro_export]
macro_rules! server_mark_request_command_parameters_types_as_copy {
    ($($t:ty),*) => {
        $(
        impl $crate::ipc::server::RequestCommandParameter<'_,$t> for $t {
            fn after_request_read(ctx: &mut $crate::ipc::server::ServerContext) -> $crate::result::Result<Self> {
                Ok(ctx.raw_data_walker.advance_get())
            }
        }

        impl $crate::ipc::server::ResponseCommandParameter for $t {
            type CarryState = ();
            fn before_response_write(_raw: &Self, ctx: &mut $crate::ipc::server::ServerContext) -> $crate::result::Result<()> {
                ctx.raw_data_walker.advance::<Self>();
                Ok(())
            }

            fn after_response_write(raw: Self, _carry_state: (), ctx: &mut $crate::ipc::server::ServerContext) -> $crate::result::Result<()> {
                ctx.raw_data_walker.advance_set(raw);
                Ok(())
            }
        }
        )*
    };
}

#[macro_export]
macro_rules! api_mark_request_command_parameters_types_as_copy {
    ($($t:ty),*) => {
        $(
        server_mark_request_command_parameters_types_as_copy!($t);
        client_mark_request_command_parameters_types_as_copy!($t);
        )*
    };
}

api_mark_request_command_parameters_types_as_copy!(
    bool, u8, i8, u16, i16, u32, i32, u64, i64, usize, isize, u128, i128, f32, f64
);

impl<T: Copy, const N: usize> crate::ipc::server::RequestCommandParameter<'_, [T; N]> for [T; N] {
    fn after_request_read(
        ctx: &mut crate::ipc::server::ServerContext,
    ) -> crate::result::Result<Self> {
        Ok(ctx.raw_data_walker.advance_get())
    }
}

impl<T: Copy, const N: usize> crate::ipc::server::ResponseCommandParameter for [T; N] {
    type CarryState = ();
    fn before_response_write(
        _raw: &Self,
        ctx: &mut crate::ipc::server::ServerContext,
    ) -> crate::result::Result<()> {
        ctx.raw_data_walker.advance::<Self>();
        Ok(())
    }

    fn after_response_write(
        raw: Self,
        _carry_state: (),
        ctx: &mut crate::ipc::server::ServerContext,
    ) -> crate::result::Result<()> {
        ctx.raw_data_walker.advance_set(raw);
        Ok(())
    }
}
