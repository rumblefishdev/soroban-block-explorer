---
url: 'https://developers.stellar.org/docs/data/apis/rpc/api-reference/methods/getLedgerEntries'
title: 'getLedgerEntries'
fetched_date: 2026-03-27
task_id: '0005'
overwritten: false
image_count: 0
---

# getLedgerEntries

For reading the current value of ledger entries directly.

This method enables querying live ledger state: accounts, trustlines, offers, data, claimable balances, and liquidity pools. It also provides direct access to inspect a contract's current state, its code, or any other ledger entry. This serves as a primary method to access your contract data which may not be available via events or `simulateTransaction`.

To fetch contract wasm byte-code, use the ContractCode ledger entry key.

## Params

(2)

Please note that parameter structure within the request must contain named parameters as a by-name object, and not as positional arguments in a by-position array

### 1. keys _(required)_

Array containing the keys of the ledger entries you wish to retrieve. (an array of serialized base64 strings)

**array[string]**

An array of LedgerKeys. The maximum number of ledger keys accepted is 200.

### 2. xdrFormat

Lets the user choose the format in which the response should be returned - either as unpacked JSON or as base64-encoded XDR strings. Note that you should not rely on any schema for the JSON, as it will change when the underlying XDR changes.

**string**

Specifies whether XDR should be encoded as Base64 (default or 'base64') or JSON ('json').

## Result

_(getLedgerEntriesResult)_

### latestLedger

**number** — required

The sequence number of the latest ledger known to Stellar RPC at the time it handled the request.

### entries

**array[object]**

Array of objects containing all found ledger entries

#### key

**string**

