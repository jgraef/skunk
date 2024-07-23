use std::{
    any::type_name,
    marker::PhantomData,
};

use crate::flow::MessageKind;

impl<DB: sqlx::Database> sqlx::Type<DB> for MessageKind
where
    i8: sqlx::Type<DB>,
{
    fn type_info() -> <DB as sqlx::Database>::TypeInfo {
        <i8 as sqlx::Type<DB>>::type_info()
    }
}

impl<'q, DB: sqlx::Database> sqlx::Encode<'q, DB> for MessageKind
where
    i8: sqlx::Encode<'q, DB>,
{
    fn encode_by_ref(
        &self,
        buf: &mut <DB as sqlx::Database>::ArgumentBuffer<'q>,
    ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync>> {
        sqlx::Encode::<'q, DB>::encode(self.as_db_value(), buf)
    }
}

impl<'r, DB: sqlx::Database> sqlx::Decode<'r, DB> for MessageKind
where
    i8: sqlx::Decode<'r, DB>,
{
    fn decode(
        value: <DB as sqlx::Database>::ValueRef<'r>,
    ) -> Result<Self, sqlx::error::BoxDynError> {
        Ok(Self::from_db_value(<i8 as sqlx::Decode<'r, DB>>::decode(
            value,
        )?)?)
    }
}

impl MessageKind {
    fn as_db_value(&self) -> i8 {
        match self {
            MessageKind::Other => 0,
            MessageKind::Request => 1,
            MessageKind::Response => 2,
        }
    }

    fn from_db_value(value: i8) -> Result<Self, InvalidEnumValue<Self, i8>> {
        match value {
            0 => Ok(Self::Other),
            1 => Ok(Self::Request),
            2 => Ok(Self::Response),
            _ => Err(InvalidEnumValue::<Self, _>::new(value)),
        }
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("Invalid value for {}: {value}", type_name::<T>())]
pub struct InvalidEnumValue<T, U> {
    value: U,
    _ty: PhantomData<fn() -> T>,
}

impl<T, U> InvalidEnumValue<T, U> {
    pub fn new(value: U) -> Self {
        Self {
            value,
            _ty: PhantomData,
        }
    }
}
