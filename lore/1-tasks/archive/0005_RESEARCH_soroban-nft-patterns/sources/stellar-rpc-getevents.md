---
url: 'https://developers.stellar.org/docs/data/apis/rpc/api-reference/methods/getEvents'
title: 'getEvents - Stellar RPC API Reference'
fetched_date: 2026-03-26
task_id: '0005'
image_count: 1
images:
  - original_url: 'https://developers.stellar.org/assets/images/getevents-2cad6ef108087a5c2275a1625326554b.jpg'
    local_path: 'images/stellar-rpc-getevents/img_1.jpg'
    alt: 'Stellar Laboratory UI for the getEvents RPC method, showing fields for Start Ledger Sequence (572089), Cursor, Limit, Filters Type (Contract), Contract IDs, Topics, and XDR Format (base64). The payload panel displays the constructed JSON-RPC request body.'
---

# getEvents

Clients can request a filtered list of events emitted by a given ledger range.

Stellar-RPC will support querying within a maximum 7 days of recent ledgers.

Note, this could be used by the client to only prompt a refresh when there is a new ledger with relevant events. It should also be used by backend Dapp components to "ingest" events into their own database for querying and serving.

If making multiple requests, clients should deduplicate any events received, based on the event's unique id field. This prevents double-processing in the case of duplicate events being received.

By default stellar-rpc retains the most recent 24 hours of events.

## Params

Please note that parameter structure within the request must contain named parameters as a by-name object, and not as positional arguments in a by-position array.

### 1. startLedger

Ledger sequence number to start fetching responses from (inclusive). This method will return an error if `startLedger` is less than the oldest ledger stored in this node, or greater than the latest ledger seen by this node. If a cursor is included in the request, `startLedger` must be omitted.

**Type:** number

Sequence number of the ledger.

### 2. endLedger

Ledger sequence number represents the end of search window (exclusive). If a cursor is included in the request, `endLedger` must be omitted.

**Type:** number

Sequence number of the ledger.

### 3. filters

List of filters for the returned events. Events matching any of the filters are included. To match a filter, an event must match both a contractId and a topic. Maximum 5 filters are allowed per request.

**Type:** array of objects (<= 5 items)

**type** (string)
Filter events by type. If omitted, all event types are included.

Allowed values:

- `system`
- `contract`

**contractIds** (array[string])
List of contract IDs to query for events. If omitted, return events for all contracts. Maximum 5 contract IDs are allowed per request. (<= 5 items)

**topics** (array[array])
A list of topic filters. Each filter is itself an array of one to four `SegmentMatcher` elements. If omitted, query for all events. If multiple filters are specified, events will be included if they match any of the filters. (<= 5 items)

**SegmentMatcher** (string)
A `SegmentMatcher` is one of the following:

- An exact ScVal encoded as base64 XDR
- `"*"` — matches any single segment
- `"**"` — matches zero or more trailing segments (only valid as the last element)

### 4. pagination

