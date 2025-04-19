# dbc-data

A derive-macro which produces code to access signals within CAN
messages, as described by a `.dbc` file.  The generated code has
very few dependencies: just core primitives and `[u8]` slices, and
is `#[no_std]` compatible.

## Example

Given a `.dbc` file containing:

```text
BO_ 1023 SomeMessage: 4 Ecu1
 SG_ Unsigned16 : 16|16@0+ (1,0) [0|0] "" Vector__XXX
 SG_ Unsigned8 : 8|8@1+ (1,0) [0|0] "" Vector__XXX
 SG_ Signed8 : 0|8@1- (1,0) [0|0] "" Vector__XXX
```

The following code can be written to access the fields of the
message:

```rust
pub use dbc_data::*;

#[derive(DbcData, Default)]
#[dbc_file = "tests/example.dbc"]
struct TestData {
    some_message: SomeMessage,
}

fn test() {
    let mut t = TestData::default();

    assert_eq!(SomeMessage::ID, 1023);
    assert_eq!(SomeMessage::DLC, 4);
    assert!(t.some_message.decode(&[0x12, 0x34, 0x56, 0x78]).is_ok());
    assert_eq!(t.some_message.Signed8, 0x12);
    assert_eq!(t.some_message.Unsigned8, 0x34);
    assert_eq!(t.some_message.Unsigned16, 0x5678); // big-endian
}
```

As `.dbc` files may contain multiple messages, each of these can be
brought into scope by referencing their name as a type (e.g. `SomeMessage`
as shown above) and this determines what code is generated.  Messages
not referenced will not generate any code.

For cases where only certain signals within a message are needed, the
`#[dbc_signals]` attribute lets you specify which ones are used.

See the test cases in this crate for examples of usage.

## Functionality

* [x] decode signals from PDU
* [ ] encode signals into PDU

* [ ] generate dispatcher for decoding based on ID
* [ ] support multiplexed signals
* [ ] consider scoping generated types to a module

## License

[MIT](/LICENSE-MIT)
