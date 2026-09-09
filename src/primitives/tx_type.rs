use alloy_consensus::InMemorySize;
use alloy_eips::{
    Typed2718,
    eip2718::{Eip2718Error, IsTyped2718},
};
use alloy_rlp::{BufMut, Decodable, Encodable};
use reth_codecs::Compact;

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(try_from = "alloy_primitives::U8", into = "alloy_primitives::U8")]
pub struct CosmosTxType(u8);
impl CosmosTxType {
    pub const COSMOS: Self = Self(cosmos_auth::TX_TYPE);
}
impl TryFrom<u8> for CosmosTxType {
    type Error = Eip2718Error;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value == cosmos_auth::TX_TYPE || alloy_consensus::TxType::try_from(value).is_ok() {
            Ok(Self(value))
        } else {
            Err(Eip2718Error::UnexpectedType(value))
        }
    }
}
impl TryFrom<alloy_primitives::U8> for CosmosTxType {
    type Error = Eip2718Error;
    fn try_from(v: alloy_primitives::U8) -> Result<Self, Self::Error> {
        Self::try_from(v.to::<u8>())
    }
}
impl From<CosmosTxType> for alloy_primitives::U8 {
    fn from(v: CosmosTxType) -> Self {
        Self::from(v.0)
    }
}
impl From<alloy_consensus::TxType> for CosmosTxType {
    fn from(v: alloy_consensus::TxType) -> Self {
        Self(v as u8)
    }
}
impl Typed2718 for CosmosTxType {
    fn ty(&self) -> u8 {
        self.0
    }
}
impl IsTyped2718 for CosmosTxType {
    fn is_type(ty: u8) -> bool {
        Self::try_from(ty).is_ok()
    }
}
impl InMemorySize for CosmosTxType {
    fn size(&self) -> usize {
        1
    }
}
impl Encodable for CosmosTxType {
    fn encode(&self, out: &mut dyn BufMut) {
        self.0.encode(out)
    }
    fn length(&self) -> usize {
        self.0.length()
    }
}
impl Decodable for CosmosTxType {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        Ok(Self::try_from(u8::decode(buf)?)?)
    }
}
impl Compact for CosmosTxType {
    fn to_compact<B: BufMut + AsMut<[u8]>>(&self, buf: &mut B) -> usize {
        if self.0 < 3 {
            self.0 as usize
        } else {
            buf.put_u8(self.0);
            3
        }
    }
    fn from_compact(buf: &[u8], identifier: usize) -> (Self, &[u8]) {
        let (ty, rest) = if identifier == 3 {
            (buf[0], &buf[1..])
        } else {
            (
                u8::try_from(identifier).expect("corrupt transaction type"),
                buf,
            )
        };
        (Self::try_from(ty).expect("corrupt transaction type"), rest)
    }
}
