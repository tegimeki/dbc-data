//! A derive-macro which produces code to access signals within CAN
//! messages, as described by a `.dbc` file.  The generated code has
//! very few dependencies: just core primitives and `[u8]` slices, and
//! is `#[no_std]` compatible.
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
//! # Functionality
//! * [x] decode signals from PDU
//! * [ ] encode signals into PDU
//! - [ ] generate dispatcher for decoding based on ID
//! - [ ] support multiplexed signals
//! - [ ] consider scoping generated types to a module
//!
//! # License
//! (MIT)[/LICENSE-MIT)
//!
#![no_std]
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
    use assert_hex::assert_eq_hex;

    #[allow(dead_code)]
    #[derive(DbcData, Default)]
    #[dbc_file = "tests/test.dbc"]
    struct Test {
        aligned_ule: AlignedUnsignedLE,
        unaligned_ule: UnalignedUnsignedLE,
        unaligned_ube: UnalignedUnsignedBE,
        unaligned_sle: UnalignedSignedLE,
        unaligned_sbe: UnalignedSignedBE,
        #[dbc_signals = "Bool_A, Bool_H, Float_A"]
        misc: MiscMessage,
        sixty_four_le: SixtyFourBitLE,
        sixty_four_be: SixtyFourBitBE,
        sixty_four_signed: SixtyFourBitSigned,
    }

    #[test]
    fn basic() {
        let mut t = Test::default();

        // invalid length
        assert!(t.aligned_ule.decode(&[0x00]).is_err());

        // message ID, DLC constants
        assert_eq!(AlignedUnsignedLE::ID, 1023);
        assert_eq!(AlignedUnsignedLE::DLC, 8);
        assert_eq!(MiscMessage::ID, 8191);
        assert_eq!(MiscMessage::DLC, 2);
    }

    #[test]
    fn aligned_unsigned_le() {
        let mut t = Test::default();

        assert!(t
            .aligned_ule
            .decode(&[0xAA, 0x55, 0x01, 0x20, 0x34, 0x56, 0x78, 0x9A])
            .is_ok());
        assert_eq_hex!(t.aligned_ule.Unsigned8, 0x55);
        assert_eq_hex!(t.aligned_ule.Unsigned16, 0x2001);
    }

    #[test]
    fn unaligned_unsigned_le() {
        let mut t = Test::default();

        // various unaligned values
        assert!(t
            .unaligned_ule
            .decode(&[0xF7, 0x70, 0x20, 0x31, 0xf0, 0xa1, 0x73, 0xfd])
            .is_ok());
        assert_eq_hex!(t.unaligned_ule.Unsigned15, 0x2E74);
        assert_eq_hex!(t.unaligned_ule.Unsigned23, 0x7C0C48);
        assert_eq_hex!(t.unaligned_ule.Unsigned3, 6u8);
    }

    #[test]
    fn unaligned_unsigned_be() {
        let mut t = Test::default();

        // various unaligned values
        assert!(t
            .unaligned_ube
            .decode(&[0xfd, 0xe5, 0xa1, 0xf0, 0x31, 0xf8, 0x70, 0x77])
            .is_ok());
        assert_eq_hex!(t.unaligned_ube.Unsigned3, 2u8);
        assert_eq_hex!(t.unaligned_ube.Unsigned15, 0x4383);
        assert_eq_hex!(t.unaligned_ube.Unsigned23, 0x1F031F);
    }

    #[test]
    fn unaligned_signed_le() {
        let mut t = Test::default();

        // various unaligned values
        assert!(t
            .unaligned_sle
            .decode(&[0xF7, 0x70, 0x20, 0x31, 0xf0, 0xa1, 0x73, 0xfd])
            .is_ok());
        assert_eq_hex!(t.unaligned_sle.Signed15, 0x2E74);
        assert_eq_hex!(t.unaligned_sle.Signed23, 0xFFFC0C48u32 as i32);
        assert_eq!(t.unaligned_sle.Signed3, -2);
    }

    #[test]
    fn unaligned_signed_be() {
        let mut t = Test::default();

        // various unaligned values
        assert!(t
            .unaligned_sbe
            .decode(&[0xfd, 0xe5, 0xa1, 0xf0, 0x31, 0xf8, 0x70, 0x77])
            .is_ok());
        assert_eq_hex!(t.unaligned_sbe.Signed3, 2);
        assert_eq_hex!(t.unaligned_sbe.Signed15, 0xC383u16 as i16);
        assert_eq_hex!(t.unaligned_sbe.Signed23, 0x1F031F);
    }

    #[test]
    fn misc() {
        let mut t = Test::default();

        // booleans
        assert!(t.misc.decode(&[0x82, 0x20]).is_ok());
        assert!(!t.misc.Bool_A);
        assert!(t.misc.Bool_H);
        assert_eq!(t.misc.Float_A, 16.25);
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

        assert_eq_hex!(t.sixty_four_be.SixtyFour, 0x1122334455667788);

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
