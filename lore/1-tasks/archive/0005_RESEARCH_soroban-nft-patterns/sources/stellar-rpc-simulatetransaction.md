---
url: 'https://developers.stellar.org/docs/data/apis/rpc/api-reference/methods/simulateTransaction'
title: 'simulateTransaction'
fetched_date: 2026-03-27
task_id: '0005'
overwritten: false
image_count: 0
---

# simulateTransaction

Submit a trial contract invocation to simulate how it would be executed by the network. This endpoint calculates the effective transaction data, required authorizations, and minimal resource fee. It provides a way to test and analyze the potential outcomes of a transaction without actually submitting it to the network.

This method can also be used to invoke read-only smart contract functions for free.

## Params

(4)

Please note that parameter structure within the request must contain named parameters as a by-name object, and not as positional arguments in a by-position array

### 1. transaction _(required)_

In order for the RPC server to successfully simulate a Stellar transaction, the provided transaction must contain only a single operation of the type `invokeHostFunction`.

**Type:** string

A Stellar [TransactionEnvelope](https://github.com/stellar/stellar-xdr/blob/v22.0/Stellar-transaction.x#L1009) (as a base64-encoded string)

### 2. resourceConfig

Contains configuration for how resources will be calculated when simulating transactions.

**Configuration for how resources will be calculated.**

#### instructionLeeway

**Type:** number

Allow this many extra instructions when budgeting resources.

### 3. xdrFormat

Lets the user choose the format in which the response should be returned - either as unpacked JSON or as base64-encoded XDR strings. Note that you should not rely on any schema for the JSON, as it will change when the underlying XDR changes.

**Type:** string

Specifies whether XDR should be encoded as Base64 (default or 'base64') or JSON ('json').

### 4. authMode

Specifies the authorization mode to use when simulating the transaction. The options are 'enforce' (default, enforces all authorization checks), 'record' (records authorization without enforcement), and 'record_allow_nonroot' (records authorization while allowing non-root invocations).

**Type:** string

**Allowed values:**

- enforce
- record
- record_allow_nonroot

## Result

_(simulateTransactionResult)_

The response will include the anticipated effects the given transaction will have on the network. Additionally, information needed to build, sign, and actually submit the transaction will be provided.

### latestLedger

**Type:** number

**Required**

The sequence number of the latest ledger known to Stellar RPC at the time it handled the request.

### minResourceFee

**Type:** string

**(optional)** Stringified number - Recommended minimum resource fee to add when submitting the transaction. This fee is to be added on top of the Stellar network fee. Not present in case of error.

### results

**Type:** array[object]

**(optional)** - This array will only have one element: the result for the Host Function invocation. Only present on successful simulation (i.e. no error) of `InvokeHostFunction` operations.

#### xdr

**Type:** string

**Required**

Serialized base64 string - return value of the Host Function call.

#### auth

**Type:** array[string]

**Required**

Array of serialized base64 strings - Per-address authorizations recorded when simulating this Host Function call.

### transactionData

**Type:** string

**(optional)** Serialized base64 string - The recommended Soroban Transaction Data to use when submitting the simulated transaction. This data contains the refundable fee and resource usage information such as the ledger footprint and IO access data. Not present in case of error.

### events

**Type:** array[string]

**(optional)** Array of serialized base64 strings - Array of the events emitted during the contract invocation. The events are ordered by their emission time. Only present when simulating `InvokeHostFunction` operations; note that it can be present on error, providing extra context about what failed.

### restorePreamble

**Type:** object

**(optional)** - Only present on successful simulation of `InvokeHostFunction` operations. If present, it indicates that the simulation detected archived ledger entries which need to be restored before the submission of the `InvokeHostFunction` operation. The `minResourceFee` and `transactionData` fields should be used to submit a transaction containing a `RestoreFootprint` operation.

#### minResourceFee

**Type:** string

**Required**

Stringified number - Recommended minimum resource fee to add when submitting the `RestoreFootprint` operation.

#### transactionData

**Type:** string

**Required**

Serialized base64 string - The recommended Soroban Transaction Data to use when submitting the `RestoreFootprint` operation.

### stateChanges

**Type:** array[object]

**(optional)** - On successful simulation of `InvokeHostFunction` operations, this field will be an array of `LedgerEntry`s before and after simulation occurred. Note that _at least_ one of `before` or `after` will be present: `before` and no `after` indicates a deletion event, the inverse is a creation event, and both present indicates an update event.

#### type

**Type:** string

**Required**

Indicates if the entry was created, updated, or deleted.

**Allowed values:**

- created
- updated
- deleted

#### key

**Type:** string

**Required**

Base64 - the `LedgerKey` for this delta

#### before

**Type:** string or null

**Required**

Base64, if present - `LedgerEntry` state prior to simulation

#### after

**Type:** string or null

**Required**

Base64, if present - `LedgerEntry` state after simulation

### error

**Type:** string

**(optional)** - This field will include details about why the invoke host function call failed. Only present if the transaction simulation failed.

## Examples

### Successful Transaction Simulation

Transaction simulation that succeeds and returns the necessary information to prepare and submit the transaction.

#### Request

**cURL**

```bash
curl -X POST \
-H 'Content-Type: application/json' \
-d '{
  "jsonrpc": "2.0",
  "id": 8675309,
  "method": "simulateTransaction",
  "params": {
    "transaction": "AAAAAgAAAAAg4dbAxsGAGICfBG3iT2cKGYQ6hK4sJWzZ6or1C5v6GAAAAGQAJsOiAAAAEQAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAGAAAAAAAAAABzAP+dP0PsNzYvFF1pv7a8RQXwH5eg3uZBbbWjE9PwAsAAAAJaW5jcmVtZW50AAAAAAAAAgAAABIAAAAAAAAAACDh1sDGwYAYgJ8EbeJPZwoZhDqEriwlbNnqivULm/oYAAAAAwAAAAMAAAAAAAAAAAAAAAA=",
    "resourceConfig": {
      "instructionLeeway": 3000000
    }
  }
}' \
https://soroban-testnet.stellar.org | jq
```

**JavaScript**

```javascript
let requestBody = {
  jsonrpc: '2.0',
  id: 8675309,
  method: 'simulateTransaction',
  params: {
    transaction:
      'AAAAAgAAAAAg4dbAxsGAGICfBG3iT2cKGYQ6hK4sJWzZ6or1C5v6GAAAAGQAJsOiAAAAEQAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAGAAAAAAAAAABzAP+dP0PsNzYvFF1pv7a8RQXwH5eg3uZBbbWjE9PwAsAAAAJaW5jcmVtZW50AAAAAAAAAgAAABIAAAAAAAAAACDh1sDGwYAYgJ8EbeJPZwoZhDqEriwlbNnqivULm/oYAAAAAwAAAAMAAAAAAAAAAAAAAAA=',
    resourceConfig: {
      instructionLeeway: 3000000,
    },
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

**Python**

```python
import json, requests
res = requests.post(https://soroban-testnet.stellar.org, json={
    "jsonrpc": "2.0",
    "id": 8675309,
    "method": "simulateTransaction",
    "params": {
        "transaction": "AAAAAgAAAAAg4dbAxsGAGICfBG3iT2cKGYQ6hK4sJWzZ6or1C5v6GAAAAGQAJsOiAAAAEQAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAGAAAAAAAAAABzAP+dP0PsNzYvFF1pv7a8RQXwH5eg3uZBbbWjE9PwAsAAAAJaW5jcmVtZW50AAAAAAAAAgAAABIAAAAAAAAAACDh1sDGwYAYgJ8EbeJPZwoZhDqEriwlbNnqivULm/oYAAAAAwAAAAMAAAAAAAAAAAAAAAA=",
        "resourceConfig": {
            "instructionLeeway": 3000000
        }
    }
})
print(json.dumps(res.json(), indent=4))
```

**JSON**

```json
{
  "jsonrpc": "2.0",
  "id": 8675309,
  "method": "simulateTransaction",
  "params": {
    "transaction": "AAAAAgAAAAAg4dbAxsGAGICfBG3iT2cKGYQ6hK4sJWzZ6or1C5v6GAAAAGQAJsOiAAAAEQAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAGAAAAAAAAAABzAP+dP0PsNzYvFF1pv7a8RQXwH5eg3uZBbbWjE9PwAsAAAAJaW5jcmVtZW50AAAAAAAAAgAAABIAAAAAAAAAACDh1sDGwYAYgJ8EbeJPZwoZhDqEriwlbNnqivULm/oYAAAAAwAAAAMAAAAAAAAAAAAAAAA=",
    "resourceConfig": {
      "instructionLeeway": 3000000
    }
  }
}
```

#### Result

```json
{
  "jsonrpc": "2.0",
  "id": 8675309,
  "result": {
    "transactionData": "AAAAAAAAAAIAAAAGAAAAAcwD/nT9D7Dc2LxRdab+2vEUF8B+XoN7mQW21oxPT8ALAAAAFAAAAAEAAAAHy8vNUZ8vyZ2ybPHW0XbSrRtP7gEWsJ6zDzcfY9P8z88AAAABAAAABgAAAAHMA/50/Q+w3Ni8UXWm/trxFBfAfl6De5kFttaMT0/ACwAAABAAAAABAAAAAgAAAA8AAAAHQ291bnRlcgAAAAASAAAAAAAAAAAg4dbAxsGAGICfBG3iT2cKGYQ6hK4sJWzZ6or1C5v6GAAAAAEAHfKyAAAFiAAAAIgAAAAAAAAAAw==",
    "minResourceFee": "90353",
    "events": [
      "AAAAAQAAAAAAAAAAAAAAAgAAAAAAAAADAAAADwAAAAdmbl9jYWxsAAAAAA0AAAAgzAP+dP0PsNzYvFF1pv7a8RQXwH5eg3uZBbbWjE9PwAsAAAAPAAAACWluY3JlbWVudAAAAAAAABAAAAABAAAAAgAAABIAAAAAAAAAACDh1sDGwYAYgJ8EbeJPZwoZhDqEriwlbNnqivULm/oYAAAAAwAAAAM=",
      "AAAAAQAAAAAAAAABzAP+dP0PsNzYvFF1pv7a8RQXwH5eg3uZBbbWjE9PwAsAAAACAAAAAAAAAAIAAAAPAAAACWZuX3JldHVybgAAAAAAAA8AAAAJaW5jcmVtZW50AAAAAAAAAwAAAAw="
    ],
    "results": [
      {
        "auth": [],
        "xdr": "AAAAAwAAAAw="
      }
    ],
    "cost": {
      "cpuInsns": "1635562",
      "memBytes": "1295756"
    },
    "latestLedger": 2552139
  }
}
```

> `simulateTransaction` can also invoke read-only functions for free.

## Using the Lab

The `simulateTransaction` method allows you to **simulate a smart contract invocation** without actually submitting it to the network. It's a powerful tool for testing and debugging transactions safely.

This endpoint returns the **calculated transaction data**, **required authorizations**, and the **minimal resource fee**, helping you understand how the network would process the transaction before you send it.
