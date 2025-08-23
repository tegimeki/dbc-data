//! A derive-macro which produces code to access signals within CAN
//! messages, as described by a `.dbc` file.  The generated code has
//! very few dependencies: just core primitives and `[u8]` slices, and
//! is `#[no_std]` compatible.
//!
//! # Changelog
//! [CHANGELOG.md]
//!
//! # Example
//! Given a `.dbc` file containing:
//!
//! ```text
//! BO_ 1023 SomeMessage: 4 Ecu1
//!  SG_ Unsigned16 : 16|16@0+ (1,0) [0|0] "" Vector__XXX
//!  SG_ Unsigned8 : 8|8@1+ (1,0) [0|0] "" Vector__XXX
//!  SG_ Signed8 : 0|8@1- (1,0) [0|0] "" Vector__XXX
//! ```
//! The following code can be written to access the fields of the
//! message:
//!
//! ```
//! pub use dbc_data::*;
//!
//! #[derive(DbcData, Default)]
//! #[dbc_file = "tests/example.dbc"]
//! struct TestData {
//!     some_message: SomeMessage,
//! }
//!
//! fn test() {
//!     let mut t = TestData::default();
//!
//!     assert_eq!(SomeMessage::ID, 1023);
//!     assert_eq!(SomeMessage::DLC, 4);
//!     assert!(t.some_message.decode(&[0xFE, 0x34, 0x56, 0x78]));
//!     assert_eq!(t.some_message.Signed8, -2);
//!     assert_eq!(t.some_message.Unsigned8, 0x34);
//!     assert_eq!(t.some_message.Unsigned16, 0x5678); // big-endian
//! }
//! ```
//! See the test cases in this crate for examples of usage.
//!
//! # Code Generation
//! This crate is aimed at embedded systems where typically some
//! subset of the messages and signals defined in the `.dbc` file are
//! of interest, and the rest can be ignored for a minimal footpint.
//! If you need to decode the entire DBC into rich (possibly `std`-dependent)
//! types to run on a host system, there are other crates for that
//! such as `dbc_codegen`.
//!
//! ## Messages
//! As `.dbc` files typically contain multiple messages, each of these
//! can be brought into scope by referencing their name as a type
//! (e.g. `SomeMessage` as shown above) and this determines what code
//! is generated.  Messages not referenced will not generate any code.
//!
//! When a range of message IDs contain the same signals, such as a
//! series of readings which do not fit into a single message, then
//! declaring an array will allow that type to be used for all of them.
//!
//! # Signals
//! For cases where only certain signals within a message are needed, the
//! `#[dbc_signals]` attribute lets you specify which ones are used.
//!
//! ## Types
//! Single-bit signals generate `bool` types, and signals with a scale factor
//! generate `f32` types.  All other signals generate signed or unsigned
//! native types which are large enough to fit the contained values, e.g.
//! 13-bit signals will be stored in a `u16` and 17-bit signals will be
//! stored in a `u32`.
//!
//! # Functionality
//! * Decode signals from PDU into native types
//!     * const definitions for `ID: u32`, `DLC: u8`, `EXTENDED: bool`,
//!       and `CYCLE_TIME: usize` when present
//! * Encode signal into PDU (except unaligned BE)
//!
//! # TODO
//! * Encode unaligned BE signals
//! * Generate dispatcher for decoding based on ID (including ranges)
//! * Enforce that arrays of messages contain the same signals
//! * Support multiplexed signals
//! * Emit `enum`s for value-tables, with optional type association
//! * (Maybe) scope generated types to a module
//!
//! # License
//! [LICENSE-MIT]
//!

extern crate proc_macro;
use can_dbc::{
    AttributeValuedForObjectType, ByteOrder, MessageId, Signal, ValueType, DBC,
};
use proc_macro2::TokenStream;
use quote::{quote, TokenStreamExt};
use std::{collections::BTreeMap, fs::read};
use syn::{
    parse_macro_input, spanned::Spanned, Attribute, Data, DeriveInput, Expr,
    Field, Fields, Ident, Lit, Meta, Result, Type,
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
    id: u32,
    extended: bool,
    index: usize,
    ident: &'a Ident,
    attrs: &'a Vec<Attribute>,
    cycle_time: Option<usize>,
}

/// Filter signals based on #[dbc_signals] list
struct SignalFilter {
    names: Vec<String>,
}

