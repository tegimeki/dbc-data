//! A derive-macro which produces code to access signals within CAN
//! messages, as described by a `.dbc` file.  The generated code has
//! very few dependencies, limited to core primitives and `[u8]`
//! slices, and is `#[no_std]` compatible.
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
//!     assert!(t.some_message.decode(&[0x12, 0x34, 0x56, 0x78]).is_ok());
//!     assert_eq!(t.some_message.Signed8, 0x12);
//!     assert_eq!(t.some_message.Unsigned8, 0x34);
//!     assert_eq!(t.some_message.Unsigned16, 0x5678); // big-endian
//! }
//! ```
//!
//! As `.dbc` files may contain multiple messages, each of these can be
//! brought into scope by referencing their name as a type (e.g. `SomeMessage`
//! as shown above) and this determines what code is generated.  Messages
//! not referenced will not generate any code.
//!
//! For cases where only certain signals within a message are needed, the
//! `#[dbc_signals]` attribute lets you specify which ones are used.
//!
//! See the test cases in this crate for examples of usage.
//!
//! # TODOs
//! - [ ] support unaligned signals
//! - [ ] support `f32` types when offset/scale are present
//! - [ ] consider scoping generated types to a module
//!
pub use dbc_data_derive::*;

/// Decode error type
pub enum DecodeError {
    /// The CAN ID is not known from the messages imported from the DBC
    UnknownId,
    /// The DLC (data length) is invalid for the message
    InvalidDlc,
}

#[cfg(test)]
mod test {
    use super::*;

    #[allow(dead_code)]
    #[derive(DbcData, Default)]
    #[dbc_file = "tests/test.dbc"]
    struct Test {
        aligned: AlignedMessage,
        #[dbc_signals = "Bool_A, Bool_H"]
        misc: MiscMessage,
        sixty_four_le: SixtyFourBitLE,
        sixty_four_be: SixtyFourBitBE,
        sixty_four_signed: SixtyFourBitSigned,
    }

    #[test]
    fn basic() {
        let mut t = Test::default();

        // invalid length
        assert!(t.aligned.decode(&[0x00]).is_err());

        // message ID, DLC constants
        assert_eq!(AlignedMessage::ID, 1023);
        assert_eq!(AlignedMessage::DLC, 8);
        assert_eq!(MiscMessage::ID, 8191);
        assert_eq!(MiscMessage::DLC, 2);
    }

    #[test]
    fn aligned() {
        let mut t = Test::default();

        // various aligned 8/16-bit values
        assert!(t
            .aligned
            .decode(&[0xAA, 0x55, 0x01, 0x00, 0x34, 0x56, 0x78, 0x9A])
            .is_ok());
        assert_eq!(t.aligned.Unsigned8, 0x55);
        assert_eq!(t.aligned.Signed8, -86); // 0xAA as i8
        assert_eq!(t.aligned.Unsigned16, 256); // 0x0100 big-endian
    }

    #[test]
    fn misc() {
        let mut t = Test::default();

        // booleans
        assert!(t.misc.decode(&[0x82, 0x20]).is_ok());
        assert!(!t.misc.Bool_A);
        assert!(t.misc.Bool_H);
    }

    #[test]
    fn sixty_four_bit() {
        let mut t = Test::default();

        // 64-bit unsigned little-endian
        assert!(t
            .sixty_four_le
            .decode(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88])
            .is_ok());

        assert_eq!(t.sixty_four_le.SixtyFour, 0x8877665544332211);

        // 64-bit unsigned big-endian
        assert!(t
            .sixty_four_be
            .decode(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88])
            .is_ok());

        assert_eq!(t.sixty_four_be.SixtyFour, 0x1122334455667788);

        // 64-bit signed little-endian
        assert!(t
            .sixty_four_signed
            .decode(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88])
            .is_ok());

        assert_eq!(t.sixty_four_signed.SixtyFour, -8613303245920329199);
    }

    #[test]
    fn extract() {
        let data: [u8; 1] = [0x87u8];
        let value = i8::from_le_bytes(data);
        assert_eq!(value, -121);
    }
}
