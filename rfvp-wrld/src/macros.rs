use crate::converter::{convert_type_to_wgpu, has_type, convert_mat_type_to_wgou};
use crate::parser::TokenVertexFormat;
use crate::parser::parse_attrs;

#[derive(Debug)]
struct Entity {
    fields: Vec<EntityFields>
}

#[derive(Debug)]
struct EntityFieldsAttrs {
    name: String,
    data: u32,
    ty: Option<String>
}

#[derive(Debug)]
struct EntityFields {
    attrs: Vec<EntityFieldsAttrs>,
    name: proc_macro2::Ident,
    ty: syn::Type
}

fn get_entity_field(field: &syn::Field) -> Option<EntityFields> {
    let mut attrs: Vec<EntityFieldsAttrs> = Vec::new();

    parse_attrs(&field.attrs,Box::new(|attr| {
        let name = attr.segment.ident.to_string();
        if name.starts_with("mat") {
            let mat : crate::parser::AttrMat = attr.attribute.parse_args().unwrap();
            
            attrs.push(EntityFieldsAttrs {
                name,
                data: mat.data,
                ty: Some(mat.ident.to_string()),
            });

            return
        }

        let lint : syn::LitInt = attr.attribute.parse_args().expect("Only integer is authorize for shader location data");
    
        attrs.push(EntityFieldsAttrs {
            name: attr.segment.ident.to_string(),
            data: lint.base10_parse().unwrap(),
            ty: None
        });
    }));

    let entity_fields = EntityFields {
        attrs,
        name: field.ident.clone().unwrap(),
        ty: field.ty.clone()
    };

    Some(entity_fields)
}

fn process_wgpu_type(
    format: &crate::converter::WGPUData, 
    shader_locations: &mut Vec<u32>,
    attrs: &mut Vec<proc_macro2::TokenStream>,
    offset: &u64
) {
    let tty = TokenVertexFormat { attribute: format.wgpu_type.ty};
    let shader_location = format.shader_location;

    if shader_locations.contains(&shader_location) {
        panic!("Cannot have two time the same location in the same struct");
    }

    shader_locations.push(shader_location);

    attrs.push(quote::quote! {
        wgpu::VertexAttribute {
            offset: #offset,
            format: #tty,
            shader_location: #shader_location
        }
    });
}

fn require_repr_c(attrs : &std::vec::Vec<syn::Attribute>) {
    let mut valid = false;

    parse_attrs(&attrs, Box::new(|attr| {
        let repr_attr = attr.attribute.parse_args::<syn::Ident>().unwrap().to_string();
        if attr.segment.ident.to_string() == "repr" && (repr_attr == "C" || repr_attr == "transparent") {
            valid = true;
        }
    }));

    if !valid {
        panic!("wrld::Desc derive macro require #[repr(C)] or #[repr(transparent)] attribute for safety measure");
    }
}

pub fn derive_wrld_desc(item: proc_macro::TokenStream, step_mode: wgpu::VertexStepMode) -> proc_macro::TokenStream {
    let syn::DeriveInput {ident, data, attrs, ..} = syn::parse_macro_input!(item as syn::DeriveInput);
    let fields = if let syn::Data::Struct(syn::DataStruct {
        fields: syn::Fields::Named(syn::FieldsNamed { ref named, ..}),
        ..
    }) = data 
    {
        named
    } else {
        panic!("Only struct are supported by wrld::Desc supported");
    };

    require_repr_c(&attrs);

    let entity = Entity {
        fields: fields.iter().filter_map(|field| {get_entity_field(field)}).collect()
    };

    let mut attrs : Vec<proc_macro2::TokenStream> = Vec::new();

    let mut offset:u64 = 0;
    let mut shader_locations: Vec<u32> = Vec::new();

    for i in entity.fields {
        for attr in i.attrs {
            if attr.ty == None {
                let format = convert_type_to_wgpu(&attr.name, attr.data).unwrap();
                process_wgpu_type(&format, &mut shader_locations, &mut attrs, &offset);
                offset += format.wgpu_type.offset;
            } else {
                let mat_format = convert_mat_type_to_wgou(
                    &attr.name, 
                    attr.data,
                    &mut attr.ty.unwrap()
                );

                for format in mat_format {
                    process_wgpu_type(&format, &mut shader_locations, &mut attrs, &offset);
                    offset += format.wgpu_type.offset;
                }
            }
        }
    }

    let step_mode = crate::parser::TokenVertexStepMode {step_mode};

    quote::quote! {
        impl #ident {
            pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
                wgpu::VertexBufferLayout {
                    array_stride: #offset as wgpu::BufferAddress,
                    step_mode: #step_mode,
                    attributes: &[#(#attrs),*]
                }
            }
        }
    }.into()
}

