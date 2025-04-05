//! DBC data derive macro
extern crate proc_macro;
use can_dbc::{ByteOrder, MessageId, Signal, ValueType, DBC};
use proc_macro2::TokenStream;
use quote::{quote, TokenStreamExt};
use std::{collections::BTreeMap, fs::read};
use syn::{
    parse_macro_input, Attribute, Data, DeriveInput, Expr, Fields, Ident, Lit,
    Meta, Result, Type,
};

struct DeriveData<'a> {
    /// Name of the struct we are deriving for
    #[allow(dead_code)]
    name: &'a Ident,
    /// The parsed DBC file
    dbc: can_dbc::DBC,
    /// All of the messages to derive
    messages: BTreeMap<String, MessageInfo<'a>>,
}

struct MessageInfo<'a> {
    ident: &'a Ident,
    attrs: &'a Vec<Attribute>,
}

/// Filter signals based on #[dbc_signals] list
struct SignalFilter {
    names: Vec<String>,
}

impl SignalFilter {
    /// Create a signal filter from a message's attribute
    fn new(message: &MessageInfo) -> Self {
        let mut names: Vec<String> = vec![];
        if let Some(attrs) = parse_attr(&message.attrs, "dbc_signals") {
            let list = attrs.split(",");
            for name in list {
                let name = name.trim();
                names.push(name.to_string());
            }
        }
        Self { names }
    }

    /// Return whether a signal should be used, i.e. whether it is
    /// in the filter list or the list is empty
    fn use_signal(&self, name: impl Into<String>) -> bool {
        if self.names.is_empty() {
            return true;
        }
        let name = name.into();
        self.names.contains(&name)
    }
}

/// Information about signal within message
struct SignalInfo<'a> {
    signal: &'a Signal,
    ident: Ident,
    ntype: Ident,
    start: usize,
    width: usize,
    nwidth: usize,
    scale: f32,
    signed: bool,
}

impl<'a> SignalInfo<'a> {
    fn new(signal: &'a Signal, message: &MessageInfo) -> Self {
        // TODO: sanitize and/or change name format
        let name = signal.name();
        let signed = matches!(signal.value_type(), ValueType::Signed);
        let width = *signal.signal_size() as usize;
        let scale = *signal.factor() as f32;

        // get native width of signal data
        let nwidth = match width {
            1 => 1,
            2..=8 => 8,
            9..=16 => 16,
            17..=32 => 32,
            _ => 64,
        };

        // get native type for signal
        let t = if scale == 1.0 {
            if width == 1 {
                "bool"
            } else {
                &format!("{}{}", if signed { "i" } else { "u" }, nwidth)
            }
        } else {
            "f32"
        };

        Self {
            signal,
            ident: Ident::new(&name, message.ident.span()),
            ntype: Ident::new(t, message.ident.span()),
            start: *signal.start_bit() as usize,
            scale,
            signed,
            width,
            nwidth,
        }
    }

    /// Generate the code for extracting signal bits into a
    /// typed value
    fn gen_bits(&self) -> TokenStream {
        let byte = self.start / 8;
        let left = self.start % 8;
        let _right = (self.start + self.width) % 8;
        let ntype = &self.ntype;
        let le = self.signal.byte_order() == &ByteOrder::LittleEndian;
        let ext = if le {
            Ident::new("from_le_bytes", ntype.span())
        } else {
            Ident::new("from_be_bytes", ntype.span())
        };
        if self.width == self.nwidth && left == 0 {
            // aligned
            match self.width {
                8 => quote! {
                    #ntype::#ext([self.pdu[#byte]]);
                },
                16 => quote! {
                    #ntype::#ext([self.pdu[#byte], self.pdu[#byte + 1]]);
                },
                32 => quote! {
                    #ntype::#ext([self.pdu[#byte + 0],
                                          self.pdu[#byte + 1],
                                          self.pdu[#byte + 2],
                                          self.pdu[#byte + 3]]);
                },
                64 => quote! {
                    #ntype::#ext([self.pdu[#byte + 0],
                                          self.pdu[#byte + 1],
                                          self.pdu[#byte + 2],
                                          self.pdu[#byte + 3],
                                          self.pdu[#byte + 4],
                                          self.pdu[#byte + 5],
                                          self.pdu[#byte + 6],
                                          self.pdu[#byte + 7],
                    ]);
                },
                _ => unimplemented!(),
            }
        } else {
            // unaligned / uneven
            let _ = self.signed;
            todo!();
        }
    }

    fn gen_decoder(&self) -> TokenStream {
        let name = &self.ident;
        if self.width == 1 {
            // boolean
            let byte = self.start / 8;
            let bit = self.start % 8;
            quote! {
                self.#name = (self.pdu[#byte] & (1 << #bit)) != 0;
            }
        } else if !self.is_float() {
            let value = self.gen_bits();
            quote! {
                self.#name = #value;
            }
        } else {
            TokenStream::new()
        }
    }

    fn is_float(&self) -> bool {
        self.scale != 1.0
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

            let filter = SignalFilter::new(&message);

            let mut signals: Vec<Ident> = vec![];
            let mut types: Vec<Ident> = vec![];
            let mut infos: Vec<SignalInfo> = vec![];
            for s in m.signals().iter() {
                if !filter.use_signal(s.name()) {
                    continue;
                }

                let signal = SignalInfo::new(s, message);
                signals.push(signal.ident.clone());
                types.push(signal.ntype.clone());
                infos.push(signal);
            }

            let (id, extended) = match *m.message_id() {
                MessageId::Standard(id) => (id as u32, false),
                MessageId::Extended(id) => (id, true),
            };

            let dlc = *m.message_size() as usize;
            let dlc8 = dlc as u8;
            let ident = message.ident;

            // build signal decoders
            let mut decoders = TokenStream::new();
            for info in infos.iter() {
                decoders.append_all(info.gen_decoder());
            }

            out.append_all(quote! {
                #[allow(dead_code)]
                #[allow(non_snake_case)]
                #[derive(Default)]
                pub struct #ident {
                    /// The message payload data
                    pub pdu: [u8; #dlc],
                    #(
                        pub #signals: #types
                    ),*
                }

                impl #ident {
                    const ID: u32 = #id;
                    const DLC: u8 = #dlc8;
                    const EXTENDED: bool = #extended;

                    pub fn decode(&mut self, data: &[u8])
                                  -> Result<(), DecodeError> {
                        if data.len() != #dlc {
                            return Err(DecodeError::InvalidDlc);
                        }
                        self.pdu.copy_from_slice(&data[..#dlc]);
                        #decoders
                        Ok(())
                    }
                }
            });
        }
        out
    }
}

/// TODO: add docs for derive macro
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