impl SignalFilter {
    /// Create a signal filter from a message's attribute
    fn new(message: &MessageInfo) -> Self {
        let mut names: Vec<String> = vec![];
        if let Some(attrs) = parse_attr(message.attrs, "dbc_signals") {
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
            ident: Ident::new(name, message.ident.span()),
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
    fn extract_bits(&self) -> TokenStream {
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
                    #utype::#ext([pdu[#low]])
                },
                16 => quote! {
                    #utype::#ext([pdu[#low],
                                  pdu[#low + 1]])
                },
                32 => quote! {
                    #utype::#ext([pdu[#low + 0],
                                  pdu[#low + 1],
                                  pdu[#low + 2],
                                  pdu[#low + 3]])
                },
                // NOTE: this compiles to very small code and does not
                // involve actually fetching 8 separate bytes; e.g. on
                // armv7 an `ldrd` to get both 32-bit values followed by
                // two `rev` instructions to reverse the bytes.
                64 => quote! {
                    #utype::#ext([pdu[#low + 0],
                                  pdu[#low + 1],
                                  pdu[#low + 2],
                                  pdu[#low + 3],
                                  pdu[#low + 4],
                                  pdu[#low + 5],
                                  pdu[#low + 6],
                                  pdu[#low + 7],
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
                            let v = pdu[#byte] as #utype;
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
                                let v = v | (((pdu[#byte]
                                               & ((1 << #right) - 1))
                                              as #utype) << #shift);
                            });
                        } else {
                            ts.append_all(quote! {
                                let v = v | ((pdu[#byte] as #utype) << #shift);
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
                            let v = pdu[#byte] as #utype;
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
                                ((pdu[#byte] as #utype) >> #shift);
                            });
                            rem = 0;
                        } else {
                            rem -= 8;
                            ts.append_all(quote! {
                                let v = v |
                                ((pdu[#byte] as #utype) << #rem);
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
                self.#name = (pdu[#byte] & (1 << #bit)) != 0;
            }
        } else {
            let value = self.extract_bits();
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

    fn gen_encoder(&self) -> TokenStream {
        let name = &self.ident;
        let low = self.start / 8;
        let mut byte = low;
        let bit = self.start % 8;
        if self.width == 1 {
            // boolean
            quote! {
                let mask: u8 = (1 << #bit);
                if self.#name {
                    pdu[#byte] |= mask;
                } else {
                    pdu[#byte] &= !mask;
                }
            }
        } else {
            let utype = &self.utype;
            let left = self.start % 8;
            // let right = (self.start + self.width) % 8;
            let le = self.signal.byte_order() == &ByteOrder::LittleEndian;

            let mut ts = TokenStream::new();
            if self.is_float() {
                let scale = self.scale;
                let offset = self.signal.offset as f32;
                ts.append_all(quote! {
                    let v = ((self.#name - #offset) / #scale) as #utype;
                });
            } else {
                ts.append_all(quote! {
                    let v = self.#name;
                });
            }
            if le {
                if self.width == self.nwidth && left == 0 {
                    // aligned little-endian
                    let mut bits = self.nwidth;
                    let mut shift = 0;
                    while bits >= 8 {
                        ts.append_all(quote! {
                            pdu[#byte] = ((v >> #shift) as u8) & 0xff;
                        });
                        bits -= 8;
                        byte += 1;
                        shift += 8;
                    }
                } else {
                    // unaligned little-endian
                    let mut rem = self.width;
                    let mut lshift = left;
                    let mut rshift = 0;
                    while rem > 0 {
                        if rem < 8 {
                            let mask: u8 = (1 << rem) - 1;
                            let mask = mask << lshift;
                            ts.append_all(quote! {
                                pdu[#byte] = (pdu[#byte] & !#mask) |
                                ((((v >> #rshift) << (#lshift)) as u8) & #mask);
                            });
                            break;
                        }

                        if lshift != 0 {
                            let mask: u8 = (1 << (8 - left)) - 1;
                            let mask = mask << lshift;
                            ts.append_all(quote! {
                                pdu[#byte] = (pdu[#byte] & !#mask) |
                                ((((v >> #rshift) << (#lshift)) as u8) & #mask);
                            });
                        } else {
                            ts.append_all(quote! {
                                pdu[#byte] = ((v >> #rshift) & 0xff) as u8;
                            });
                        }

                        if byte == low {
                            rem -= 8 - left;
                            rshift += 8 - left;
                        } else {
                            rem -= 8;
                            rshift += 8;
                        }
                        byte += 1;
                        lshift = 0;
                    }
                }
            } else {
                if self.width == self.nwidth && left == 7 {
                    // aligned big-endian
                    let mut bits = self.nwidth;
                    let mut shift = bits - 8;
                    let mut byte = (self.start - 7) / 8;
                    while bits >= 8 {
                        ts.append_all(quote! {
                            pdu[#byte] = ((v >> #shift) as u8) & 0xff;
                        });
                        bits -= 8;
                        byte += 1;
                        if shift >= 8 {
                            shift -= 8;
                        }
                    }
                } else {
                    // unaligned big-endian
                    //                    todo!();
                }
            }
            ts
        }
    }

    fn is_float(&self) -> bool {
        self.scale != 1.0
    }
}

impl<'a> MessageInfo<'a> {
    fn new(dbc: &DBC, field: &'a Field) -> Option<Self> {
        let stype = match &field.ty {
            Type::Path(v) => v,
            Type::Array(a) => match *a.elem {
                // TODO: validate that all signals match in ID range
                Type::Path(ref v) => v,
                _ => unimplemented!(),
            },
            _ => unimplemented!(),
        };
        let ident = &stype.path.segments[0].ident;
        let name = ident.to_string();

        for (index, message) in dbc.messages().iter().enumerate() {
            if message.message_name() == &name {
                let id = message.message_id();
                let (id32, extended) = match *id {
                    MessageId::Standard(id) => (id as u32, false),
                    MessageId::Extended(id) => (id, true),
                };
                let mut cycle_time: Option<usize> = None;
                for attr in dbc.attribute_values().iter() {
                    let value = attr.attribute_value();
                    use AttributeValuedForObjectType as AV;
                    match value {
                        AV::MessageDefinitionAttributeValue(aid, Some(av)) => {
                            if aid == id
                                && attr.attribute_name() == "GenMsgCycleTime"
                            {
                                cycle_time = Some(Self::attr_value(av));
                            }
                        }
                        _ => {}
                    }
                }

                return Some(Self {
                    id: id32,
                    extended,
                    index,
                    ident,
                    cycle_time,
                    attrs: &field.attrs,
                });
            }
        }
        None
    }

    // TODO: revisit this to handle type conversion better; we
    // expect that the value fits in a usize for e.g. GenMsgCycleTime
    fn attr_value(v: &can_dbc::AttributeValue) -> usize {
        use can_dbc::AttributeValue as AV;
        match v {
            AV::AttributeValueU64(x) => *x as usize,
            AV::AttributeValueI64(x) => *x as usize,
            AV::AttributeValueF64(x) => *x as usize,
            AV::AttributeValueCharString(_) => 0usize, // TODO: parse as int?
        }
    }
}

impl<'a> DeriveData<'a> {
    fn from(input: &'a DeriveInput) -> Result<Self> {
        // load the DBC file
        let dbc_file = parse_attr(&input.attrs, "dbc_file")
            .expect("No DBC file specified");
        let contents = read(&dbc_file).expect("Could not read DBC");
        let dbc = match DBC::from_slice(&contents) {
            Ok(dbc) => dbc,
            Err(can_dbc::Error::Incomplete(dbc, _)) => {
                // TODO: emit an actual compiler warning
                eprintln!(
                    "Warning: DBC load incomplete; some data may be missing"
                );
                dbc
            }
            Err(_) => {
                panic!("Unable to parse {dbc_file}");
            }
        };

        // gather all of the messages and associated attributes
        let mut messages: BTreeMap<String, MessageInfo<'_>> =
            Default::default();
        match &input.data {
            Data::Struct(data) => match &data.fields {
                Fields::Named(fields) => {
                    for field in &fields.named {
                        if let Some(info) = MessageInfo::new(&dbc, field) {
                            messages.insert(info.ident.to_string(), info);
                        } else {
                            return Err(syn::Error::new(
                                field.span(),
                                "Unknown message",
                            ));
                        }
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
                .get(message.index)
                .unwrap_or_else(|| panic!("Unknown message {name}"));

            let filter = SignalFilter::new(message);

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

            let id = message.id;
            let extended = message.extended;

            let dlc = *m.message_size() as usize;
            let dlc8 = dlc as u8;
            let ident = message.ident;

            // build signal decoders and encoders
            let mut decoders = TokenStream::new();
            let mut encoders = TokenStream::new();
            for info in infos.iter() {
                decoders.append_all(info.gen_decoder());
                encoders.append_all(info.gen_encoder());
            }
            let cycle_time = if let Some(c) = message.cycle_time {
                quote! {
                    const CYCLE_TIME: usize = #c;
                }
            } else {
                quote! {}
            };

            out.append_all(quote! {
                #[allow(dead_code)]
                #[allow(non_snake_case)]
                #[allow(non_camel_case_types)]
                #[derive(Default)]
                pub struct #ident {
                    #(
                        pub #signals: #types
                    ),*
                }

                impl #ident {
                    const ID: u32 = #id;
                    const DLC: u8 = #dlc8;
                    const EXTENDED: bool = #extended;
                    #cycle_time

                    pub fn decode(&mut self, pdu: &[u8])
                                  -> bool {
                        if pdu.len() != #dlc {
                            return false
                        }
                        #decoders
                        true
                    }

                    pub fn encode(&mut self, pdu: &mut [u8])
                                  -> bool {
                        if pdu.len() != #dlc {
                            return false
                        }
                        #encoders
                        true
                    }
                }

                impl TryFrom<&[u8]> for #ident {
                    type Error = ();
                    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
                        let mut pdu = Self::default(); // TODO: elide
                        if pdu.decode(data) {
                            Ok(pdu)
                        } else {
                            Err(())
                        }
                    }
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
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

fn derive_data(input: &DeriveInput) -> Result<TokenStream> {
    Ok(DeriveData::from(input)?.build())
}

fn parse_attr(attrs: &[Attribute], name: &str) -> Option<String> {
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
