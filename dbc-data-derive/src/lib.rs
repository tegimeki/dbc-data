//! DBC data derive macro
extern crate proc_macro;
use can_dbc::{MessageId, ValueType, DBC};
use proc_macro2::TokenStream;
use quote::{quote, TokenStreamExt};
use std::{collections::BTreeMap, fs::read};
use syn::{
    parse_macro_input, Attribute, Data, DeriveInput, Expr, Fields, Ident, Lit,
    Meta, Result, Type,
};

//#[derive(Debug)]
struct DeriveData<'a> {
    /// Name of the struct we are deriving for
    #[allow(dead_code)]
    name: &'a Ident,
    /// The parsed DBC file
    dbc: can_dbc::DBC,
    /// Message info
    messages: BTreeMap<String, MessageInfo<'a>>,
}

struct MessageInfo<'a> {
    ident: &'a Ident,
    attrs: &'a Vec<Attribute>,
}

fn parse_attr(attrs: &Vec<Attribute>, name: &str) -> Option<String> {
    let attr = attrs
        .iter()
        .filter(|a| {
            a.path().segments.len() == 1 && a.path().segments[0].ident == name
        })
        .nth(0)?;

    let expr = match &attr.meta {
        Meta::NameValue(n) => Some(&n.value),
        _ => None,
    };

    match &expr {
        Some(Expr::Lit(e)) => match &e.lit {
            Lit::Str(s) => Some(s.value()),
            _ => None,
        },
        _ => None,
    }
}

impl<'a> DeriveData<'a> {
    fn from(input: &'a DeriveInput) -> Result<Self> {
        // load the DBC file
        let dbc_file = parse_attr(&input.attrs, "dbc_file")
            .expect("No DBC file specified");
        let contents = read(&dbc_file).expect("Could not read DBC");
        let dbc = DBC::from_slice(&contents).expect("Could not parse DBC");

        // gather all of the messages and associated attributes
        let mut messages: BTreeMap<String, MessageInfo<'_>> =
            Default::default();
        match &input.data {
            Data::Struct(data) => match &data.fields {
                Fields::Named(fields) => {
                    for field in &fields.named {
                        let stype = match &field.ty {
                            Type::Path(v) => v,
                            _ => unimplemented!(),
                        };
                        let ident = &stype.path.segments[0].ident;
                        messages.insert(
                            ident.to_string(),
                            MessageInfo {
                                ident,
                                attrs: &field.attrs,
                            },
                        );
                    }
                }
                Fields::Unnamed(_) | Fields::Unit => unimplemented!(),
            },
            _ => unimplemented!(),
        }

        Ok(Self {
            name: &input.ident,
            dbc,
            messages,
        })
    }

    fn build(self) -> TokenStream {
        let mut out = TokenStream::new();

        for (name, message) in self.messages.iter() {
            let m = self
                .dbc
                .messages()
                .iter()
                .find(|m| *m.message_name() == *name)
                .expect(&format!("Unknown message {name}"));

            if let Some(attrs) = parse_attr(&message.attrs, "dbc_signals") {
                println!("{:#?}", attrs);
                // TODO: only add the signals in the attr list
            }

            let mut signals: Vec<Ident> = vec![];
            let mut types: Vec<Ident> = vec![];
            for signal in m.signals().iter() {
                // TODO: filter-out signals not in the #[dbc_signals] list
                signals.push(Ident::new(signal.name(), message.ident.span()));
                let vt = signal.value_type();

                let t = match signal.signal_size() {
                    1 => "bool",
                    2..=8 => match vt {
                        ValueType::Signed => "i8",
                        ValueType::Unsigned => "u8",
                    },
                    9..=16 => match vt {
                        ValueType::Signed => "i16",
                        ValueType::Unsigned => "u16",
                    },
                    17..=32 => match vt {
                        ValueType::Signed => "i32",
                        ValueType::Unsigned => "u32",
                    },
                    _ => match vt {
                        ValueType::Signed => "i64",
                        ValueType::Unsigned => "u64",
                    },
                };
                types.push(syn::Ident::new(t, message.ident.span()));
            }

            let (id, extended) = match *m.message_id() {
                MessageId::Standard(id) => (id as u32, false),
                MessageId::Extended(id) => (id, true),
            };

            let dlc = *m.message_size() as u8;
            let ident = message.ident;

            out.append_all(quote! {
                #[allow(dead_code)]
                #[allow(non_snake_case)]
                #[derive(Default)]
                struct #ident {
                    #(#signals: #types),*
                }
                impl #ident {
                    const ID: u32 = #id;
                    const DLC: u8 = #dlc;
                    const EXTENDED: bool = #extended;
                }
            });
        }
        out
    }
}

#[proc_macro_derive(DbcData, attributes(dbc_file, dbc_signals))]
pub fn dbc_data_derive(
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    derive_data(&parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(|err| err.to_compile_error().into())
        .into()
}

fn derive_data(input: &DeriveInput) -> Result<TokenStream> {
    Ok(DeriveData::from(input)?.build())
}
