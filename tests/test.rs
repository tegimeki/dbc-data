#[cfg(test)]
mod test {
    use assert_hex::assert_eq_hex;
    use dbc_data::DbcData;

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
        assert!(!t.aligned_ule.decode(&[0x00]));

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
            .decode(&[0xAA, 0x55, 0x01, 0x20, 0x34, 0x56, 0x78, 0x9A]));
        assert_eq_hex!(t.aligned_ule.Unsigned8, 0x55);
        assert_eq_hex!(t.aligned_ule.Unsigned16, 0x2001);
    }

    #[test]
    fn unaligned_unsigned_le() {
        let mut t = Test::default();

        // various unaligned values
        assert!(t
            .unaligned_ule
            .decode(&[0xF7, 0x70, 0x20, 0x31, 0xf0, 0xa1, 0x73, 0xfd]));
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
            .decode(&[0xfd, 0xe5, 0xa1, 0xf0, 0x31, 0xf8, 0x70, 0x77]));
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
            .decode(&[0xF7, 0x70, 0x20, 0x31, 0xf0, 0xa1, 0x73, 0xfd]));
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
            .decode(&[0xfd, 0xe5, 0xa1, 0xf0, 0x31, 0xf8, 0x70, 0x77]));
        assert_eq_hex!(t.unaligned_sbe.Signed3, 2);
        assert_eq_hex!(t.unaligned_sbe.Signed15, 0xC383u16 as i16);
        assert_eq_hex!(t.unaligned_sbe.Signed23, 0x1F031F);
    }

    #[test]
    fn misc() {
        let mut t = Test::default();

        // booleans
        assert!(t.misc.decode(&[0x82, 0x20]));
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
            .decode(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]));

        assert_eq!(t.sixty_four_le.SixtyFour, 0x8877665544332211);

        // 64-bit unsigned big-endian
        assert!(t
            .sixty_four_be
            .decode(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]));

        assert_eq_hex!(t.sixty_four_be.SixtyFour, 0x1122334455667788);

        // 64-bit signed little-endian
        assert!(t
            .sixty_four_signed
            .decode(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]));

        assert_eq!(t.sixty_four_signed.SixtyFour, -8613303245920329199);
    }

    #[test]
    fn extract() {
        let data: [u8; 1] = [0x87u8];
        let value = i8::from_le_bytes(data);
        assert_eq!(value, -121);
    }
}
