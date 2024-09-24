from django.contrib.postgres.fields import ArrayField
import polars as pl
import pandas as pd
from rust_trader import (
    Position as Position,
    run_binance as run_rust_trader_binance,
)
from django.conf import settings
from django.db import models


class OrderQuerySet(models.QuerySet):

    def df(self):
        values = [
            {
                k: v
                for k, v in d.items()
                if k not in ["error", "info", "mids", "trades"]
            }
            for d in list(
                self.values()
            )  # can be explicit here for perf
        ]
        df = (
            pl.DataFrame(values)
            .with_columns(
                side=pl.when(pl.col("sz") > 0).then(1).otherwise(-1)
            )
            .with_columns(
                slippage=(
                    # TODO this will be overstated, should be an average
                    (pl.col("price_actual") / pl.col("price_expected"))
                    - 1
                )
                * (pl.col("side") * -1)
            )
        )
        return df

    def execute(self):
        print("executing via rust trader", self)
        if settings.DRYRUN:
            print("dry run, quitting bulk rust execute")
            return
        exchange = pd.Series([x.exchange for x in self]).unique()
        assert len(exchange) == 1
        if exchange == settings.BINANCE_FUTURES_STRING:
            result = run_rust_trader_binance(
                [x.to_rust_trader() for x in self]
            )
        else:
            raise NotImplementedError("exchange not supported")

        result = [dict(x.as_dict()) for x in result]
        for order in self:
            for execute_result in result:
                if execute_result["symbol"] == order.symbol:
                    order.price_expected = execute_result[
                        "price_at_start"
                    ]
                    order.price_actual = execute_result["avg_entry"]
                    order.matched_qty = execute_result["matched_qty"]
                    order.mids = execute_result["mids"]
                    order.trades = execute_result["trades"]
                    order.completed_at = pd.to_datetime(
                        execute_result["completed_at"], unit="ms"
                    )
        for order in self:
            order.executed = True
            order.save()
        return result


class OrderManager(models.Manager):
    def get_queryset(self):
        return OrderQuerySet(self.model, using=self._db)


class Order(models.Model):

    DRYRUN_ORDER_TEST_ENDPOINT = "/fapi/v1/order/test"

    ORDER_TYPE_LIMIT = "l"
    ORDER_TYPE_MARKET = "m"
    SZ_TYPE_NOTIONAL = "n"
    SZ_TYPE_QTY = "q"

    ORDER_TYPES = (
        ("m", "market"),
        ("l", "limit"),
    )
    SZ_TYPES = (
        ("n", "notional"),
        ("q", "qty"),
    )

    created_at = models.DateTimeField(auto_now_add=True, blank=False)
    completed_at = models.DateTimeField(
        auto_now=False, blank=True, null=True
    )
    exchange = models.CharField(blank=False, max_length=10)
    _type = models.CharField(
        blank=False, choices=ORDER_TYPES, max_length=1
    )
    sz_type = models.CharField(
        blank=False, choices=SZ_TYPES, max_length=1
    )
    symbol = models.CharField(blank=False, max_length=100)
    price_expected = models.FloatField(blank=False)
    price_actual = models.FloatField(blank=True, null=True)
    matched_qty = models.FloatField(blank=True, null=True)
    info = models.JSONField(blank=True, null=True)
    paper = models.BooleanField()
    dry_run = models.BooleanField()
    sz = models.FloatField(null=False)
    executed = models.BooleanField(default=False)
    reduce_only = models.BooleanField(default=False)
    error = models.TextField(blank=True, null=True)
    strategy = models.CharField(blank=False, null=False, max_length=100)
    liquidated = models.BooleanField(null=False, default=False)
    mids = ArrayField(
        ArrayField(models.FloatField(), size=2), null=True
    )
    trades = ArrayField(
        ArrayField(models.FloatField(), size=3), null=True
    )

    objects = OrderManager()

    def __str__(self):
        return f"{'**ERROR** ' if not not self.error else ''}{self.created_at} {self.exchange} {self.symbol} @ {self.price_expected} -- {self.sz}/{self.sz_type}"

    def to_rust_trader(self):
        assert self.sz_type == "n"
        assert self._type == "l"
        assert self.exchange in [
            settings.BINANCE_FUTURES_STRING,
        ]
        if self.exchange == settings.BINANCE_FUTURES_STRING:
            mult = 1 if not self.liquidated else 2
            print("mult,self.liquidate,symbol,ex")
            print(mult, self.liquidated, self.symbol, self.exchange)
            notional = self.sz * mult
            return Position(
                symbol=self.symbol,
                notional=notional,
                reduce_only=self.reduce_only,
            )

    def save(self, *args, **kwargs):
        self.paper = settings.DEBUG
        self.dry_run = settings.DRYRUN
        if self.exchange not in [
            settings.BINANCE_FUTURES_STRING,
        ]:
            raise ValueError(
                f"{self.exchange} not in allowed exchanges"
            )
        if self._type not in ["m", "l"]:
            raise ValueError(f"{self._type} not in allowed order type")
        super().save(*args, **kwargs)
