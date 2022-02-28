# 01 REST Service

**NOTE**: API is currently unstable and subject to change.

## Example usage

### Get balances

```
GET /collateral/balances
```

### Deposit

The `tokenAccount` defaults to the mint's associated token account, which
must exist. Same goes for withdrawing.

```
POST /collateral/deposit/BTC
{
  "amount": 1,
  "repayOnly": false,
}
```


### Withdraw

```
POST /collateral/withdraw/BTC
{
  "amount": 1,
  "allowBorrow": false,
}
```

### Get position

```
GET /position
```

### View orders

```
GET /orders/BTC-PERP
```

### Place order

`order_type` is one of: `"limit", "ioc", "postonly", "reduceonlyioc", "reduceonlylimit", "fok"`.

```
POST /orders/BTC-PERP
{
  "size": 0.1,
  "price": 40000,
  "side": "bid",
  "orderType": "limit",
}
```

### Delete order

```
DELETE /orders/BTC-PERP?order_id=123456&side="bid"
```

Or, if `clientId` was provided when placing the order.

```
DELETE /orders/BTC-PERP?client_id=123
```
