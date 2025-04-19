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
    utype: Ident,
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

        // get storage width of signal data
        let nwidth = match width {
            1 => 1,
            2..=8 => 8,
            9..=16 => 16,
            17..=32 => 32,
            _ => 64,
        };

        let utype = if width == 1 {
            "bool"
        } else {
            &format!("{}{}", if signed { "i" } else { "u" }, nwidth)
        };

        // get native type for signal
        let ntype = if scale == 1.0 { utype } else { "f32" };

        Self {
            signal,
            ident: Ident::new(&name, message.ident.span()),
            ntype: Ident::new(ntype, message.ident.span()),
            utype: Ident::new(utype, message.ident.span()),
            start: *signal.start_bit() as usize,
            scale,
            signed,
            width,
            nwidth,
        }
    }

    /// Generate the code for extracting signal bits
    fn gen_bits(&self) -> TokenStream {
        let low = self.start / 8;
        let left = self.start % 8;
        let high = (self.start + self.width - 1) / 8;
        let right = (self.start + self.width) % 8;
        let utype = &self.utype;
        let le = self.signal.byte_order() == &ByteOrder::LittleEndian;

        let mut ts = TokenStream::new();
        if self.width == self.nwidth && left == 0 {
            // aligned
            let ext = if le {
                Ident::new("from_le_bytes", utype.span())
            } else {
                Ident::new("from_be_bytes", utype.span())
            };
            let tokens = match self.width {
                8 => quote! {
                    #utype::#ext([self.pdu[#low]])
                },
                16 => quote! {
                    #utype::#ext([self.pdu[#low],
                                  self.pdu[#low + 1]])
                },
                32 => quote! {
                    #utype::#ext([self.pdu[#low + 0],
                                  self.pdu[#low + 1],
                                  self.pdu[#low + 2],
                                  self.pdu[#low + 3]])
                },
                // NOTE: this compiles to very small code and does not
                // involve actually fetching 8 separate bytes; e.g. on
                // armv7 an `ldrd` to get both 32-bit values followed by
                // two `rev` instructions to reverse the bytes.
                64 => quote! {
                    #utype::#ext([self.pdu[#low + 0],
                                  self.pdu[#low + 1],
                                  self.pdu[#low + 2],
                                  self.pdu[#low + 3],
                                  self.pdu[#low + 4],
                                  self.pdu[#low + 5],
                                  self.pdu[#low + 6],
                                  self.pdu[#low + 7],
                    ])
                },
                _ => unimplemented!(),
            };
            ts.append_all(tokens);
        } else {
            if le {
                let count = high - low;
                for o in 0..=count {
                    let byte = low + o;
                    if o == 0 {
                        // first byte
                        ts.append_all(quote! {
                            let v = self.pdu[#byte] as #utype;
                        });
                        if left != 0 {
                            if count == 0 {
                                ts.append_all(quote! {
                                    let v = (v >> #left) & ((1 << #left) - 1);
                                });
                            } else {
                                ts.append_all(quote! {
                                    let v = v >> #left;
                                });
                            }
                        }
                    } else {
                        let shift = (o * 8) - left;
                        if o == count && right != 0 {
                            ts.append_all(quote! {
                                let v = v | (((self.pdu[#byte]
                                               & ((1 << #right) - 1))
                                              as #utype) << #shift);
                            });
                        } else {
                            ts.append_all(quote! {
                            let v = v | ((self.pdu[#byte] as #utype) << #shift);
                        });
                        }
                    }
                }
            } else {
                // big-endian
                let mut rem = self.width;
                let mut byte = low;
                while rem > 0 {
                    if byte == low {
                        // first byte
                        ts.append_all(quote! {
                            let v = self.pdu[#byte] as #utype;
                        });
                        if rem < 8 {
                            // single byte
                            let mask = rem - 1;
                            let shift = left + 1 - rem;
                            ts.append_all(quote! {
                                let mask: #utype = (1 << #mask)
                                    | ((1 << #mask) - 1);
                                let v = (v >> #shift) & mask;
                            });
                            rem = 0;
                        } else {
                            // first of multiple bytes
                            let mask = left;
                            let shift = rem - left - 1;
                            if mask < 7 {
                                ts.append_all(quote! {
                                    let mask: #utype = (1 << #mask)
                                        | ((1 << #mask) - 1);
                                    let v = (v & mask) << #shift;
                                });
                            } else {
                                ts.append_all(quote! {
                                    let v = v << #shift;
                                });
                            }
                            rem -= left + 1;
                        }
                        byte += 1;
                    } else {
                        if rem < 8 {
                            // last byte: take top bits
                            let shift = 8 - rem;
                            ts.append_all(quote! {
                                let v = v |
                                ((self.pdu[#byte] as #utype) >> #shift);
                            });
                            rem = 0;
                        } else {
                            rem -= 8;
                            ts.append_all(quote! {
                                let v = v |
                                ((self.pdu[#byte] as #utype) << #rem);
                            });
                            byte += 1;
                        }
                    };
                }
            }
            // perform sign-extension for values with fewer bits than
            // the storage type
            if self.signed && self.width < self.nwidth {
                let mask = self.width - 1;
                ts.append_all(quote! {
                    let mask: #utype = (1 << #mask);
                    let v = if (v & mask) != 0 {
                        let mask = mask | (mask - 1);
                        v | !mask
                    } else {
                        v
                    };
                });
            }
            ts.append_all(quote! { v });
        }
        quote! { { #ts } }
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
        } else {
            let value = self.gen_bits();
            let ntype = &self.ntype;
            if !self.is_float() {
                quote! {
                    self.#name = #value as #ntype;
                }
            } else {
                let scale = self.scale;
                let offset = *self.signal.offset() as f32;
                quote! {
                    self.#name = ((#value as f32) * #scale) + #offset;
                }
            }
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
