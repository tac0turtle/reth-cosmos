//! Retain the original compact database layout through upstream codec extension traits.
use super::{CosmosEnvelope, CosmosSigned, CosmosTxType, Transaction};
use alloy_consensus::{EthereumTxEnvelope, TransactionEnvelope, TxEip4844};
use alloy_primitives::{Bytes, Signature};
use alloy_rlp::{BufMut, Decodable, Encodable, Header};
use reth_codecs::{
    Compact,
    alloy::transaction::{CompactEnvelope, Envelope, FromTxCompact, ToTxCompact},
};

impl ToTxCompact for Transaction {
    fn to_tx_compact(&self, buf: &mut (impl BufMut + AsMut<[u8]>)) {
        match self {
            CosmosEnvelope::Ethereum(tx) => tx.to_tx_compact(buf),
            CosmosEnvelope::Cosmos(tx) => {
                let payload = tx.operation().payload();
                Header {
                    list: true,
                    payload_length: payload.as_slice().length() + tx.public_key().length(),
                }
                .encode(buf);
                payload.as_slice().encode(buf);
                tx.public_key().encode(buf);
            }
        }
    }
}
impl FromTxCompact for Transaction {
    type TxType = CosmosTxType;
    fn from_tx_compact(mut buf: &[u8], ty: CosmosTxType, signature: Signature) -> (Self, &[u8]) {
        if ty != CosmosTxType::COSMOS {
            use alloy_eips::Typed2718;
            let (tx, rest) = EthereumTxEnvelope::<TxEip4844>::from_tx_compact(
                buf,
                alloy_consensus::TxType::try_from(ty.ty()).expect("stored Ethereum type"),
                signature,
            );
            return (Self::Ethereum(tx), rest);
        }
        assert!(!signature.v(), "corrupt Cosmos signature parity");
        let header = Header::decode(&mut buf).expect("corrupt Cosmos storage header");
        assert!(
            header.list
                && header.payload_length <= cosmos_auth::MAX_RAW_TX
                && header.payload_length <= buf.len(),
            "corrupt Cosmos storage size"
        );
        let (mut fields, rest) = buf.split_at(header.payload_length);
        let payload = Bytes::decode(&mut fields).expect("corrupt Cosmos payload");
        let key = Bytes::decode(&mut fields).expect("corrupt Cosmos key");
        assert!(fields.is_empty(), "trailing Cosmos storage fields");
        let op =
            cosmos_auth::Operation::decode_payload(&payload).expect("corrupt Cosmos operation");
        let tx = CosmosSigned::new(op, key, Bytes::copy_from_slice(&signature.as_bytes()[..64]))
            .expect("corrupt Cosmos transaction");
        (Self::Cosmos(tx), rest)
    }
}
impl Envelope for Transaction {
    fn signature(&self) -> &Signature {
        match self {
            Self::Ethereum(tx) => tx.signature(),
            Self::Cosmos(tx) => tx.signature(),
        }
    }
    fn tx_type(&self) -> CosmosTxType {
        TransactionEnvelope::tx_type(self)
    }
}
impl Compact for Transaction {
    fn to_compact<B: BufMut + AsMut<[u8]>>(&self, buf: &mut B) -> usize {
        <Self as CompactEnvelope>::to_compact(self, buf)
    }
    fn from_compact(buf: &[u8], len: usize) -> (Self, &[u8]) {
        <Self as CompactEnvelope>::from_compact(buf, len)
    }
}

impl reth_codecs::Compress for Transaction {
    type Compressed = Vec<u8>;
    fn compress_to_buf<B: BufMut + AsMut<[u8]>>(&self, buf: &mut B) {
        Compact::to_compact(self, buf);
    }
}
impl reth_codecs::Decompress for Transaction {
    fn decompress(value: &[u8]) -> Result<Self, reth_codecs::DecompressError> {
        Ok(<Self as Compact>::from_compact(value, value.len()).0)
    }
}
