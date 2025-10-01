#[cfg(test)]
mod test {
    use assert_eq_float::assert_eq_float;
    use assert_hex::assert_eq_hex;
    use dbc_data::DbcData;

    #[derive(DbcData, Default)]
    #[dbc_file = "tests/test.dbc"]
    struct Test {
        aligned_le: AlignedLE,
        aligned_be: AlignedBE,
        unaligned_ule: UnalignedUnsignedLE,
        unaligned_ube: UnalignedUnsignedBE,
        unaligned_sle: UnalignedSignedLE,
        unaligned_sbe: UnalignedSignedBE,
        #[dbc_signals = "Bool_A, Bool_H, Float_A"]
        misc: MiscMessage,
        sixty_four_le: SixtyFourBitLE,
        sixty_four_be: SixtyFourBitBE,
        sixty_four_signed: SixtyFourBitSigned,
        grouped: [GroupData1; 3],
        #[allow(dead_code)]
        extended: Extended1,
    }

    #[test]
    fn basic() {
        let mut t = Test::default();

        // invalid length
        assert!(!t.aligned_le.decode(&[0x00]));

        // message ID, DLC constants
        assert_eq!(AlignedLE::ID, 1023);
        assert_eq!(AlignedLE::DLC, 8);
        assert_eq!(MiscMessage::ID, 8191);
        assert_eq!(MiscMessage::DLC, 2);
        assert!(!MiscMessage::EXTENDED);
        assert_eq!(Extended1::ID, 0x0012_3456);
        assert!(Extended1::EXTENDED);
    }

    #[test]
    fn value_table() {
        assert_eq_float!(MiscMessage::FLOAT_A_PI, 3.14f32);
        assert_eq_float!(MiscMessage::FLOAT_A_E, 2.718f32);
        assert!(MiscMessage::BOOL_A_ON);
        assert!(!MiscMessage::BOOL_A_OFF);
    }

    #[test]
    fn aligned_unsigned_le() {
        let mut t = Test::default();

        assert!(t
            .aligned_le
            .decode(&[0xfe, 0x55, 0x01, 0x20, 0x34, 0x56, 0x78, 0x9A]));
        assert_eq_hex!(t.aligned_le.Signed8, -2);
        assert_eq_hex!(t.aligned_le.Unsigned8, 0x55);
        assert_eq_hex!(t.aligned_le.Unsigned16, 0x2001);
        assert_eq_hex!(t.aligned_le.Unsigned32, 0x9A78_5634);

        let mut pdu: [u8; 8] = [0u8; 8];
        t.aligned_le.Signed8 = -99;
        t.aligned_le.Unsigned8 = 0x33;
        t.aligned_le.Unsigned16 = 0x78bc;
        assert!(t.aligned_le.encode(pdu.as_mut_slice()));
        assert_eq_hex!(pdu[0], 0x9d);
        assert_eq_hex!(pdu[1], 0x33);
        assert_eq_hex!(pdu[2], 0xbc);
        assert_eq_hex!(pdu[3], 0x78);
    }

    #[test]
    fn aligned_unsigned_be() {
        let mut t = Test::default();

        assert!(t
            .aligned_be
            .decode(&[0xAA, 0x55, 0x01, 0x20, 0x34, 0x56, 0x78, 0x9A]));
        assert_eq_hex!(t.aligned_be.Signed8, -86);
        assert_eq_hex!(t.aligned_be.Unsigned8, 0x55);
        assert_eq_hex!(t.aligned_be.Unsigned16, 0x0120);
        assert_eq_hex!(t.aligned_be.Unsigned32, 0x3456_789A);

        let mut pdu: [u8; 8] = [0u8; 8];
        t.aligned_be.Signed8 = 12;
        t.aligned_be.Unsigned8 = 0x77;
        t.aligned_be.Unsigned16 = 0x78bc;
        t.aligned_be.Unsigned32 = 0x1234_FEDC;
        assert!(t.aligned_be.encode(pdu.as_mut_slice()));
        assert_eq_hex!(pdu[0], 0x0C);
        assert_eq_hex!(pdu[1], 0x77);
        assert_eq_hex!(pdu[2], 0x78);
        assert_eq_hex!(pdu[3], 0xbc);
        assert_eq_hex!(pdu[4], 0x12);
        assert_eq_hex!(pdu[5], 0x34);
        assert_eq_hex!(pdu[6], 0xFE);
        assert_eq_hex!(pdu[7], 0xDC);
    }

