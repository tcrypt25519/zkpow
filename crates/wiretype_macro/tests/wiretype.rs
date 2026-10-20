// use bytemuck::Zeroable;
// // use zkpow_types::{WireB/ytes, as_wire_bytes};

// #[wiretype]
// #[repr(C)]
// #[derive(Clone, Copy, Debug, PartialEq)]
// struct Pair {
//     a: u32,
//     b: u16,
//     c: u16,
// }

// #[wiretype]
// #[repr(C)]
// #[derive(Clone, Copy, Debug, PartialEq)]
// struct Triple([u8; 3]);

// #[test]
// fn wire_bytes_layout() {
//     assert_eq!(Pair::SIZE, size_of::<Pair>());
//     assert_eq!(Pair::ALIGN, align_of::<Pair>());
//     assert_eq!(Pair::SIZE, 8);
//     assert_eq!(Pair::ALIGN, 4);

//     assert_eq!(Triple::SIZE, 3);
//     assert_eq!(Triple::ALIGN, 1);
// }

// #[test]
// fn wire_bytes_round_trip() {
//     let pair = Pair {
//         a: 0x01020304,
//         b: 0x0506,
//         c: 0x0708,
//     };
//     let bytes = as_wire_bytes(&pair);
//     assert_eq!(bytes.len(), Pair::SIZE);
//     assert_eq!(bytes, &[0x04, 0x03, 0x02, 0x01, 0x06, 0x05, 0x08, 0x07]);
//     assert_eq!(*bytemuck::from_bytes::<Pair>(bytes), pair);

//     let triple = Triple([0xaa, 0xbb, 0xcc]);
//     let bytes = as_wire_bytes(&triple);
//     assert_eq!(bytes.len(), Triple::SIZE);
//     assert_eq!(bytes, &[0xaa, 0xbb, 0xcc]);
// }

// #[test]
// fn wire_bytes_zeroed() {
//     let pair = Pair::zeroed();
//     assert!(as_wire_bytes(&pair).iter().all(|b| *b == 0));

//     let triple = Triple::zeroed();
//     assert!(as_wire_bytes(&triple).iter().all(|b| *b == 0));
// }
