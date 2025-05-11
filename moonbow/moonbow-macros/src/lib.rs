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

#[proc_macro_derive(Peripheral, attributes(register))]
pub fn peripheral(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    impl_peripheral(&input)
}

fn get_ident(path: &syn::Path) -> Result<&syn::Ident, TokenStream> {
    path.get_ident().ok_or_else(|| {
        quote_spanned! {
            path.span() => compile_error!("Expected identifier");
        }.into()
    })
}

fn ensure_none(expr: Option<syn::Expr>) -> Result<(), TokenStream> {
    match expr {
        None => Ok(()),
        _ => {
            Err(quote_spanned! {
                expr.span() => compile_error!("Unexpected value");
            }.into())
        }
    }
}

fn get_int(name: &syn::Ident, expr: Option<syn::Expr>) -> Result<u32, TokenStream> {
    let expr = expr.ok_or_else(|| {
        Into::<TokenStream>::into(quote_spanned! {
            name.span() => compile_error!("Expected integer value");
        })
    })?;
    match expr {
        syn::Expr::Lit(syn::ExprLit{lit: syn::Lit::Int(i), ..}) => {
            let value = i.base10_parse::<u32>().or_else(|e| Err(e.to_compile_error()))?;
            Ok(value)
        }
        _ => {
            Err(quote_spanned! {
                expr.span() => compile_error!("Expected integer literal");
            }.into())
        }
    }
}

#[derive(Default)]
struct RegisterSettings {
    offset: Option<u32>,
    reset: u32,
    read_const: Option<u32>,
    write_nop: bool,
}

fn process_register_attr(attr: &syn::Attribute, rs: &mut RegisterSettings) -> Result<(), TokenStream> {
    match &attr.meta {
        syn::Meta::List(arglist) => {
            let args = arglist.parse_args_with(
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated
            ).unwrap();

            for arg in args {
                let name_value: (syn::Ident, Option<syn::Expr>) = match arg {
                    syn::Meta::Path(p) => {
                        (get_ident(&p)?.clone(), None)
                    }
                    syn::Meta::NameValue(nv) => {
                        let n = get_ident(&nv.path)?;
                        (n.clone(), Some(nv.value))
                    }
                    syn::Meta::List(l) => {
                        return Err(quote_spanned! {
                            l.delimiter.span().join() => compile_error!("Unexpected argument");
                        }.into());
                    }
                };
                match name_value {
                    (n, v) if n == "offset" => {
                        let off = get_int(&n, v.clone())?;
                        if (off & 3) != 0 {
                            return Err(quote_spanned! {
                                v.span() => compile_error!("Offset must be on word boundary");
                            }.into());
                        }
                        rs.offset = Some(off);
                    }
                    (n, v) if n == "read_const" => {
                        rs.read_const = Some(get_int(&n, v)?);
                    }
                    (n, v) if n == "reset" => {
                        rs.reset = get_int(&n, v)?;
                    }
                    (n, v) if n == "write_nop" => {
                        ensure_none(v)?;
                        rs.write_nop = true;
                    }
                    (n, _) => {
                        return Err(quote_spanned! {
                            n.span() => compile_error!("Unknown argument");
                        }.into());
                    }
                };
            }
        }
        _ => {
            // bare attribute, no arguments
        }
    }

    Ok(())
}

fn impl_peripheral(input: &DeriveInput) -> TokenStream {
    let name = &input.ident;
    //let namestr = name.to_string();

    let mut offset: u32 = 0;

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

            let mut rs = RegisterSettings::default();
            for attr in &f.attrs {
                if attr.path().is_ident("register") {
                    process_register_attr(attr, &mut rs)?;
                }
            }

            if let Some(new_offset) = rs.offset {
                offset = new_offset;
            }

            let r = Register {
                name: f.ident.clone().unwrap(),
                ty: f.ty.clone(),
                offset,
                reset: rs.reset,
                read_const: rs.read_const,
                write_nop: rs.write_nop,
            };
            offset += 4;
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
