use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput};
use quote::quote;

#[proc_macro_derive(RegBlock, attributes(base, offset, reset))]
pub fn register_block(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    impl_register_block(&input)
}

#[derive(Debug)]
struct RegisterInfo {
    name: syn::Ident,
    offset: u32,
    reset: u32,
}

fn impl_register_block(input: &DeriveInput) -> TokenStream {
    let name = &input.ident;

    let base = if let Some(baseattr) = input.attrs.iter().find(|a| a.path().is_ident("base")) {
        let lit: syn::LitInt = baseattr.parse_args().unwrap();
        let n = lit.base10_parse::<u32>().unwrap();
        quote! { Some(#n) }
    } else {
        quote! { None }
    };

    let reginfos = if let syn::Data::Struct(data) = &input.data {
        let mut off: u32 = 0;
        data.fields.iter().filter(|f| {
            if let syn::Type::Path(p) = &f.ty { p.path.is_ident("Register") } else { false }
        }).map(move |f| {
            let mut res = 0;
            for attr in &f.attrs {
                if attr.path().is_ident("offset") {
                    let lit: syn::LitInt = attr.parse_args().unwrap();
                    off = lit.base10_parse().unwrap();
                    assert!((off & 3) == 0, "Register address must be aligned on word boundary")
                } else if attr.path().is_ident("reset") {
                    let lit: syn::LitInt = attr.parse_args().unwrap();
                    res = lit.base10_parse().unwrap();
                }
            }
            let r = RegisterInfo {
                name: f.ident.clone().unwrap(),
                offset: off,
                reset: res,
            };
            off += 4;
            r
        }).collect::<Vec<_>>()
    } else {
        panic!("RegisterBlock can only be used on structs");
    };

    // Match statements for get() function
    let get_matches = reginfos.iter().map(|ri| {
        let rname = &ri.name;
        let offset = ri.offset >> 2;

        quote! {
            #offset => Some(&self.#rname),
        }
    });

    // Match statements for get_mut() function
    let get_mut_matches = reginfos.iter().map(|ri| {
        let rname = &ri.name;
        let offset = ri.offset >> 2;

        quote! {
            #offset => Some(&mut self.#rname),
        }
    });

    // Field initializers for new() function
    let new_fields = reginfos.iter().map(|ri| {
        let rname = &ri.name;
        let rnamestr = rname.to_string();
        let reset = ri.reset;

        quote! {
            #rname: Register { name: #rnamestr, value: #reset },
        }
    });

    // Field setters for reset() function
    let reset_fields = reginfos.iter().map(|ri| {
        let rname = &ri.name;
        let reset = ri.reset;

        quote! {
            self.#rname.value = #reset;
        }
    });

    let expanded = quote! {
        impl RegisterBlock for #name {
            fn get(&self, offset:u32) -> Option<&Register> {
                match (offset >> 2) {
                    #(#get_matches)*
                    _ => None
                }
            }

            fn get_mut(&mut self, offset:u32) -> Option<&mut Register> {
                match (offset >> 2) {
                    #(#get_mut_matches)*
                    _ => None
                }
            }

            fn base(&self) -> Option<u32> {
                #base 
            }
        }

        impl #name {
            pub fn new() -> Self {
                Self {
                    #(#new_fields)*
                }
            }

            pub fn reset(&mut self) {
                #(#reset_fields)*
            }
        }
    };

    TokenStream::from(expanded)
}