The [LedgerKey](https://github.com/stellar/stellar-xdr/blob/v22.0/Stellar-ledger-entries.x#L600) corresponding to the ledger entry (base64 string).

#### xdr

**string**

The key's current [LedgerEntryData](https://github.com/stellar/stellar-xdr/blob/v22.0/Stellar-ledger-entries.x#L564) value (base64 string).

#### lastModifiedLedgerSeq

**number**

The ledger sequence number of the last time this entry was updated.

#### liveUntilLedgerSeq

**number**

The ledger sequence number of the ledger that the entry will be live until. May be zero if the entry is no longer live.

## Examples

### Retrieve a Contract's Counter Entry for an Address

Example request to the `getLedgerEntries` method for a `Counter(Address)` ledger entry.

#### Request

##### cURL

```bash
curl -X POST \
-H 'Content-Type: application/json' \
-d '{
  "jsonrpc": "2.0",
  "id": 8675309,
  "method": "getLedgerEntries",
  "params": {
    "keys": [
      "AAAABgAAAAHMA/50/Q+w3Ni8UXWm/trxFBfAfl6De5kFttaMT0/ACwAAABAAAAABAAAAAgAAAA8AAAAHQ291bnRlcgAAAAASAAAAAAAAAAAg4dbAxsGAGICfBG3iT2cKGYQ6hK4sJWzZ6or1C5v6GAAAAAE="
    ]
  }
}' \
https://soroban-testnet.stellar.org | jq
```

##### JavaScript

```javascript
let requestBody = {
  jsonrpc: '2.0',
  id: 8675309,
  method: 'getLedgerEntries',
  params: {
    keys: [
      'AAAABgAAAAHMA/50/Q+w3Ni8UXWm/trxFBfAfl6De5kFttaMT0/ACwAAABAAAAABAAAAAgAAAA8AAAAHQ291bnRlcgAAAAASAAAAAAAAAAAg4dbAxsGAGICfBG3iT2cKGYQ6hK4sJWzZ6or1C5v6GAAAAAE=',
    ],
  },
};
let res = await fetch('https://soroban-testnet.stellar.org', {
  method: 'POST',
  headers: {
    'Content-Type': 'application/json',
  },
  body: JSON.stringify(requestBody),
});
let json = await res.json();
console.log(json);
```

##### Python

```python
import json, requests
res = requests.post(https://soroban-testnet.stellar.org, json={
    "jsonrpc": "2.0",
    "id": 8675309,
    "method": "getLedgerEntries",
    "params": {
        "keys": [
            "AAAABgAAAAHMA/50/Q+w3Ni8UXWm/trxFBfAfl6De5kFttaMT0/ACwAAABAAAAABAAAAAgAAAA8AAAAHQ291bnRlcgAAAAASAAAAAAAAAAAg4dbAxsGAGICfBG3iT2cKGYQ6hK4sJWzZ6or1C5v6GAAAAAE="
        ]
    }
})
print(json.dumps(res.json(), indent=4))
```

##### JSON

```json
{
  "jsonrpc": "2.0",
  "id": 8675309,
  "method": "getLedgerEntries",
  "params": {
    "keys": [
      "AAAABgAAAAHMA/50/Q+w3Ni8UXWm/trxFBfAfl6De5kFttaMT0/ACwAAABAAAAABAAAAAgAAAA8AAAAHQ291bnRlcgAAAAASAAAAAAAAAAAg4dbAxsGAGICfBG3iT2cKGYQ6hK4sJWzZ6or1C5v6GAAAAAE="
    ]
  }
}
```

#### Result

```json
{
  "jsonrpc": "2.0",
  "id": 8675309,
  "result": {
    "entries": [
      {
        "key": "AAAAB+qfy4GuVKKfazvyk4R9P9fpo2n9HICsr+xqvVcTF+DC",
        "xdr": "AAAABgAAAAAAAAABzAP+dP0PsNzYvFF1pv7a8RQXwH5eg3uZBbbWjE9PwAsAAAAQAAAAAQAAAAIAAAAPAAAAB0NvdW50ZXIAAAAAEgAAAAAAAAAAIOHWwMbBgBiAnwRt4k9nChmEOoSuLCVs2eqK9Qub+hgAAAABAAAAAwAAAAw=",
        "lastModifiedLedgerSeq": 2552504
      }
    ],
    "latestLedger": 2552990
  }
}
```

The Stellar ledger is, on some level, essentially a key-value store. The keys are instances of [LedgerKey](https://github.com/stellar/stellar-xdr/blob/v25.0/Stellar-ledger-entries.x#L588) and the values are instances of [LedgerEntry](https://github.com/stellar/stellar-xdr/blob/v25.0/Stellar-ledger-entries.x#L548). An interesting product of the store's internal design is that the key is a _subset_ of the entry.

The `getLedgerEntries` method returns the "values" (or "entries") for a given set of "keys". Ledger keys come in a lot of forms, and we'll go over the commonly used ones on this page alongside tutorials on how to build and use them.

## Types of `LedgerKey`s

The source of truth should always be the XDR defined in the protocol. `LedgerKey`s are a union type defined in [Stellar-ledger-entries.x](https://github.com/stellar/stellar-xdr/blob/v25.0/Stellar-ledger-entries.x#L588). There are 10 different forms a ledger key can take:

1. **Account:** holistically defines a Stellar account, including its balance, signers, etc.
2. **Trustline:** defines a balance line to a non-native asset issued on the network
3. **Offer:** defines an offer made on the Stellar DEX
4. **Account Data:** defines key-value data entries attached to an account
5. **Claimable Balance:** defines a balance that may or may not actively be claimable
6. **Liquidity Pool:** defines the configuration of a native constant liquidity pool between two assets
7. **Contract Data:** defines a piece of data being stored in a contract under a key
8. **Contract Code:** defines the Wasm bytecode of a contract
9. **Config Setting:** defines the currently active network configuration
10. **TTL:** defines the time-to-live of an associated contract data or code entry

### Accounts

To fetch an account, all you need is its public key:

#### TypeScript

```typescript
import { Keypair, xdr } from '@stellar/stellar-sdk';

const publicKey = 'GALAXYVOIDAOPZTDLHILAJQKCVVFMD4IKLXLSZV5YHO7VY74IWZILUTO';
const accountLedgerKey = xdr.LedgerKey.ledgerKeyAccount(
  new xdr.LedgerKeyAccount({
    accountId: Keypair.fromPublicKey(publicKey).xdrAccountId(),
  })
);
console.log(accountLedgerKey.toXDR('base64'));
```

#### Python

```python
from stellar_sdk import Keypair, xdr

public_key = "GALAXYVOIDAOPZTDLHILAJQKCVVFMD4IKLXLSZV5YHO7VY74IWZILUTO"
account_ledger_key = xdr.LedgerKey(
    type=xdr.LedgerEntryType.ACCOUNT,
    account=xdr.LedgerKeyAccount(
        account_id=Keypair.from_public_key(public_key).xdr_account_id()
    ),
)
print(account_ledger_key.to_xdr())
```

This will give you the full account details.

#### TypeScript

```typescript
const accountEntryData = (
  await s.getLedgerEntries(accountLedgerKey)
).entries[0].account();
```

#### Python

```python
account_entry_data = xdr.LedgerEntryData.from_xdr(
    server.get_ledger_entries([account_ledger_key]).entries[0].xdr
).account
```

#### TypeScript

```typescript
console.log(
  `Account ${publicKey} has ${accountEntryData
    .balance()
    .toString()} stroops of XLM and is on sequence number ${accountEntryData
    .seqNum()
    .toString()}`
);
```

#### Python

```python
print(
    f"Account {public_key} has {account_entry_data.balance.int64} stroops of XLM and is on sequence number {account_entry_data.seq_num.sequence_number.int64}"
)
```

### Trustlines

A trustline is a balance entry for any non-native asset. To fetch one, you need the trustline owner and the asset in question:

#### TypeScript

```typescript
const trustlineLedgerKey = xdr.LedgerKey.ledgerKeyTrustLine(
  new xdr.LedgerKeyTrustLine({
    accountId: Keypair.fromPublicKey(publicKey).xdrAccountId(),
    asset: new Asset(
      'USDC',
      'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN'
    ).toTrustLineXDRObject(),
  })
);
```

#### Python

```python
trustline_ledger_key = xdr.LedgerKey(
    type=xdr.LedgerEntryType.TRUSTLINE,
    trust_line=xdr.LedgerKeyTrustLine(
        account_id=Keypair.from_public_key(public_key).xdr_account_id(),
        asset=Asset(
            "USDC", "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN"
        ).to_trust_line_asset_xdr_object(),
    ),
)

trustline_entry_data = xdr.LedgerEntryData.from_xdr(
    server.get_ledger_entries([trustline_ledger_key]).entries[0].xdr
).trust_line
```

The asset field can be either an issued asset or a liquidity pool:

#### TypeScript

```typescript
let asset: string;
let rawAsset = trustlineEntryData.asset();

switch (rawAsset.switch().value) {
  case AssetType.assetTypeCreditAlphanum4().value:
    asset = Asset.fromOperation(
      xdr.Asset.assetTypeCreditAlphanum4(rawAsset.alphaNum4())
    ).toString();
    break;

  case AssetType.assetTypeCreditAlphanum12().value:
    asset = Asset.fromOperation(
      xdr.Asset.assetTypeCreditAlphanum12(rawAsset.alphaNum12())
    ).toString();
    break;

  case AssetType.assetTypePoolShare().value:
    asset = rawAsset.liquidityPoolId().toXDR('hex');
    break;
}
```

#### Python

```python
raw_asset = trustline_entry_data.asset
asset: str = ""

if (
    raw_asset.type == xdr.AssetType.ASSET_TYPE_CREDIT_ALPHANUM4
    or raw_asset.type == xdr.AssetType.ASSET_TYPE_CREDIT_ALPHANUM12
):
    asset_obj = Asset.from_xdr_object(raw_asset)
    asset = f"{asset_obj.code}:{asset_obj.issuer}"
elif raw_asset.type == xdr.AssetType.ASSET_TYPE_POOL_SHARE:
    asset_obj = LiquidityPoolId.from_xdr_object(raw_asset)
    asset = f"{asset_obj.liquidity_pool_id}"
else:
    raise ValueError("Invalid asset type")
```

### Contract Data

To find a value stored in a contract under a symbol key (e.g. the `COUNTER` entry from the increment example contract):

#### TypeScript

```typescript
import { xdr, Address } from '@stellar/stellar-sdk';

const getLedgerKeySymbol = (
  contractId: string,
  symbolText: string
): xdr.LedgerKey => {
  return xdr.LedgerKey.contractData(
    new xdr.LedgerKeyContractData({
      contract: new Address(contractId).toScAddress(),
      key: xdr.ScVal.scvSymbol(symbolText),
      // The increment contract stores its state in persistent storage,
      // but other contracts may use temporary storage
      // (xdr.ContractDataDurability.temporary()).
      durability: xdr.ContractDataDurability.persistent(),
    })
  );
};

const ledgerKey = getLedgerKeySymbol(
  'CCPYZFKEAXHHS5VVW5J45TOU7S2EODJ7TZNJIA5LKDVL3PESCES6FNCI',
  'COUNTER'
);
```

#### Python

```python
from stellar_sdk import xdr, scval, Address

def get_ledger_key_symbol(contract_id: str, symbol_text: str) -> str:
    ledger_key = xdr.LedgerKey(
        type=xdr.LedgerEntryType.CONTRACT_DATA,
        contract_data=xdr.LedgerKeyContractData(
            contract=Address(contract_id).to_xdr_sc_address(),
            key=scval.to_symbol(symbol_text),
            durability=xdr.ContractDataDurability.PERSISTENT
        ),
    )
    return ledger_key.to_xdr()

print(
    get_ledger_key_symbol(
        "CCPYZFKEAXHHS5VVW5J45TOU7S2EODJ7TZNJIA5LKDVL3PESCES6FNCI",
        "COUNTER"
    )
)
```

### Contract Wasm Code

To understand this, we need a handle on how smart contract deployment works:

- When you deploy a contract, first the code is "installed" (i.e. uploaded onto the blockchain), creating a `LedgerEntry` with the Wasm byte-code that can be uniquely identified by its hash.
- Then, when a contract _instance_ is "instantiated," we create a `LedgerEntry` with a reference to that code's hash. This means many contracts can point to the same Wasm code.

Thus, fetching the contract code is a two-step process:

1. First, we look up the contract itself, to see which code hash it is referencing.
2. Then, we can look up the raw Wasm byte-code using that hash.

#### 1. Find the ledger key for the contract instance

##### TypeScript

```typescript
import { Contract } from '@stellar/stellar-sdk';

function getLedgerKeyContractCode(contractId): xdr.LedgerKey {
  return new Contract(contractId).getFootprint();
}

console.log(
  getLedgerKeyContractCode(
    'CCPYZFKEAXHHS5VVW5J45TOU7S2EODJ7TZNJIA5LKDVL3PESCES6FNCI'
  )
);
```

##### Python

```python
from stellar_sdk import xdr, Address

def get_ledger_key_contract_code(contract_id: str) -> xdr.LedgerKey:
  return xdr.LedgerKey(
    type=xdr.LedgerEntryType.CONTRACT_DATA,
    contract_data=xdr.LedgerKeyContractData(
      contract=Address(contract_id).to_xdr_sc_address(),
      key=xdr.SCVal(xdr.SCValType.SCV_LEDGER_KEY_CONTRACT_INSTANCE),
      durability=xdr.ContractDataDurability.PERSISTENT
    )
  )

print(get_ledger_key_contract_code(
  "CCPYZFKEAXHHS5VVW5J45TOU7S2EODJ7TZNJIA5LKDVL3PESCES6FNCI"
))
```

Once we have the ledger entry (via `getLedgerEntries`), we can extract the Wasm hash:

#### 2. Request the `ContractCode` using the retrieved `LedgerKey`

Take the `xdr` field from the previous response's `result` object, and create a `LedgerKey` from the hash contained inside.

##### TypeScript

```typescript
import { xdr } from '@stellar/stellar-sdk';

function getLedgerKeyWasmId(
  contractData: xdr.ContractDataEntry
): xdr.LedgerKey {
  const wasmHash = contractData.val().instance().executable().wasmHash();

  return xdr.LedgerKey.contractCode(
    new xdr.LedgerKeyContractCode({
      hash: wasmHash,
    })
  );
}
```

##### Python

```python
from stellar_sdk import xdr

def get_ledger_key_wasm_id(
  # received from getLedgerEntries and decoded
  contract_data: xdr.ContractDataEntry
) -> xdr.LedgerKey:
  # First, we dig the wasm_id hash out of the xdr we received from RPC
  wasm_hash = contract_data.val.instance.executable.wasm_hash

  # Now, we can create the `LedgerKey` as we've done in previous examples
  ledger_key = xdr.LedgerKey(
    type=xdr.LedgerEntryType.CONTRACT_CODE,
    contract_code=xdr.LedgerKeyContractCode(
      hash=wasm_hash
    ),
  )
  return ledger_key
```

Now we have a `LedgerKey` that corresponds to the Wasm byte-code deployed under the `contractId` we started with. This `LedgerKey` can be used in a final request to `getLedgerEntries`. In that response we will get a `LedgerEntryData` corresponding to a `ContractCodeEntry` which will contain the actual deployed contract byte-code:

##### TypeScript

```typescript
const theHashData: xdr.ContractDataEntry = await getLedgerEntries(
  getLedgerKeyContractCode('C...')
).entries[0].contractData();

const theCode: Buffer = await getLedgerEntries(getLedgerKeyWasmId(theHashData))
  .entries[0].contractCode()
  .code();
```

##### Python

```python
the_hash_data = xdr.LedgerEntryData.from_xdr(
    server.get_ledger_entries([get_ledger_key_contract_code("C...")]).entries[0].xdr
).contract_data

the_code = xdr.LedgerEntryData.from_xdr(
    server.get_ledger_entries([get_ledger_key_wasm_id(the_hash_data)]).entries[0].xdr
).contract_code.code
```

## Actually fetching the ledger entry data

Once we've learned to build and parse these keys, the process for actually fetching them is always identical. If you know the type of key you fetched, you apply the accessor method accordingly:

#### TypeScript

```typescript
const s = new Server('https://soroban-testnet.stellar.org');

// assume key1 is an account, key2 is a trustline, and key3 is contract data
const response = await s.getLedgerEntries(key1, key2, key3);

const account = response.entries[0].account();
const trustline = response.entries[1].trustline();
const contractData = response.entries[2].contractData();
```

#### Python

```python
server = SorobanServer("https://soroban-testnet.stellar.org")

# assume key1 is an account, key2 is a trustline, and key3 is contract data
response = server.get_ledger_entries([key1, key2, key3])
account = xdr.LedgerEntryData.from_xdr(response.entries[0].xdr).account
trustline = xdr.LedgerEntryData.from_xdr(response.entries[1].xdr).trust_line
contract_data = xdr.LedgerEntryData.from_xdr(response.entries[2].xdr).contract_data
```

Example multi-entry request fetching three different contract ledger keys:

```json
{
  "jsonrpc": "2.0",
  "id": 12345,
  "method": "getLedgerEntries",
  "params": {
    "keys": [
      "AAAAB+QzbW3JDhlUbDVW/C+1/5SIQDstqORuhpCyl73O1vH6",
      "AAAABgAAAAGfjJVEBc55drW3U87N1Py0Rw0/nlqUA6tQ6r28khEl4gAAABQAAAAB",
      "AAAABgAAAAAAAAABn4yVRAXOeXa1t1POzdT8tEcNP55alAOrUOq9vJIRJeIAAAAUAAAAAQAAABMAAAAA5DNtbckOGVRsNVb8L7X/lIhAOy2o5G6GkLKXvc7W8foAAAAA"
    ]
  }
}
```

## Viewing and understanding XDR

If you don't want to parse the XDR out programmatically, you can also leverage both the [Stellar CLI](https://developers.stellar.org/docs/tools/cli/stellar-cli) and the [Stellar Lab](https://lab.stellar.org/xdr/view) to get a human-readable view of ledger keys and entries. For example:

```bash
echo 'AAAAAAAAAAAL76GC5jcgEGfLG9+nptaB9m+R44oweeN3EcqhstdzhQ==' | stellar xdr decode --type LedgerKey --output json-formatted
{
  "account": {
    "account_id": "GAF67IMC4Y3SAEDHZMN57J5G22A7M34R4OFDA6PDO4I4VINS25ZYLBZZ"
  }
}
```

## Using the Lab

The `getLedgerEntries` method allows you to **read live ledger data directly** from the network. It is especially useful for inspecting a contract's **current state**, **deployed code**, or any other ledger entry tied to your application. This method is often the **primary way to retrieve contract-related data** that may not surface through events or `simulateTransaction`.

To retrieve a contract's WASM byte-code, use the `ContractCode` ledger entry key.