    #[test]
    fn unaligned_unsigned_le() {
        let mut t = Test::default();

        // various unaligned values
        assert!(t
            .unaligned_ule
            .decode(&[0xF5, 0x71, 0x20, 0x31, 0xf0, 0xa1, 0x73, 0xfd]));
        assert_eq_hex!(t.unaligned_ule.Unsigned15, 0x2E74);
        assert_eq_hex!(t.unaligned_ule.Unsigned23, 0x007C_0C48);
        assert_eq_hex!(t.unaligned_ule.Unsigned3, 6u8);
        assert_eq_hex!(t.unaligned_ule.Unsigned2, 1u8);
        assert_eq_hex!(t.unaligned_ule.Unsigned2a, 2u8);

        let mut pdu: [u8; 8] = [0xffu8; 8];
        t.unaligned_ule.Unsigned15 = 0x5af5;
        t.unaligned_ule.Unsigned23 = 0x003C_0C49;
        t.unaligned_ule.Unsigned3 = 0x2;
        t.unaligned_ule.Unsigned2 = 0x2;
        t.unaligned_ule.Unsigned2a = 0x3;
        assert!(t.unaligned_ule.encode(pdu.as_mut_slice()));
        assert_eq_hex!(pdu, [0xffu8, 0xd6, 0x27, 0x31, 0xf0, 0xae, 0xd7, 0xfe]);
    }

    #[test]
    fn unaligned_unsigned_be() {
        let mut t = Test::default();

        // various unaligned values
        assert!(t
            .unaligned_ube
            .decode(&[0xfd, 0xe5, 0xa1, 0xf0, 0x31, 0xf8, 0x70, 0x77]));
        assert_eq_hex!(t.unaligned_ube.Unsigned3, 2u8);
        assert_eq_hex!(
            t.unaligned_ube.Unsigned15,
            UnalignedUnsignedBE::UNSIGNED15_TEST
        );
        assert_eq_hex!(t.unaligned_ube.Unsigned23, 0x001F_031F);
    }

    #[test]
    fn unaligned_signed_le() {
        let mut t = Test::default();

        // various unaligned values
        assert!(t
            .unaligned_sle
            .decode(&[0xF7, 0x70, 0x20, 0x31, 0xf0, 0xa1, 0x73, 0xfd]));
        assert_eq_hex!(t.unaligned_sle.Signed15, 0x2E74);
        assert_eq_hex!(t.unaligned_sle.Signed23, 0xFFFC_0C48_u32 as i32);
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
        assert_eq_hex!(t.unaligned_sbe.Signed23, 0x001F_031F);
    }

    #[test]
    fn misc() {
        let mut t = Test::default();

        // booleans
        assert!(t.misc.decode(&[0x82, 0x20]));
        assert!(!t.misc.Bool_A);
        assert!(t.misc.Bool_H);
        assert_eq!(t.misc.Float_A, 16.25);

        let mut pdu: [u8; 2] = [0u8; 2];
        t.misc.Bool_A = true;
        t.misc.Float_A = 20.75;
        assert!(t.misc.encode(pdu.as_mut_slice()));
        assert_eq_hex!(pdu[0], 0x81);
        assert_eq_hex!(pdu[1], 0x29);
    }

    #[test]
    fn sixty_four_bit() {
        let mut t = Test::default();

        // 64-bit unsigned little-endian
        assert!(t
            .sixty_four_le
            .decode(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]));

        assert_eq!(t.sixty_four_le.SixtyFour, 0x8877_6655_4433_2211);

        // 64-bit unsigned big-endian
        assert!(t
            .sixty_four_be
            .decode(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]));

        assert_eq_hex!(t.sixty_four_be.SixtyFour, 0x1122_3344_5566_7788);

        // 64-bit signed little-endian
        assert!(t
            .sixty_four_signed
            .decode(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]));

        assert_eq!(t.sixty_four_signed.SixtyFour, -8_613_303_245_920_329_199);
    }

    #[test]
    fn extract() {
        let data: [u8; 1] = [0x87u8];
        let value = i8::from_le_bytes(data);
        assert_eq!(value, -121);
    }

    #[test]
    fn grouped() {
        let mut t = Test::default();
        assert!(t.grouped[0]
            .decode(&[0xAA, 0x55, 0x01, 0x20, 0x34, 0x56, 0x78, 0x9A]));
        assert_eq!(t.grouped[0].ValueA, 0x2001_55AA);
    }

    #[test]
    fn try_from() {
        let data: [u8; 8] = [0x20, 0x30, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let misc = MiscMessage::try_from(&data[0..2]);
        assert!(misc.is_ok());
        let misc = MiscMessage::try_from(&data[0..3]);
        assert!(misc.is_err());
        let sixty_four = SixtyFourBitLE::try_from(&data[0..8]);
        assert!(sixty_four.is_ok());
    }
}