pub fn derive_wrld_buffer_data(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let syn::DeriveInput {ident, data, ..} = syn::parse_macro_input!(item as syn::DeriveInput);
    let fields = if let syn::Data::Struct(syn::DataStruct {
        fields: syn::Fields::Named(syn::FieldsNamed { ref named, ..}),
        ..
    }) = data 
    {
        named
    } else {
        panic!("This is not supported");
    };

    let entity = Entity {
        fields: fields.iter().filter_map(|field| {get_entity_field(field)}).collect()
    };

    let subclass_name = quote::format_ident!("{}{}", ident, "BufferData");

    let mut struct_fields : Vec<proc_macro2::TokenStream> = Vec::new();
    let mut equal_fields : Vec<proc_macro2::TokenStream> = Vec::new();
    let mut partial_eq_fields : Vec<proc_macro2::TokenStream> = Vec::new();
    let mut into_fields : Vec<proc_macro2::TokenStream> = Vec::new();

    entity.fields.iter().for_each(|f| {
        let mut process = false;

        for i in &f.attrs {
            process = has_type(&i.name);
        }

        if process
        {
            let name = f.name.clone();
            let ty = f.ty.clone();

            struct_fields.push(quote::quote! {
                #name: #ty
            });

            equal_fields.push(quote::quote! {
                #name: other_data_from_ident_to_into.#name
            });

            partial_eq_fields.push(quote::quote! {
                self.#name == other_ident_data_boolean_condition.#name
            });

            into_fields.push(quote::quote! {
                #name: other_ident_data_to_into_const.#name
            });
        }
    });

    let ident_regex_lowercase = regex::Regex::new(r"(?P<M>[A-Z])").unwrap();
    let ident_string = ident.to_string();
    let replace_all = ident_regex_lowercase.replace_all(ident_string.as_str(), "_$M");
    let mut result = replace_all.to_ascii_lowercase();
    
    if result.chars().nth(0).unwrap() == '_' {
        result.remove(0);
    }

    let const_into_macro = quote::format_ident!("{}_const_into", result);
    let mutate_data_macro = quote::format_ident!("mutate_{}", result);

    quote::quote! {
        #[repr(C)]
        #[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        struct #subclass_name {
            #(#struct_fields),*
        }

        impl From<#ident> for #subclass_name {
            fn from(other_data_from_ident_to_into: #ident) -> Self {
                Self {
                    #(#equal_fields),*
                }
            }
        }

        impl From<&'static #ident> for #subclass_name {
            fn from(other_data_from_ident_to_into: &'static #ident) -> Self {
                Self {
                    #(#equal_fields),*
                }
            }
        }

        impl PartialEq<#ident> for #subclass_name {
            fn eq(&self, other_ident_data_boolean_condition: &#ident) -> bool {
                #(#partial_eq_fields)&&*
            }
        }

        impl FromIterator<#ident> for Vec<#subclass_name> {
            fn from_iter<T: IntoIterator<Item = #ident>>(iter: T) -> Self {
                let mut vec_data_from_ident_from_iterator = Vec::new();

                for c in iter {
                    vec_data_from_ident_from_iterator.push(c.into());
                }

                vec_data_from_ident_from_iterator
            }
        }

        impl FromIterator<&'static #ident> for Vec<#subclass_name> {
            fn from_iter<T: IntoIterator<Item = &'static #ident>>(iter: T) -> Self {
                let mut vec_data_from_ident_single_from_iterator : Vec<#subclass_name> = Vec::new();

                for c in iter {
                    vec_data_from_ident_single_from_iterator.push(c.into());
                }

                vec_data_from_ident_single_from_iterator
            }
        }

        impl #subclass_name {
            pub const fn const_into(other_ident_data_to_into_const: &#ident) -> Self {
                Self {
                    #(#into_fields),*
                }
            }
        }

        impl #ident {
            pub fn mutate<'a>(other_data_from_ident_to_mutate: &'a Vec<#subclass_name>) -> &'a [u8] {
                bytemuck::cast_slice(other_data_from_ident_to_mutate.as_slice())
            }

            pub fn transmute(other_data_from_ident_to_transmute: &'static [Self]) -> Vec<#subclass_name> {
                other_data_from_ident_to_transmute.into_iter().collect::<Vec<#subclass_name>>() 
            }
        }

        #[allow(unused_macros)]
        macro_rules! #const_into_macro {
            ($data: expr) => {
                #subclass_name::const_into(&$data)
            };
        }
        
        #[allow(unused_macros)]
        macro_rules! #mutate_data_macro {
            ($data: expr) => {
                #ident::mutate(&#ident::transmute($data))
            };
        }
    }.into()
}