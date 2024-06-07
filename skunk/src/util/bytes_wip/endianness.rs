macro_rules! endianness_trait {
    {$(
        $ty:ty : $bytes:expr => $from_name:ident, $to_name:ident;
    )*} => {
        pub trait Endianness {
            $(
                fn $from_name(bytes: [u8; $bytes]) -> $ty;
                fn $to_name(value: $ty) -> [u8; $bytes];
            )*
        }

        impl Endianness for BigEndian {
            $(
                fn $from_name(bytes: [u8; $bytes]) -> $ty {
                    <$ty>::from_be_bytes(bytes)
                }

                fn $to_name(value: $ty) -> [u8; $bytes] {
                    <$ty>::to_be_bytes(value)
                }
            )*
        }

        impl Endianness for LittleEndian {
            $(
                fn $from_name(bytes: [u8; $bytes]) -> $ty {
                    <$ty>::from_le_bytes(bytes)
                }

                fn $to_name(value: $ty) -> [u8; $bytes] {
                    <$ty>::to_le_bytes(value)
                }
            )*
        }
    }
}

pub enum BigEndian {}
pub enum LittleEndian {}

#[cfg(target_endian = "little")]
pub type NativeEndian = LittleEndian;

#[cfg(target_endian = "big")]
pub type NativeEndian = BigEndian;

pub type NetworkEndian = BigEndian;

endianness_trait! {
    u8: 1 => u8_from_bytes, u8_to_bytes;
    i8: 1 => i8_from_bytes, i8_to_bytes;
    u16: 2 => u16_from_bytes, u16_to_bytes;
    i16: 2 => i16_from_bytes, i16_to_bytes;
    u32: 4 => u32_from_bytes, u32_to_bytes;
    i32: 4 => i32_from_bytes, i32_to_bytes;
    u64: 8 => u64_from_bytes, u64_to_bytes;
    i64: 8 => i64_from_bytes, i64_to_bytes;
    u128: 16 => u128_from_bytes, u128_to_bytes;
    i128: 16 => i128_from_bytes, i128_to_bytes;
    f32: 4 => f32_from_bytes, f32_to_bytes;
    f64: 8 => f64_from_bytes, f64_to_bytes;
}
