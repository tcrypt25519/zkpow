use wiretype::{WireType, wiretype};

#[wiretype]
struct Value {
    number: u32,
}

fn encode<T: WireType + ?Sized>(value: &T) -> T::Bytes {
    value.to_bytes()
}

#[test]
fn trait_output_types() {
    let value = Value { number: 42 };
    let bytes: [u8; Value::SIZE] = encode(&value);
    assert_eq!(bytes, 42u32.to_ne_bytes());

    let values = vec![value, Value { number: 17 }];
    let expected = vec![42u32.to_ne_bytes(), 17u32.to_ne_bytes()];
    let vector_bytes: Vec<[u8; Value::SIZE]> = encode(&values);
    let slice_bytes: Vec<[u8; Value::SIZE]> = encode(values.as_slice());
    let reference_bytes: Vec<[u8; Value::SIZE]> = encode(&values.as_slice());
    assert_eq!(vector_bytes, expected);
    assert_eq!(slice_bytes, expected);
    assert_eq!(reference_bytes, expected);
}

#[test]
fn scalar_round_trip() {
    let value = Value { number: 42 };
    assert_eq!(Value::SIZE, 4);
    assert_eq!(Value::ALIGN, 4);
    assert_eq!(value.to_bytes(), 42u32.to_ne_bytes());
    assert_eq!(value.as_bytes(), value.to_bytes());
    assert_eq!(Value::parse(value.as_bytes()), Ok(&value));
    assert!(Value::parse(&[]).is_err());
}

#[test]
fn collection_round_trip() {
    let values = vec![Value { number: 1 }, Value { number: u32::MAX }];
    let slice = values.as_slice();
    let expected: Vec<[u8; Value::SIZE]> = vec![1u32.to_ne_bytes(), u32::MAX.to_ne_bytes()];
    assert_eq!(values.to_bytes(), expected);
    assert_eq!(slice.to_bytes(), expected);
    assert_eq!(Vec::<Value>::parse(values.as_bytes()), Ok(slice));
    assert_eq!(<[Value]>::parse(slice.as_bytes()), Ok(slice));
    assert_eq!(<&[Value]>::parse(slice.as_bytes()), Ok(slice));
}

#[test]
fn empty_collections() {
    let values: Vec<Value> = vec![];
    assert!(values.to_bytes().is_empty());
    assert!(values.as_slice().to_bytes().is_empty());
    assert_eq!(Vec::<Value>::parse(values.as_bytes()), Ok([].as_slice()));
    assert_eq!(<&[Value]>::parse(values.as_bytes()), Ok([].as_slice()));
}

#[test]
fn invalid_collection_bytes() {
    let values = [Value { number: 1 }, Value { number: 2 }];
    let slice = values.as_slice();
    let bytes = slice.as_bytes();
    for invalid in [&bytes[..3], &bytes[1..5]] {
        assert!(Vec::<Value>::parse(invalid).is_err());
        assert!(<&[Value]>::parse(invalid).is_err());
        assert!(Value::parse(invalid).is_err());
    }
}
