//! Derived DBC encode/decode operations

#[cfg(test)]
mod test {
    //    use super::*;
    use dbc_data_derive::DbcData;

    #[allow(dead_code)]
    #[derive(DbcData)]
    #[dbc_file = "tests/test.dbc"]
    struct Test {
        #[dbc_signals = "Unsigned8, Signed8"]
        message_one: Message1,
        message_two: Message2,
    }

    #[test]
    fn basic() {
        let _ = Test {
            message_one: Message1::default(),
            message_two: Message2::default(),
        };
    }
}
