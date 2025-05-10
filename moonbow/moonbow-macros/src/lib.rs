use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput};
use syn::spanned::Spanned;
use quote::quote;
use quote::quote_spanned;

#[proc_macro_derive(RegBlock, attributes(base, size, offset, reset))]
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
    let namestr = name.to_string();

    let base = if let Some(baseattr) = input.attrs.iter().find(|a| a.path().is_ident("base")) {
        let lit: syn::LitInt = baseattr.parse_args().unwrap();
        let n = lit.base10_parse::<u32>().unwrap();
        quote! { Some(#n) }
    } else {
        quote! { None }
    };

    let mut off: u32 = 0;
    let reginfos = if let syn::Data::Struct(data) = &input.data {
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

    let size = if let Some(sizeattr) = input.attrs.iter().find(|a| a.path().is_ident("size")) {
        let lit: syn::LitInt = sizeattr.parse_args().unwrap();
        lit.base10_parse::<u32>().unwrap()
    } else {
        off
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

            fn size(&self) -> u32 {
                #size
            }

            fn name(&self) -> &'static str {
                #namestr
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


// ------------------------------------------------------------------------------------------------

struct Register {
    name: syn::Ident,
    ty: syn::Type,
    offset: u32,
    reset: u32,
}

#[proc_macro_derive(Peripheral, attributes(register))]
pub fn peripheral(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    impl_peripheral(&input)
}

fn impl_peripheral(input: &DeriveInput) -> TokenStream {
    let name = &input.ident;
    let namestr = name.to_string();

    let mut off: u32 = 0;
    let reginfos = if let syn::Data::Struct(data) = &input.data {
        data.fields.iter().filter(|f| f.attrs.iter().any(|a| {
            a.path().is_ident("register")
        })).map(move |f| {
            match &f.ty {
                syn::Type::Path(p) if p.path.is_ident("u32") => {},
                syn::Type::Tuple(t) if t.elems.is_empty() => {},
                _ => {
                    return Err(quote_spanned! {
                        f.ty.span() => compile_error!("Expected u32 or ()");
                    });
                }
            }
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
            let r = Register {
                name: f.ident.clone().unwrap(),
                ty: f.ty.clone(),
                offset: off,
                reset: res,
            };
            off += 4;
            Ok(r)
        }).collect::<Vec<_>>()
    } else {
        panic!("RegisterBlock can only be used on structs");
    };

    let errors: Vec<_> = reginfos.iter().filter_map(|r| match r {
        Err(ts) => Some(ts),
        _ => None
    }).collect();

    // Match statements for read() function
    let read_matches: Vec<_> = reginfos.iter().filter_map(|r| match r {
        Ok(r) => {
            let rname = &r.name;
            let offset = r.offset >> 2;

            Some(match &r.ty {
                syn::Type::Path(p) if p.path.is_ident("u32") => {
                    quote! { #offset => Ok(self.#rname), }
                },
                syn::Type::Tuple(t) if t.elems.is_empty() => {
                    let gname = format!("get_{}", rname);
                    let getter = syn::Ident::new(&gname, rname.span());
                    quote! { #offset => self.#getter(), }
                },
                _ => panic!()
            })
        },
        _ => None
    }).collect();

    let expanded = quote! {
        impl #name {
            fn read_register(&self, base: u32, offset: u32) -> Result<u32, String> {
                match (offset >> 2) {
                    #(#read_matches)*
                    _ => Err(format!("No register mapped at 0x{:08x} ({}+0x{:x})",
                            base + offset, self.name(), offset))
                }
            }
        }

        #(#errors)*
    };

    TokenStream::from(expanded)
}
