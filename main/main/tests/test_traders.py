from main.models import Order
from django.conf import settings
import polars as pl
from django.test import TestCase, override_settings


@override_settings(DEBUG=True, DRYRUN=False, TESTING=True)
class TestRustTrader(TestCase):
    def test_rust_trader_as_qs_binance(self):
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
        result = orders.execute()
        result = result[0]
        order = Order.objects.get(id=order.id)
        self.assertEqual(order.executed, True)
        self.assertEqual(order.price_actual, result["avg_entry"])
        self.assertEqual(order.price_expected, result["price_at_start"])
        print(order.mids)
        print(order.trades)

    def test_rust_trader_as_qs_reduce_only_binance(self):
        ids = []
        order = Order.objects.create(
            exchange=settings.BINANCE_FUTURES_STRING,
            symbol="BTCUSDT",
            _type="l",
            sz=-150,
            sz_type="n",
            liquidated=True,
            reduce_only=True,
            price_expected=0,
            strategy="test",
        )
        ids.append(order.id)
        orders = Order.objects.filter(id__in=(ids))
        result = orders.execute()
        result = result[0]
        print(result)
        order = Order.objects.get(id=order.id)
        self.assertEqual(order.executed, True)
        self.assertEqual(order.price_actual, result["avg_entry"])
        self.assertEqual(order.price_expected, result["price_at_start"])

        pl.Config.set_tbl_cols(20)
        orders_df = Order.objects.filter(id__in=(ids)).df()
        print(orders_df)

    def test_rust_trader_ls_binance(self):

        print("enter leg..")
        btc_long_order = Order.objects.create(
            exchange=settings.BINANCE_FUTURES_STRING,
            symbol="BTCUSDT",
            _type="l",
            sz=200,
            sz_type="n",
            liquidated=False,
            reduce_only=False,
            price_expected=0,
            strategy="test",
        )
        eth_short_order = Order.objects.create(
            exchange=settings.BINANCE_FUTURES_STRING,
            symbol="ETHUSDT",
            _type="l",
            sz=200,
            sz_type="n",
            liquidated=False,
            reduce_only=False,
            price_expected=0,
            strategy="test",
        )
        result = Order.objects.filter(
            id__in=[btc_long_order.id, eth_short_order.id]
        ).execute()
        print(result)
