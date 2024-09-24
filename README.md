## An algo for placing a limit order at the best price, written in Rust, callable from Python (Django). Binance Futures only.



### Overview

The algorithm subscribes to real-time order book snapshots and trade events, then submits/resubmits/cancels a make-only limit order at the best price. It monitor fills until the target notional has been reached and exits once filled.


**Features**: 
- Python API integration through Django
- Records trades on Postgres
- Simple post-trade analysis
- Concurrency

**Notes**: 
- The code has only been run on a `ap-northeast-1` machine.
- The BBO is evaluated against the resting order (possibly canceled/resubmitted) at every tick. If you're doing multiple symbols simultaneously, rate limiting might be an issue.
- Edge cases are not handled. Use at your own risk.

![Alt text](screenshot.png?raw=true "Screenshot")

Here's an overview of the execution flow:

```
+-----------------------------------------+
| OrderBook Snapshot / Trade Event Arrival|
+-----------------------------------------+
                  |
                  |
                  v
+-----------------------------------------+
|  Check for Existing Resting Order       |
+-----------------------------------------+
                  |                                          
    Resting Order Exists? (Yes) ------------> No ------------> Submits a resting order at the BBO
                  |                           |
                  |                           |
                  |                           |
    Resting price is stale? (Yes) ----------> No
                  |                           |
+-----------------------------------------+   |
|     Cancel Resting Order                |   |
+-----------------------------------------+   |
                  |                           |
                  |                           |
            Check Fills                   Check Fills
                  |                           |
                  |                           |
            +-------------------------+       |
            |Reached Target Notional? | <-----+
            +-------------------------+
                  |             |
                  |             |
               (Yes) -------->  No
                  |             |
                  |             |
                  v
                Exit          Do nothing
                                |
                                |
                                .
                                .
```


best price is defined as:
```
bids=[(64_999,1),(64_998,1)]
asks=[(65_005,1),(65_006,1)]

best_price = match order.side {
    -1 => 65_005,
    1 => 64_999,
}
```




---

to initiate the project via Docker Compose:

```
./dup.sh docker/compose_files/main.yml
```

to bulk execute:

```
from main.models import Order

ids = []
order = Order.objects.create(
    exchange=settings.BINANCE_FUTURES_STRING,
    symbol="BTCUSDT",
    _type="l",
    sz=15_000,
    sz_type="n",
    liquidated=False,
    reduce_only=False,
    price_expected=0,
    strategy="test",
)
ids.append(order.id)
orders = Order.objects.filter(id__in=(ids))
orders.execute()
```

you can call `.df()` on the queryset for simple post-trade metrics:

```
>>> orders.df()
shape: (1, 19)
┌─────┬─────────────┬────────────┬──────────┬───┬──────────┬────────────┬──────┬───────────┐
│ id  ┆ created_at  ┆ completed_ ┆ exchange ┆ … ┆ strategy ┆ liquidated ┆ side ┆ slippage  │
│ --- ┆ ---         ┆ at         ┆ ---      ┆   ┆ ---      ┆ ---        ┆ ---  ┆ ---       │
│ i64 ┆ datetime[μs ┆ ---        ┆ str      ┆   ┆ str      ┆ bool       ┆ i32  ┆ f64       │
│     ┆ ]           ┆ datetime[μ ┆          ┆   ┆          ┆            ┆      ┆           │
│     ┆             ┆ s]         ┆          ┆   ┆          ┆            ┆      ┆           │
╞═════╪═════════════╪════════════╪══════════╪═══╪══════════╪════════════╪══════╪═══════════╡
│ 6   ┆ 2024-09-24  ┆ 2024-09-24 ┆ binfut   ┆ … ┆ test     ┆ false      ┆ 1    ┆ -0.001285 │
│     ┆ 09:47:38.59 ┆ 09:50:07.8 ┆          ┆   ┆          ┆            ┆      ┆           │
│     ┆ 7414        ┆ 34         ┆          ┆   ┆          ┆            ┆      ┆           │
└─────┴─────────────┴────────────┴──────────┴───┴──────────┴────────────┴──────┴───────────┘
>>>
```

for more in-depth analysis, timestamped matched trades and mid prices are also available:

```
>>> orders[0].mids
[(63235.95, 1727171284697), (63366.6, 1727171284898), (63366.6, 1727171285007), (63366.6, 1727171285115), (63366.6, 1727171285401), (63366.6, 1727171285547), (63366.6, 1727171286253), (63366.6, 1727171286957), (63366.6, 1727171287165), (63366.6, 1727171298252), (63403.6, 1727171298641), (63403.6, 1727171298897), (63403.6, 1727171299153), (63403.6, 1727171300616), (63403.6, 1727171301121), (63403.6, 1727171307443), (63403.6, 1727171311345), (63403.6, 1727171330523), (63403.6, 1727171344813), (63403.6, 1727171378475), (63403.6, 1727171393138), (63403.6, 1727171402506), (63270.0, 1727171407932), (63330.85, 1727171408040)]
>>> orders[0].trades
[(63317.2, 0.014, 1727171378370), (63317.2, 0.223, 1727171407827)]
>>>
```
