//! Signal information and codegen

use crate::MessageInfo;
use can_dbc::{ByteOrder, Signal, ValueType};
use proc_macro2::TokenStream;
use quote::{quote, TokenStreamExt};
use syn::{parse_quote, Expr, Ident};

/// Information about signal within message
pub struct SignalInfo<'a> {
    /// The DBC signal reference
    pub signal: &'a Signal,
    /// Our source identifier
    pub ident: Ident,
    /// The native type identifier
    pub ntype: Ident,
    /// The unsigned type used for encoding/decoding
    pub utype: Ident,
    /// The start bit of the signal within the PDU
    pub start: usize,
    /// The width (in bits) of the signal
    pub width: usize,
    /// The native width of the type containing the signal
    pub nwidth: usize,
    /// The scale-factor for the signal
    pub scale: f32,
    /// Indicates signed v.s. unsigned signal
    pub signed: bool,
}

impl<'a> SignalInfo<'a> {
    /// Create signal information
    pub fn new(signal: &'a Signal, message: &MessageInfo) -> Self {
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

    /// Produce an identifier for the DBC f64 value
    pub fn const_ident(&self, v: f64) -> Expr {
        if self.is_float() {
            let v = v as f32;
            parse_quote!(#v)
        } else if self.width == 1 {
            let b = v != 0.0;
            parse_quote!(#b)
        } else {
            let v = v as usize;
            let t = self.ntype.clone();
            // TODO: make this less verbose and use type directly
            parse_quote!(#v as #t)
        }
    }

    /// Generate the code for extracting signal bits
    fn extract_bits(&self) -> TokenStream {
        let same_width = self.width == self.nwidth;
        let le = self.signal.byte_order() == &ByteOrder::LittleEndian;
        let bit_aligned = if le {
            (self.start % 8) == 0
        } else {
            (self.start % 8) == 7
        };

        if same_width && bit_aligned {
            self.extract_aligned(le)
        } else if le {
            self.extract_unaligned_le()
        } else {
            self.extract_unaligned_be()
        }
    }

    /// Code generation for aligned signal bits
    fn extract_aligned(&self, le: bool) -> TokenStream {
        let low = self.start / 8;
        let utype = &self.utype;
        let mut ts = TokenStream::new();

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
        quote! { { #ts } }
    }

    fn extract_unaligned_le(&self) -> TokenStream {
        let low = self.start / 8;
        let left = self.start % 8;
        let high = (self.start + self.width - 1) / 8;
        let right = (self.start + self.width) % 8;
        let utype = &self.utype;

        let mut ts = TokenStream::new();
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
                        let width = self.width;
                        ts.append_all(quote! {
                            let v = (v >> #left) & ((1 << #width) - 1);
                        });
                    } else {
                        ts.append_all(quote! {
                            let v = v >> #left;
                        });
                    }
                } else {
                    let rem = self.width;
                    ts.append_all(quote! {
                        let v = v & ((1 << #rem) -1);
                    });
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

        quote! { { #ts } }
    }

    fn extract_unaligned_be(&self) -> TokenStream {
        let low = self.start / 8;
        let left = self.start % 8;
        let utype = &self.utype;

        let mut ts = TokenStream::new();

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
            } else if rem < 8 {
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

        quote! { { #ts } }
    }

    /// Generate a signal's decoder
    pub fn gen_decoder(&self) -> TokenStream {
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
            if self.is_float() {
                let scale = self.scale;
                let offset = *self.signal.offset() as f32;
                quote! {
                    self.#name = ((#value as f32) * #scale) + #offset;
                }
            } else {
                quote! {
                    self.#name = #value as #ntype;
                }
            }
        }
    }

    /// Generate code for encoding a signal value
    pub fn gen_encoder(&self) -> TokenStream {
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
            } else if self.width == self.nwidth && left == 7 {
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
            ts
        }
    }

    /// We consider any signal with a scale to be a floating-point
    /// value
    pub fn is_float(&self) -> bool {
        self.scale != 1.0
    }
}
