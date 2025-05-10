use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput};
use syn::spanned::Spanned;
use quote::quote;
use quote::quote_spanned;


struct Register {
    name: syn::Ident,
    ty: syn::Type,
    offset: u32,
    reset: u32,
    read_const: Option<u32>,
    write_nop: bool,
}

#[proc_macro_derive(Peripheral, attributes(register, offset, reset, read_const, write_nop))]
pub fn peripheral(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    impl_peripheral(&input)
}

fn impl_peripheral(input: &DeriveInput) -> TokenStream {
    let name = &input.ident;
    //let namestr = name.to_string();

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
            let mut reset = 0;
            let mut read_const: Option<u32> = None;
            let mut write_nop = false;
            for attr in &f.attrs {
                if attr.path().is_ident("offset") {
                    let lit: syn::LitInt = attr.parse_args().unwrap();
                    off = lit.base10_parse().unwrap();
                    assert!((off & 3) == 0, "Register address must be aligned on word boundary")
                } else if attr.path().is_ident("reset") {
                    let lit: syn::LitInt = attr.parse_args().unwrap();
                    reset = lit.base10_parse().unwrap();
                } else if attr.path().is_ident("read_const") {
                    let lit: syn::LitInt = attr.parse_args().unwrap();
                    read_const = Some(lit.base10_parse().unwrap());
                } else if attr.path().is_ident("write_nop") {
                    write_nop = true;
                }
            }
            let r = Register {
                name: f.ident.clone().unwrap(),
                ty: f.ty.clone(),
                offset: off,
                reset,
                read_const,
                write_nop,
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

            Some(match r.read_const {
                Some(v) => {
                    quote! { #offset => Ok(#v), }
                },
                None => {
                    match &r.ty {
                        syn::Type::Path(p) if p.path.is_ident("u32") => {
                            quote! { #offset => Ok(self.#rname), }
                        },
                        syn::Type::Tuple(t) if t.elems.is_empty() => {
                            let gname = format!("get_{}", rname);
                            let getter = syn::Ident::new(&gname, rname.span());
                            quote! { #offset => self.#getter(), }
                        },
                        _ => panic!() // SNH
                    }
                }
            })
        },
        _ => None
    }).collect();

    // Match statements for write() function
    let write_matches: Vec<_> = reginfos.iter().filter_map(|r| match r {
        Ok(r) => {
            let rname = &r.name;
            let offset = r.offset >> 2;

            Some(match r.write_nop {
                true => {
                    quote! { #offset => { Ok(()) } }
                },
                false => {
                    match &r.ty {
                        syn::Type::Path(p) if p.path.is_ident("u32") => {
                            quote! { #offset => { self.#rname = value; Ok(()) } }
                        },
                        syn::Type::Tuple(t) if t.elems.is_empty() => {
                            let sname = format!("set_{}", rname);
                            let setter = syn::Ident::new(&sname, rname.span());
                            quote! { #offset => { self.#setter(value) } }
                        },
                        _ => panic!() // SNH
                    }
                }
            })
        },
        _ => None
    }).collect();

    // Reset statements for reset() function
    let reset_statements: Vec<_> = reginfos.iter().filter_map(|r| match r {
        Ok(r) => {
            let rname = &r.name;
            let rreset = &r.reset;

            match &r.ty {
                syn::Type::Path(p) if p.path.is_ident("u32") => {
                    Some(quote! { self.#rname = #rreset; })
                },
                _ => None
            }
        },
        _ => None
    }).collect();

    // Unused statements for unused() function
    let unused_statements: Vec<_> = reginfos.iter().filter_map(|r| match r {
        Ok(r) => {
            let rname = &r.name;

            match &r.ty {
                syn::Type::Tuple(t) if t.elems.is_empty() => {
                    Some(quote! { self.#rname; })
                },
                _ => None
            }
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

            fn write_register(&mut self, base: u32, offset: u32, value: u32) -> Result<(), String> {
                match (offset >> 2) {
                    #(#write_matches)*
                    _ => Err(format!("No register mapped at 0x{:08x} ({}+0x{:x})",
                            base + offset, self.name(), offset))
                }
            }

            fn reset_registers(&mut self)  {
                #(#reset_statements)*
            }

            #[allow(dead_code)]
            fn _unused_registers(&mut self)  {
                #(#unused_statements)*
            }

            #[allow(dead_code)]
            fn read_registers(&self, base: u32, offset: u32, size: u32) -> Result<u32, String> {
                self.read_register(base, offset).and_then(|v| {
                    if size == 4 && (offset & 3) == 0 {
                        Ok(v)
                    } else {
                        Err(String::from("Unaligned access"))
                    }
                })
            }

            #[allow(dead_code)]
            fn write_registers(&mut self,
                base: u32, offset: u32, size: u32, value: u32) -> Result<(), String> {
                if size == 4 && (offset & 3) == 0 {
                    self.write_register(base, offset, value)
                } else {
                    Err(String::from("Unaligned access"))
                }
            }
        }

        #(#errors)*
    };

    TokenStream::from(expanded)
}
