# Changelog

## 0.1.3
* Allow partial parsing of DBC files to generate what code it can; the can_dbc crate does not support all token types (e.g. `BA_DEF_REL_` and `BA_DEF_DEF_REL_`) but if those are later in the file it's still possible to get at the messages and signals of interest
* Add more tests of signed values for LE/BE aligned cases

## 0.1.2
* Adds support for arrays of messages, so that the same type can be used for all instances.  There is no enforcement that the signals within the messages match, and the client is responsible for deciding which IDs should decode into which array elements.  Typically this would be done for some `id` and `message_array: MessageName[COUNT]` via `message_array[id - MessageName::ID]` after a range check that `id` is within `MessageName::ID..message_array.len()` (shown here as `COUNT`, but can be declared as appropriate for the application).  The IDs do not need to be in a contiguous range, as long as the client maps them to the appropriate array indices.

## 0.1.1
* Adds support for unaligned signals (except big-endian, still a TODO)
* Adds `CYCLE_TIME` constant for messages whose DBC declares them

## 0.1.0
* Initial release with support for encoding and decoding (aligned signals only)
