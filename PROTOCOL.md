# Cosmos authorization protocol v1

The sender is `RIPEMD160(SHA256(compressed_secp256k1_public_key))`, exactly 20 bytes. The key must use the 33-byte SEC1 compressed encoding with prefix `02` or `03` and be a valid curve point. Bech32 uses the `cosmos` HRP and original Bech32 checksum, not Bech32m. Hex and Bech32 encode the same account bytes.

## Authorized payload

The signed data is the RLP encoding of this ordered list:

```text
[
  UTF8("reth-cosmos-adr036"),
  1,
  chain_id,
  genesis_hash,
  sender,
  nonce,
  recipient,
  value,
  calldata,
  gas_limit,
  max_fee_per_gas,
  max_priority_fee_per_gas
]
```

Integers use minimal unsigned big-endian RLP, with zero represented by the empty byte string. Chain ID, nonce, and gas limit are `u64`; fee bounds are `u128`; value is `u256`. Genesis hash is exactly 32 bytes, and sender and recipient are exactly 20 bytes. The recipient is required; this version supports transfers and calls, not contract creation or access lists.

The only accepted domain is chain ID `366036` and genesis hash:

```text
0x09171e117af661185d9a4a4ff7cf83d29b4b99893bc95b204c0e6e0a6d6a5272
```

All fields are bound by the signature. The decoded sender must equal the account derived from the public key. Changing the protocol requires an explicit version and corresponding node/client changes. The chain's genesis is fixed in the verifier and checked at node startup.

## ADR-036 sign bytes

Construct the following Amino sign document and serialize it as UTF-8 JSON with recursively sorted keys, no whitespace, and the displayed string-valued scalar fields:

```json
{"account_number":"0","chain_id":"","fee":{"amount":[],"gas":"0"},"memo":"","msgs":[{"type":"sign/MsgSignData","value":{"data":"<standard padded base64 of payload>","signer":"<cosmos Bech32 sender>"}}],"sequence":"0"}
```

The placeholder strings above are replaced before signing. The signing digest is SHA-256 of these bytes. The signature is compact secp256k1 ECDSA `r || s`, exactly 64 bytes, with nonzero in-range scalars and low S (`s <= n/2`). DER signatures and recovery bytes are not accepted. The Rust verifier uses k256, and the client uses `Secp256k1Wallet.signAmino` from CosmJS. The [Cosmos ADR-036 specification](https://github.com/cosmos/cosmos-sdk/blob/main/docs/architecture/adr-036-arbitrary-signature.md) defines the outer signing document; this application supplies its domain and replay protection in `data`.

## Signed transaction encoding

```text
raw = 0x7e || RLP([payload_as_byte_string, compressed_public_key, signature_r_concat_s])
transaction_hash = Keccak256(raw)
```

The payload is a byte string containing the encoded inner list. The signature is a single 64-byte string. No Ethereum `v` or recovery parity is encoded. Decoders reject trailing data, unsupported versions, wrong field lengths, non-minimal RLP, and alternate encodings. Maximum raw size is 33,280 bytes; maximum calldata is 32,768 bytes. Size and scalar constraints are checked before expensive cryptography.

For reuse of Alloy's generic storage machinery, the Rust implementation holds the actual r/s scalars in `Signed<TxCosmos>`, with an internal parity invariant of false. Custom wire and RPC serializers omit that parity. All sender-recovery paths dispatch to Cosmos verification, including generic signed/typed conversions.

## RPC and execution

`cosmos_sendRawTransaction` takes one `0x`-prefixed raw transaction string and returns the transaction hash. Errors use JSON-RPC code `-32000` (parameter parsing can also return `-32602`). The normal Ethereum raw transaction endpoint understands the custom envelope as well; both reach the same signature recovery and pool validation.

Use `eth_getBalance`, `eth_getTransactionCount`, `eth_getTransactionByHash`, `eth_getBlockByNumber`, `eth_getStorageAt`, and `eth_getTransactionReceipt` to inspect results. Custom transaction JSON includes `type: "0x7e"`, the authorized fields, `publicKey`, and `cosmosSignature`. The public key and signature are hex byte strings. Quantity fields use Ethereum hex quantities. The response includes the actual recovered `from` and transaction `hash`, and never fabricates Ethereum `v`, `r`, `s`, or `yParity` fields.

The EVM applies EIP-1559 execution rules to the operation. The account pays `gasUsed × effectiveGasPrice`, transfers the requested value on success, and advances its nonce. A reverted call still pays gas and advances its nonce while reverting value and contract state changes. Receipts and receipt-trie commitments use the outer transaction type `0x7e`.

Reth's pool handles nonce ordering, balances, replacement, and duplicate hashes. Included nonces cannot execute again. Block validation repeats Cosmos authorization independently of pool admission, and the normal execution/state-root checks govern imported blocks.
