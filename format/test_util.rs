use std::{fmt::Debug, io::Cursor};

use binrw::{io::NoSeek, BinRead, BinWrite};

// NOTE: eh, okay, we assume little endian here
// It's not like we support any other endianness anyway..
// maybe we should explicitly mark everything with a `ReadEndian` and use `::read` instead?

/// Check whether the encodable type roundtrips correctly, both when encoding and decoding.
pub fn assert_enc_dec_pair<
    T: Debug + PartialEq + for<'a> BinRead<Args<'a> = ()> + for<'a> BinWrite<Args<'a> = ()>,
>(
    decoded: &T,
    encoded: &str,
) {
    assert_enc(decoded, encoded);
    assert_dec(decoded, encoded);
}

/// Checks whether the encodable type encodes to the expected value.
pub fn assert_enc<T: Debug + for<'a> BinWrite<Args<'a> = ()>>(decoded: &T, encoded: &str) {
    let mut encoded_actual = NoSeek::new(Vec::new());
    T::write_le(&decoded, &mut encoded_actual).expect("failed to encode");
    assert_eq!(
        hex::encode(encoded_actual.into_inner()),
        encoded,
        "encoded value mismatch for {:?}",
        decoded
    )
}

/// Checks whether the encodable type decodes to the expected value.
pub fn assert_dec<T: Debug + PartialEq + for<'a> BinRead<Args<'a> = ()>>(
    decoded: &T,
    encoded: &str,
) {
    let encoded_bytes = hex::decode(encoded).expect("invalid encoded hex string");

    assert_eq!(
        &T::read_le(&mut Cursor::new(&encoded_bytes)).expect("failed to decode"),
        decoded,
        "decoded value mismatch"
    );
}