Pagination in stellar-rpc is similar to pagination in Horizon. See [Pagination](https://developers.stellar.org/docs/data/rpc/api-reference/structure/pagination).

**cursor** (string)
An opaque string which acts as a paging token. To obtain the next page of results occurring after a given response set this value to the `cursor` field of the response.

**limit** (number)
The maximum number of records returned. The limit for getEvents can range from 1 to 10000 — an upper limit that is hardcoded in Stellar-RPC for performance reasons. If this argument isn't designated, it defaults to 100.

### 5. xdrFormat

Lets the user choose the format in which the response should be returned — either as unpacked JSON or as base64-encoded XDR strings. Note that you should not rely on any schema for the JSON, as it will change when the underlying XDR changes.

**Type:** string

Specifies whether XDR should be encoded as Base64 (default or `'base64'`) or JSON (`'json'`).

## Result

_(getEventsResult)_

**latestLedger** (number)
The sequence number of the latest ledger known to Stellar RPC at the time it handled the request.

**events** (array[object])

**type** (string)
The type of event emission.

Allowed values:

- `contract`
- `system`

**ledger** (number)
Sequence number of the ledger in which this event was emitted.

**ledgerClosedAt** (string)
[ISO-8601](https://www.iso.org/iso-8601-date-and-time-format.html) timestamp of the ledger closing time.

**contractId** (string)
StrKey representation of the contract address that emitted this event.

**id** (string)
Unique identifier for this event, based on the [TOID](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0035.md#specification) format. It combines a 19-character TOID and a 10-character, zero-padded event index, separated by a hyphen.

**transactionIndex** (number)
The index of the transaction within the ledger this event occurred in.

**operationIndex** (number)
The index of the operation within the transaction this event occurred in.

**inSuccessfulContractCall** (boolean) — _deprecated_
If true the event was emitted during a successful contract call.

**topic** (array[string])
The [ScVal](https://github.com/stellar/stellar-xdr/blob/v22.0/Stellar-contract.x#L214)s containing the topics this event was emitted with (as a base64 string). (>= 1 items, <= 4 items)

**value** (string)
The data emitted by the event (an [ScVal](https://github.com/stellar/stellar-xdr/blob/v22.0/Stellar-contract.x#L214), serialized as a base64 string).

**txHash** (string)
The transaction which triggered this event. (64 hex characters, pattern: `^[a-f\d]{64}$`)

**cursor** (string)
A token which can be included in a subsequent request to obtain the next page of results.

## Examples

### Native XLM Transfer Events

Example request to the `getEvents` method, filtering for `transfer` events for native Lumens and limiting the number of returned events to 2.

#### Request

**cURL**

```bash
curl -X POST \
-H 'Content-Type: application/json' \
-d '{
  "jsonrpc": "2.0",
  "id": 8675309,
  "method": "getEvents",
  "params": {
    "startLedger": 199616,
    "filters": [
      {
        "type": "contract",
        "contractIds": [
          "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC"
        ],
        "topics": [
          [
            "AAAADwAAAAh0cmFuc2Zlcg==",
            "*",
            "*",
            "**"
          ]
        ]
      }
    ],
    "pagination": {
      "limit": 2
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
  method: 'getEvents',
  params: {
    startLedger: 199616,
    filters: [
      {
        type: 'contract',
        contractIds: [
          'CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC',
        ],
        topics: [['AAAADwAAAAh0cmFuc2Zlcg==', '*', '*', '**']],
      },
    ],
    pagination: {
      limit: 2,
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
res = requests.post('https://soroban-testnet.stellar.org', json={
    "jsonrpc": "2.0",
    "id": 8675309,
    "method": "getEvents",
    "params": {
        "startLedger": 199616,
        "filters": [
            {
                "type": "contract",
                "contractIds": [
                    "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC"
                ],
                "topics": [
                    [
                        "AAAADwAAAAh0cmFuc2Zlcg==",
                        "*",
                        "*",
                        "**"
                    ]
                ]
            }
        ],
        "pagination": {
            "limit": 2
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
  "method": "getEvents",
  "params": {
    "startLedger": 199616,
    "filters": [
      {
        "type": "contract",
        "contractIds": [
          "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC"
        ],
        "topics": [["AAAADwAAAAh0cmFuc2Zlcg==", "*", "*", "**"]]
      }
    ],
    "pagination": {
      "limit": 2
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
    "events": [
      {
        "type": "contract",
        "ledger": 200010,
        "ledgerClosedAt": "2025-06-30T07:27:13Z",
        "contractId": "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
        "id": "0000859036408881152-0000000003",
        "pagingToken": "0000859036408881152-0000000003",
        "inSuccessfulContractCall": true,
        "txHash": "d9e771ac73ec80503c7594f540d10ec068fb80981d11acea41aa193b7543c5ce",
        "topic": [
          "AAAADwAAAAh0cmFuc2Zlcg==",
          "AAAAEgAAAAAAAAAA6qNYcgGe/Zw2XRAUKPzIjtK2Cfp0eT8bn/BCJTcEq4s=",
          "AAAAEgAAAAGWF5MS3cqvdZjF5BY4yqI44bey/KmQmH9oF0gX3IIWuw==",
          "AAAADgAAAAZuYXRpdmUAAA=="
        ],
        "value": "AAAACgAAAAAAAAAAAAAAAF2UTIA="
      },
      {
        "type": "contract",
        "ledger": 201047,
        "ledgerClosedAt": "2025-06-30T08:53:44Z",
        "contractId": "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
        "id": "0000863490289963008-0000000010",
        "pagingToken": "0000863490289963008-0000000010",
        "inSuccessfulContractCall": true,
        "txHash": "d0ee56996d4a750989c385bde0feb322825dbcf82e8053659806e79db1998828",
        "topic": [
          "AAAADwAAAAh0cmFuc2Zlcg==",
          "AAAAEgAAAAAAAAAACMEAtVPau/0s+2y4o3aWt1MAtjmdqWNzPmy6MRVcdfo=",
          "AAAAEgAAAAHlC9It3oz+Kboqo4BcasoaIkOFNSzYyClfcGqpVj/sJA==",
          "AAAADgAAAAZuYXRpdmUAAA=="
        ],
        "value": "AAAACgAAAAAAAAAAAAAAAAAAAAo="
      }
    ],
    "latestLedger": 320543,
    "cursor": "0000863490289963008-0000000010"
  }
}
```

### Using the Lab

The Stellar Laboratory supports sharable URLs that prefill input fields based on query parameters, making it easy to share and revisit specific configurations.

[View Native XLM Transfer Events example on the Lab](https://lab.stellar.org/endpoints/rpc/get-events?$=network$id=testnet&label=Testnet&horizonUrl=https:////horizon-testnet.stellar.org&rpcUrl=https:////soroban-testnet.stellar.org&passphrase=Test%20SDF%20Network%20/;%20September%202015;&endpoints$params$startLedger=572089&filters=%7B%22type%22:%22contract%22,%22contract_ids%22:%5B%22CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC%22%5D,%22topics%22:%5B%22%5B%5C%22AAAADwAAAAh0cmFuc2Zlcg==%5C%22,%5C%22%2A%5C%22,%5C%22%2A%5C%22,%5C%22%2A%5C%22%5D%22%5D%7D;;)

![Stellar Laboratory UI for the getEvents RPC method, showing fields for Start Ledger Sequence (572089), Cursor, Limit, Filters Type (Contract), Contract IDs, Topics, and XDR Format (base64). The payload panel displays the constructed JSON-RPC request body.](images/stellar-rpc-getevents/img_1.jpg)
