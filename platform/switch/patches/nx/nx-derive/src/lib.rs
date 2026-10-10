use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

mod ipc_traits;

/// This creates the required trait implementations for the type to be used as an IPC request parameter.
/// As the type is directly copied into the buffer from an &Self, this will only work on `Copy` types.
#[proc_macro_derive(Request)]
pub fn derive_request(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let name = input.ident;
    TokenStream::from(quote!(
        impl ::nx::ipc::client::RequestCommandParameter for #name {
            fn before_request_write(_raw: &Self, walker: &mut ::nx::ipc::DataWalker, ctx: &mut ::nx::ipc::CommandContext) -> ::nx::result::Result<()> {
                walker.advance::<Self>();
                Ok(())
            }

            fn before_send_sync_request(raw: &Self, walker: &mut ::nx::ipc::DataWalker, ctx: &mut ::nx::ipc::CommandContext) -> ::nx::result::Result<()> {
                walker.advance_set(*raw);
                Ok(())
            }
        }

        impl ::nx::ipc::server::RequestCommandParameter<'_, #name> for #name {
            fn after_request_read(ctx: &mut ::nx::ipc::server::ServerContext<'_>) -> ::nx::result::Result<Self> {
                Ok(ctx.raw_data_walker.advance_get())
            }
        }
    ))
}

/// This creates the required trait implementations for the type to be used as an IPC response parameter.
/// As the type is directly copied into the buffer from an &Self, this will only work on `Copy` types.
#[proc_macro_derive(Response)]
pub fn derive_response(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let name = input.ident;
    let item_generics = &input.generics;
    TokenStream::from(quote!(
        impl #item_generics ::nx::ipc::client::ResponseCommandParameter<#name> for #name {
            fn after_response_read(walker: &mut ::nx::ipc::DataWalker, ctx: &mut ::nx::ipc::CommandContext) -> ::nx::result::Result<Self> {
                Ok(walker.advance_get())
            }
        }

        impl #item_generics ::nx::ipc::server::ResponseCommandParameter for #name {
            type CarryState = ();
            fn before_response_write(_raw: &Self, ctx: &mut ::nx::ipc::server::ServerContext) -> ::nx::result::Result<Self::CarryState> {
                ctx.raw_data_walker.advance::<Self>();
                Ok(())
            }
            fn after_response_write(raw: Self, _carry_state: Self::CarryState, ctx: &mut ::nx::ipc::server::ServerContext) -> ::nx::result::Result<()> {
                ctx.raw_data_walker.advance_set(raw);
                Ok(())
            }
        }
    ))
}

#[proc_macro_attribute]
pub fn ipc_trait(args: TokenStream, ipc_trait: TokenStream) -> TokenStream {
    match ipc_traits::ipc_trait(args.into(), ipc_trait.into()) {
        Ok(ts) => ts.into(),
        Err(e) => e.into_compile_error().into(),
    }
}
