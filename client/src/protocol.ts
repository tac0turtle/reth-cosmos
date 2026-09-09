import {
  Secp256k1Wallet,
  serializeSignDoc,
  type StdSignDoc,
} from "@cosmjs/amino";
import { ripemd160, sha256 } from "@cosmjs/crypto";
import { fromBase64, toBase64, toBech32 } from "@cosmjs/encoding";
import {
  encodeRlp,
  getBytes,
  hexlify,
  keccak256,
  toBeHex,
  toUtf8Bytes,
} from "ethers";

export const CHAIN_ID = 366036n;
export const TEST_KEY = getBytes(`0x${"0".repeat(63)}1`);
export const RECORDER = "0x0000000000000000000000000000000000001000";
export const REVERTER = "0x0000000000000000000000000000000000001001";
export const RECIPIENT = "0x0000000000000000000000000000000000002000";
export const MAX_CALLDATA = 32768;

export interface Operation {
  chainId: bigint;
  genesisHash: string;
  sender: string;
  nonce: bigint;
  to: string;
  value: bigint;
  input: string;
  gasLimit: bigint;
  maxFeePerGas: bigint;
  maxPriorityFeePerGas: bigint;
}

function integer(n: bigint, bits: number): string {
  if (n < 0n || n >= 1n << BigInt(bits))
    throw new Error(`uint${bits} overflow`);
  return n === 0n ? "0x" : toBeHex(n);
}

function fixed(value: string, length: number): string {
  if (getBytes(value).length !== length)
    throw new Error(`expected ${length} bytes`);
  return value;
}

export function payload(op: Operation): string {
  if (getBytes(op.input).length > MAX_CALLDATA)
    throw new Error("calldata too large");
  if (op.gasLimit < 21000n || op.gasLimit > 30000000n)
    throw new Error("gas limit");
  if (op.maxPriorityFeePerGas > op.maxFeePerGas) throw new Error("fee bounds");
  if (op.nonce === (1n << 64n) - 1n) throw new Error("nonce limit");
  return encodeRlp([
    hexlify(toUtf8Bytes("reth-cosmos-adr036")),
    "0x01",
    integer(op.chainId, 64),
    fixed(op.genesisHash, 32),
    fixed(op.sender, 20),
    integer(op.nonce, 64),
    fixed(op.to, 20),
    integer(op.value, 256),
    op.input,
    integer(op.gasLimit, 64),
    integer(op.maxFeePerGas, 128),
    integer(op.maxPriorityFeePerGas, 128),
  ]);
}

export function signDoc(op: Operation): StdSignDoc {
  return {
    account_number: "0",
    chain_id: "",
    fee: { amount: [], gas: "0" },
    memo: "",
    msgs: [
      {
        type: "sign/MsgSignData",
        value: {
          data: toBase64(getBytes(payload(op))),
          signer: toBech32("cosmos", getBytes(op.sender)),
        },
      },
    ],
    sequence: "0",
  };
}

export async function sign(op: Operation, privateKey = TEST_KEY) {
  const wallet = await Secp256k1Wallet.fromKey(privateKey, "cosmos");
  const [account] = await wallet.getAccounts();
  if (!account) throw new Error("missing signing account");
  const { signed, signature } = await wallet.signAmino(
    account.address,
    signDoc(op),
  );
  const signBytes = serializeSignDoc(signed);
  if (hexlify(signBytes) !== hexlify(serializeSignDoc(signDoc(op)))) {
    throw new Error("wallet changed the authorization");
  }
  const key = fromBase64(signature.pub_key.value);
  const sig = fromBase64(signature.signature);
  const raw =
    "0x7e" + encodeRlp([payload(op), hexlify(key), hexlify(sig)]).slice(2);
  return {
    raw,
    hash: keccak256(raw),
    signBytes: hexlify(signBytes),
    signature: hexlify(sig),
    publicKey: hexlify(key),
    sender: hexlify(ripemd160(sha256(key))),
    cosmos: account.address,
  };
}

export function operation(genesisHash: string, nonce = 0n): Operation {
  return {
    chainId: CHAIN_ID,
    genesisHash,
    sender: "0x751e76e8199196d454941c45d1b3a323f1433bd6",
    nonce,
    to: RECIPIENT,
    value: 123456789n,
    input: "0x",
    gasLimit: 21000n,
    maxFeePerGas: 2000000000n,
    maxPriorityFeePerGas: 100000000n,
  };
}
