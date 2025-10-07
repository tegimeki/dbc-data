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
//! An `enum` can also be used to derive the types for signals
//! and messages.
//!
//! See the test cases in this crate for examples of usage.
//!
//! # Code Generation
//! This crate is aimed at embedded systems where typically some
//! subset of the messages and signals defined in the `.dbc` file are
//! of interest, and the rest can be ignored for a minimal footpint.
//! If you need to decode the entire DBC into rich (possibly
//! `std`-dependent) types to run on a host system, there are other
//! crates for that such as `dbc_codegen`.
//!
//! ## Messages
//! As `.dbc` files typically contain multiple messages, each of these
//! can be brought into scope by referencing their name as a type
//! (e.g. `SomeMessage` as shown above) and this determines what code
//! is generated.  Messages not referenced will not generate any code.
//!
//! When a range of message IDs contain the same signals, such as a
//! series of readings which do not fit into a single message, then
//! declaring an array will allow that type to be used for all of
//! them.
//!
//! # Signals
//! For cases where only certain signals within a message are needed,
//! the `#[dbc_signals]` attribute lets you specify which ones are
//! used.
//!
//! ## Types
//! Single-bit signals generate `bool` types, and signals with a scale
//! factor generate `f32` types.  All other signals generate signed or
//! unsigned native types which are large enough to fit the contained
//! values, e.g.  13-bit signals will be stored in a `u16` and 17-bit
//! signals will be stored in a `u32`.
//!
//! # Usage
//! As DBC message names tend to follow different conventions from Rust
//! code, it can be helpful to wrap them in newtype declarations.
//! Additionally, it is often desirable to scope these identifiers away
//! from application code by using a private module:
//!
//! ```ignore
//! mod private {
//!     use dbc_data::DbcData;
//!     #[derive(DbcData)]
//!     // (struct with DBC messages, e.g. some_Message_NAME)
//! }
//!
//! pub type SomeMessageName = private::some_Message_NAME;
//!
//! ```
//!
//! The application uses this wrapped type without exposure to the
//! DBC-centric naming.  The wrapped types can have their own `impl`
//! block(s) to extend functionality, if desired.  Functions which
//! perform operations on signals, define new constants, etc. can be
//! added in such blocks.  The application can access signal fields
//! directly from the underlying type and/or use the wrapped
//! interfaces.
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
//!
//! # License
//! [LICENSE-MIT]
//!

extern crate proc_macro;

mod derive;
mod message;
mod signal;

use proc_macro2::TokenStream;
use syn::{parse_macro_input, Attribute, DeriveInput, Expr, Lit, Meta, Result};

use derive::DeriveData;
use message::MessageInfo;

/// See the crate documentation for details.
///
/// The `#[dbc_file]` attribute specifies the name of the .dbc file
/// to use, and is required.
///
/// Individual messages may specify a `#[dbc_signals]` attribute
/// naming the individual signals of interest; otherwise, all
/// signals within the message are generated.
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
    let attr = attrs.iter().find(|a| {
        a.path().segments.len() == 1 && a.path().segments[0].ident == name
    })?;

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
