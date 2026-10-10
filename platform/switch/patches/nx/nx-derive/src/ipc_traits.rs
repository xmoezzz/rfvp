use std::str::FromStr;

use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote};
use syn::{
    AngleBracketedGenericArguments, AttrStyle, FnArg, GenericArgument, Path, PathSegment,
    ReturnType, TraitItem, TraitItemFn, Type, TypePath,
    punctuated::Punctuated,
    spanned::Spanned,
    token::{Gt, Lt, Mut, PathSep},
};

pub fn ipc_trait(_args: TokenStream, ipc_trait: TokenStream) -> syn::Result<TokenStream> {
    let input: syn::ItemTrait = syn::parse2::<syn::ItemTrait>(ipc_trait)?;

    let name = input.ident;
    let vis = input.vis;

    let client_trait = format_ident!("I{}Client", name);
    let server_trait = format_ident!("I{}Server", name);

    let build_default_client = input.attrs.iter().any(|attr| {
        if let syn::Attribute {
            meta: syn::Meta::Path(p),
            ..
        } = attr
        {
            p.is_ident("default_client")
        } else {
            false
        }
    });

    let default_client: TokenStream = if build_default_client {
        quote! {
            #[doc = concat!("The default client for the `", stringify!(#name), "` trait. All implementors of the trait need to read their session in accordance with this Types IPC Parameter traits.")]
            pub struct #name {
                #[doc(hidden)]
                pub (crate) session: ::nx::ipc::sf::Session
            }

            impl ::nx::ipc::client::IClientObject for #name {
                fn new(session: ::nx::ipc::sf::Session) -> Self {
                    Self { session }
                }

                fn get_session(&self) -> & ::nx::ipc::sf::Session {
                    &self.session
                }

                fn get_session_mut(&mut self) -> &mut ::nx::ipc::sf::Session {
                    &mut self.session
                }
            }

            unsafe impl ::core::marker::Sync for #name {}
            unsafe impl ::core::marker::Send for #name {}

            impl ::nx::ipc::client::RequestCommandParameter for #name {
                fn before_request_write(session: &Self, _walker: &mut ::nx::ipc::DataWalker, ctx: &mut ::nx::ipc::CommandContext) -> ::nx::result::Result<()> {
                    ctx.in_params.add_object(session.session.object_info)
                }

                fn before_send_sync_request(_session: &Self, _walker: &mut ::nx::ipc::DataWalker, _ctx: &mut ::nx::ipc::CommandContext) -> ::nx::result::Result<()> {
                    Ok(())
                }
            }

            impl ::nx::ipc::client::ResponseCommandParameter<#name> for #name {
                fn after_response_read(_walker: &mut ::nx::ipc::DataWalker, ctx: &mut ::nx::ipc::CommandContext) -> ::nx::result::Result<Self> {
                    let object_info = ctx.pop_object()?;
                    Ok(Self { session: ::nx::ipc::sf::Session::from(object_info)})
                }
            }
            impl ::nx::ipc::server::RequestCommandParameter<'_, #name> for #name {
                fn after_request_read(_ctx: &mut ::nx::ipc::server::ServerContext) -> ::nx::result::Result<Self> {
                    use ::nx::result::ResultBase;
                    // TODO: determine if we need to do this, since this is a server side operation of a client object?
                    // probably needs to be supported right?
                    ::nx::ipc::sf::hipc::rc::ResultUnsupportedOperation::make_err()
                }
            }

            impl ::nx::ipc::server::ResponseCommandParameter for #name {
                type CarryState = ();
                fn before_response_write(_session: &Self, _ctx: &mut ::nx::ipc::server::ServerContext) -> ::nx::result::Result<()> {
                    use ::nx::result::ResultBase;
                    // TODO: determine if we need to do this, since this is a server side operation of a client object?
                    // probably needs to be supported right?
                    ::nx::ipc::sf::hipc::rc::ResultUnsupportedOperation::make_err()
                }

                fn after_response_write(_session: Self, _carry_state: (), _ctx: &mut ::nx::ipc::server::ServerContext) -> ::nx::result::Result<()> {
                    use ::nx::result::ResultBase;
                    // TODO: determine if we need to do this, since this is a server side operation of a client object?
                    // probably needs to be supported right?
                    ::nx::ipc::sf::hipc::rc::ResultUnsupportedOperation::make_err()
                }
            }

            impl #client_trait for #name {}
        }
    } else {
        quote! {}
    };

    for item in input.items.iter() {
        if !matches!(item, TraitItem::Fn(_)) {
            return Err(stringify_error(
                item.span(),
                "Only function items are supported for ipc_trait derivations",
            ));
        }
    }

    let mut client_fns = vec![];
    let mut server_fns = vec![];
    let mut handle_request_matches = vec![];
    let mut item_iter = input.items.iter();
    while let Some(TraitItem::Fn(fn_item)) = item_iter.next() {
        if fn_item.default.is_some() {
            return Err(stringify_error(
                fn_item.span(),
                "No default implementations are supported for ipc_trait derivations",
            ));
        }

        let fn_name = fn_item.sig.ident.clone();
        let mut ipc_rid: Option<u32> = None;
        let mut version_req = None;
        let mut return_type_is_session = false;
        let mut return_wrap_result = true;
        let mut doc_comments: Vec<syn::Attribute> = vec![];
        for attr in fn_item.attrs.iter() {
            if let syn::Attribute {
                style: AttrStyle::Outer,
                meta:
                    syn::Meta::List(syn::MetaList {
                        path,
                        delimiter: _,
                        tokens,
                    }),
                ..
            } = attr
            {
                if path.is_ident("ipc_rid") {
                    ipc_rid = Some(syn::parse2::<syn::LitInt>(tokens.clone())?.base10_parse()?);
                } else if path.is_ident("version") {
                    version_req = Some(syn::parse2::<syn::Expr>(tokens.clone())?);
                } else {
                    return Err(stringify_error(
                        fn_item.span(),
                        "Only the `ipc_rid`, `version`, `no_wrap_return`, and `return_session` attrs are supported on ipc trait functions (plus doc comments)",
                    ));
                }
            } else if let syn::Attribute {
                meta: syn::Meta::Path(p),
                ..
            } = attr
            {
                if p.is_ident("return_session") {
                    return_type_is_session = true;
                } else if p.is_ident("no_wrap_return") {
                    return_wrap_result = false
                } else {
                    return Err(stringify_error(
                        fn_item.span(),
                        "Only the `ipc_rid`, `version` `no_wrap_return`, and `return_session` attrs are supported on ipc trait functions (plus doc comments)",
                    ));
                }
            } else if let syn::Attribute {
                style: AttrStyle::Outer,
                meta: syn::Meta::NameValue(syn::MetaNameValue { path, .. }),
                ..
            } = attr
                && path.is_ident("doc")
            {
                doc_comments.push(attr.clone());
            } else {
                return Err(stringify_error(
                    fn_item.span(),
                    "Only the `ipc_rid`, `version` `no_wrap_return`, and `return_session` attrs are supported on ipc trait functions (plus doc comments)",
                ));
            }
        }

        if ipc_rid.is_none() {
            return Err(stringify_error(
                fn_item.span(),
                "IPC member functions must have an assigned request id",
            ));
        }

        let ipc_rid = ipc_rid.unwrap();

        let version_req = version_req
            .unwrap_or(syn::parse2(quote! {::nx::version::VersionInterval::all()}).unwrap());

        // fix up the return types of the client functions to return nx::result::Result
        let mut client_fn = fn_item.clone();
        client_fn.attrs = doc_comments.clone();

        let mut client_in_param_names = vec![];
        let mut server_in_param_names = vec![];
        let mut in_param_types = vec![];
        client_fn
            .sig
            .inputs
            .iter()
            .skip(1)
            .try_for_each(|fn_args| {
                let arg_span = fn_args.span();
                let arg_pat = match fn_args {
                    FnArg::Typed(pat) => pat,
                    _ => {
                        return Err(stringify_error(
                        arg_span,
                        "We should only have non-receiver arguments after skipping the first arg.",
                    ));
                    }
                };

                let arg_name = match *arg_pat.pat.clone() {
                    syn::Pat::Ident(pat_ident) => pat_ident.ident,
                    _ => {
                        return Err(stringify_error(
                            arg_span,
                            "Only basic ident names are supported.",
                        ));
                    }
                };

                client_in_param_names.push(arg_pat.pat.clone());
                server_in_param_names.push(format_ident!("in_param_{}", arg_name));
                in_param_types.push(arg_pat.ty.clone());

                Ok(())
            })?;

        let mut out_param_names = vec![];
        let mut out_param_types = vec![];
        if return_wrap_result {
            match client_fn.sig.output.clone() {
                ReturnType::Default => {}
                ReturnType::Type(_, ty) => match *ty {
                    Type::Tuple(tuple) => {
                        let types: Vec<Type> = (0..)
                            .map_while(|off| tuple.elems.get(off).cloned())
                            .collect();

                        let _: () = types
                            .into_iter()
                            .enumerate()
                            .map(|(off, t)| {
                                out_param_names.push(format_ident!("out_param_{}", off));
                                out_param_types.push(t);
                            })
                            .collect();
                    }
                    Type::Paren(ty_pat) => {
                        out_param_names.push(format_ident!("out_param_0"));
                        out_param_types.push((*ty_pat.elem).clone());
                    }
                    Type::Path(ty_path) => {
                        out_param_names.push(format_ident!("out_param_0"));
                        out_param_types.push(Type::Path(ty_path));
                    }
                    _ => {
                        return Err(stringify_error(
                            client_fn.sig.output.span(),
                            "Only tuple types, paren-wrapped types, or paths are supported for return types",
                        ));
                    }
                },
            }
        }
        client_fn.default = Some(syn::parse2(quote! {
                {
                    let mut ctx = ::nx::ipc::CommandContext::new_client(self.get_session().object_info);

                    let mut walker = ::nx::ipc::DataWalker::new(core::ptr::null_mut());

                    #(::nx::ipc::client::RequestCommandParameter::before_request_write(&#client_in_param_names, &mut walker, &mut ctx)?;)*

                    ctx.in_params.data_size = walker.get_offset() as u32;

                    match self.get_session().object_info.protocol {
                        ::nx::ipc::CommandProtocol::Cmif => ::nx::ipc::cmif::client::write_request_command_on_msg_buffer(&mut ctx, Some(#ipc_rid), ::nx::ipc::cmif::DomainCommandType::SendMessage),
                        ::nx::ipc::CommandProtocol::Tipc => ::nx::ipc::tipc::client::write_request_command_on_msg_buffer(&mut ctx, #ipc_rid)
                    };

                    walker.reset_with(ctx.in_params.data_offset);
                    #( ::nx::ipc::client::RequestCommandParameter::before_send_sync_request(&#client_in_param_names, &mut walker, &mut ctx)?; )*

                    ::nx::svc::send_sync_request(self.get_session().object_info.handle)?;

                    match self.get_session().object_info.protocol {
                        ::nx::ipc::CommandProtocol::Cmif => ::nx::ipc::cmif::client::read_request_command_response_from_msg_buffer(&mut ctx)?,
                        ::nx::ipc::CommandProtocol::Tipc => ::nx::ipc::tipc::client::read_request_command_response_from_msg_buffer(&mut ctx)?
                    };

                    walker.reset_with(ctx.out_params.data_offset);
                    #( let #out_param_names = <#out_param_types as ::nx::ipc::client::ResponseCommandParameter<_>>::after_response_read(&mut walker, &mut ctx)?; )*

                    Ok(( #(#out_param_names as _),* ))
                }
        })?);
        client_fn.semi_token = None;

        let output_span = client_fn.sig.output.span();
        match &mut client_fn.sig.output {
            ReturnType::Default => {
                client_fn.sig.output = syn::parse2::<ReturnType>(
                    FromStr::from_str(" -> ::nx::result::Result<()>").unwrap(),
                )
                .unwrap()
            }
            ReturnType::Type(_, ty) => {
                let mut wrapped_result_generic = Punctuated::new();
                wrapped_result_generic.push(GenericArgument::Type(*ty.clone()));

                let mut result_path = Punctuated::new();
                result_path.push(PathSegment {
                    ident: Ident::new_raw("nx", output_span),
                    arguments: syn::PathArguments::None,
                });
                result_path.push(PathSegment {
                    ident: Ident::new_raw("result", output_span),
                    arguments: syn::PathArguments::None,
                });
                result_path.push(PathSegment {
                    ident: Ident::new_raw("Result", output_span),
                    arguments: syn::PathArguments::AngleBracketed(AngleBracketedGenericArguments {
                        colon2_token: None,
                        lt_token: Lt::default(),
                        args: wrapped_result_generic,
                        gt_token: Gt::default(),
                    }),
                });

                *ty = Box::new(Type::Path(TypePath {
                    qself: None,
                    path: Path {
                        leading_colon: Some(PathSep::default()),
                        segments: result_path,
                    },
                }));
            }
        };

        client_fns.push(client_fn);

        // fix the return type for the server function types if
        let mut server_fn = fn_item.clone();
        server_fn.attrs = doc_comments;

        if let Some(FnArg::Receiver(r)) = server_fn.sig.inputs.iter_mut().next() {
            // all server functions are considered &mut borrowing
            r.mutability = Some(Mut::default());
            r.ty = Box::new(syn::parse2(quote! {&mut Self}).unwrap());
        } else {
            return Err(stringify_error(
                server_fn.span(),
                "IPC traits with associated functions is not supported.",
            ));
        }

        if return_type_is_session {
            if let ReturnType::Type(_, bty) = &mut server_fn.sig.output {
                match *bty.clone() {
                    Type::Path(ty) => {
                        if ty.path.segments.len() != 1 {
                            return Err(stringify_error(
                                server_fn.sig.output.span(),
                                "Output type be a raw type name (the base name of the traits) the return type is marked as a session type",
                            ));
                        }
                        let out_type_ident = format!(
                            "impl I{}Server + 'static",
                            ty.path.segments.last().unwrap().ident
                        );
                        *bty = Box::new(syn::parse2::<Type>(FromStr::from_str(
                            out_type_ident.as_str(),
                        )?)?);
                    }
                    Type::Tuple(mut tup_ty) => {
                        if let Some(first_mut_ref) = tup_ty.elems.first_mut() {
                            if let Type::Path(ty) = first_mut_ref.clone() {
                                if ty.path.segments.len() != 1 {
                                    return Err(stringify_error(
                                        server_fn.sig.output.span(),
                                        "Output type be a raw type name (the base name of the traits) the return type is marked as a session type",
                                    ));
                                }
                                *first_mut_ref = syn::parse2(FromStr::from_str(
                                    format!(
                                        "impl I{}Server + 'static",
                                        ty.path.segments.last().unwrap().ident
                                    )
                                    .as_str(),
                                )?)?;
                            } else {
                                return Err(stringify_error(
                                    server_fn.sig.output.span(),
                                    "The first element of a tuple return type must be a bare type name when returning a session type",
                                ));
                            }

                            *bty = Box::new(Type::Tuple(tup_ty));
                        } else {
                            return Err(stringify_error(
                                server_fn.sig.output.span(),
                                "Tuple type outputs must have at least 2 elements when specifying a session type. If one element is being returned, omit the braces to return the bare type.",
                            ));
                        }
                    }
                    _ => {
                        return Err(stringify_error(
                            server_fn.sig.output.span(),
                            "Output type be a raw type name (the base name of the traits) or a tuple with the base name as the first element, if the return type is marked as a session type",
                        ));
                    }
                }
            }
        }

        // now that the return type for server functions has been fixed, we can apply the same T -> Result<T> from the client functions
        match &mut server_fn.sig.output {
            ReturnType::Default => {
                server_fn.sig.output = syn::parse2::<ReturnType>(
                    FromStr::from_str(" -> ::nx::result::Result<()>").unwrap(),
                )
                .unwrap()
            }
            ReturnType::Type(_, ty) => {
                let mut wrapped_result_generic = Punctuated::new();
                wrapped_result_generic.push(GenericArgument::Type(*ty.clone()));

                let mut result_path = Punctuated::new();
                result_path.push(PathSegment {
                    ident: Ident::new_raw("nx", Span::call_site()),
                    arguments: syn::PathArguments::None,
                });
                result_path.push(PathSegment {
                    ident: Ident::new_raw("result", Span::call_site()),
                    arguments: syn::PathArguments::None,
                });
                result_path.push(PathSegment {
                    ident: Ident::new_raw("Result", Span::call_site()),
                    arguments: syn::PathArguments::AngleBracketed(AngleBracketedGenericArguments {
                        colon2_token: None,
                        lt_token: Lt::default(),
                        args: wrapped_result_generic,
                        gt_token: Gt::default(),
                    }),
                });

                *ty = Box::new(Type::Path(TypePath {
                    qself: None,
                    path: Path {
                        leading_colon: Some(PathSep::default()),
                        segments: result_path,
                    },
                }));
            }
        };
        server_fns.push(server_fn);

        let server_impl_fn_name = format_ident!("sf_server_impl_{}", fn_name);
        let carry_state_names: Vec<Ident> = out_param_names
            .iter()
            .map(|ident| format_ident!("{}_carry_state", ident))
            .collect();
        let server_internal_fn: TraitItemFn = syn::parse2(quote! {
            #[allow(unused_assignments)]
            #[allow(unused_parens)]
            #[allow(unused_mut)]
            #[doc(hidden)]
            fn #server_impl_fn_name(&mut self, protocol: ::nx::ipc::CommandProtocol, mut ctx: &mut ::nx::ipc::server::ServerContext) -> ::nx::result::Result<()> {
                use ::nx::result::ResultBase;

                ctx.raw_data_walker = ::nx::ipc::DataWalker::new(ctx.ctx.in_params.data_offset);
                #( let #server_in_param_names = <#in_param_types as ::nx::ipc::server::RequestCommandParameter<_>>::after_request_read(&mut ctx)?; )*

                let ( #( #out_param_names ),* ) = unsafe {self.#fn_name( #( #server_in_param_names ),* )? };

                ctx.raw_data_walker = ::nx::ipc::DataWalker::new(core::ptr::null_mut());
                #( let #carry_state_names = ::nx::ipc::server::ResponseCommandParameter::before_response_write(&#out_param_names, &mut ctx)?; )*
                ctx.ctx.out_params.data_size = ctx.raw_data_walker.get_offset() as u32;

                match protocol {
                    ::nx::ipc::CommandProtocol::Cmif => {
                        ::nx::ipc::cmif::server::write_request_command_response_on_msg_buffer(&mut ctx.ctx, ::nx::result::ResultSuccess::make(), ::nx::ipc::cmif::CommandType::Request);
                    },
                    ::nx::ipc::CommandProtocol::Tipc => {
                        ::nx::ipc::tipc::server::write_request_command_response_on_msg_buffer(&mut ctx.ctx, ::nx::result::ResultSuccess::make(), 16); // TODO: is this command type actually read/used/relevant?
                    }
                };

                ctx.raw_data_walker = ::nx::ipc::DataWalker::new(ctx.ctx.out_params.data_offset);
                #( ::nx::ipc::server::ResponseCommandParameter::after_response_write(#out_param_names, #carry_state_names, &mut ctx)?; )*

                Ok(())
            }
        })?;

        server_fns.push(server_internal_fn);
        handle_request_matches.push(quote! {
            #ipc_rid if (#version_req).contains(version) => {
                Some(self.#server_impl_fn_name(protocol, ctx))
            }
        });
    }

    Ok(quote! {
        #default_client

        #vis trait #client_trait: ::nx::ipc::client::IClientObject + Sync {
            #(
                #[allow(unused_parens)]
                #[allow(clippy::too_many_arguments)]
                #[allow(missing_docs)]
                #client_fns
            )*
        }

        #vis trait #server_trait: ::nx::ipc::server::ISessionObject + Sync {
            #(
                #[allow(unused_parens)]
                #[allow(clippy::too_many_arguments)]
                #[allow(missing_docs)]
                #server_fns
            )*

            /// The dynamic dispatch function that calls into the IPC server functions. This should only be called from the [`ServerManager`][`::nx::ipc::server::ServerManager`] and not from client code.
            ///
            /// Examples for implementing [`ISessionObject`][`::nx::ipc::server::ISessionObject`] or [`IMitmServerOject`][`::nx::ipc::server::IMitmServerObject`] can be found in the [examples](https://github.com/aarch64-switch-rs/examples/) crate.
            fn try_handle_request_by_id(&mut self, req_id: u32, protocol: ::nx::ipc::CommandProtocol, ctx: &mut ::nx::ipc::server::ServerContext) -> Option<::nx::result::Result<()>> {
                let version = ::nx::version::get_version();
                match req_id {
                    #(
                        #handle_request_matches
                    ),*
                    _ => None
                }
            }
        }
    })
}

fn stringify_error(span: proc_macro2::Span, msg: impl std::fmt::Display) -> syn::Error {
    syn::Error::new(span, msg)
}
