use crate::utils::Round;
use pyo3::prelude::*;
use pyo3::ToPyObject;
use std::{
    collections::HashMap,
    fmt::{self, Debug},
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::Duration,
};
use std::{thread, vec};

use binance::futures::model::OrderTradeEvent;
use binance::{
    account::OrderSide,
    errors::{BinanceContentError, Error, ErrorKind},
    futures::{
        account::{CustomOrderRequest, FuturesAccount, OrderType},
        model::{Symbol, Transaction},
    },
    model::DepthOrderBookEvent,
};

use crate::{position::Position, subscriber::init_stream, utils::get_futures_account};

#[derive(PartialEq)]
enum OrderStatus {
    New,
    Cancelled,
    Filled,
    PartiallyFilled,
    Trade,
}

impl OrderStatus {
    fn from(s: &str) -> Self {
        match s {
            "NEW" => OrderStatus::New,
            "CANCELED" => OrderStatus::Cancelled,
            "FILLED" => OrderStatus::Filled,
            "PARTIALLY_FILLED" => OrderStatus::PartiallyFilled,
            "TRADE" => OrderStatus::Trade,
            _ => {
                dbg!(s);
                panic!("Invalid order status")
            }
        }
    }
}

#[derive(Debug, Clone)]
struct PriceInfo {
    best_bid: f64,
    best_ask: f64,
    mid: f64,
    timestamp: u64,
}

#[derive(Debug, Clone)]
pub struct Order {
    pub transaction: Transaction,
    pub px: f64,
}

impl Order {
    pub fn new(transaction: Transaction, px: f64) -> Self {
        Order {
            transaction: transaction,
            px: px,
        }
    }
}

#[derive(Clone)]
pub struct SymbolTrader {
    pub position: Position,
    pub latest_orderbook_event: Option<DepthOrderBookEvent>,
    pub trade_events: Vec<OrderTradeEvent>,
    pub order: Option<Order>,
    pub keep_running: Arc<AtomicBool>, //external
    pub inflight: Arc<AtomicBool>,
    pub account: FuturesAccount,
    pub info: Symbol,
    pub filled: bool,
    pub avg_entry: Option<f64>,
    pub price_at_start: Option<f64>,
    pub matched_qty: Option<f64>,
    pub mids: Vec<Mid>,
}

impl fmt::Debug for SymbolTrader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SymbolTrader")
            .field("position", &self.position)
            .field("latest_orderbook_event", &self.latest_orderbook_event)
            .field("trade_events", &self.trade_events)
            .field("order", &self.order)
            .field("keep_running", &self.keep_running)
            .finish()
    }
}

pub trait Trader {
    fn new(
        position: Position,
        keep_running: Arc<AtomicBool>,
        info: Symbol,
        account: FuturesAccount,
    ) -> Self;
}

#[pyclass]
#[derive(Debug, Clone)]
pub struct Trade {
    //custom type in case we introduce other exchanges
    timestamp: u64,
    qty: f64,
    px: f64,
}

#[pyclass]
#[derive(Debug, Clone)]
pub struct Mid {
    //custom type in case we introduce other exchanges
    timestamp: u64,
    mid: f64,
}

impl Trader for SymbolTrader {
    fn new(
        position: Position,
        keep_running: Arc<AtomicBool>,
        info: Symbol,
        account: FuturesAccount,
    ) -> SymbolTrader {
        let symbol_trader = SymbolTrader {
            position: position,
            order: None,
            latest_orderbook_event: None,
            trade_events: vec![],
            keep_running: keep_running,
            inflight: Arc::new(AtomicBool::new(false)),
            account: account,
            info: info,
            filled: false,
            avg_entry: None,
            price_at_start: None,
            matched_qty: None,
            mids: vec![],
        };
        symbol_trader
    }
}

impl SymbolTrader {
    pub fn handle_price_event(&mut self, event: DepthOrderBookEvent) {
        self.latest_orderbook_event = Some(event);
        self.on_orderbook_update();
    }

    fn get_min_notional(&self) -> f64 {
        self.info
            .filters
            .iter()
            .find_map(|filter| match filter {
                binance::model::Filters::MinNotional { notional, .. } => {
                    Some(notional.as_ref().unwrap())
                }
                _ => None,
            })
            .unwrap()
            .parse::<f64>()
            .unwrap()
    }

    fn get_last_ts(&self) -> Option<u64> {
        let len_trades = self.trade_events.len();
        match len_trades {
            0 => None,
            _ => Some(self.trade_events.get(len_trades - 1).unwrap().event_time),
        }
    }
    fn get_matched_qty(&self) -> Option<f64> {
        let matched_qty = match self.trade_events.len() {
            0 => None,
            _ => {
                let matched_qty: f64 = self
                    .trade_events
                    .iter()
                    .map(|trade_event| {
                        trade_event
                            .order
                            .qty_last_filled_trade
                            .parse::<f64>()
                            .unwrap()
                    })
                    .sum();

                Some(match self.is_long() {
                    true => matched_qty,
                    false => matched_qty * -1.0,
                })
            }
        };

        matched_qty
    }
    fn get_avg_entry(&self) -> Option<f64> {
        let avg_entry = match self.trade_events.len() {
            0 => None,
            _ => {
                let total_traded_val: f64 = self
                    .trade_events
                    .iter()
                    .map(|trade_event| {
                        trade_event.order.qty.parse::<f64>().unwrap()
                            * trade_event.order.price.parse::<f64>().unwrap()
                    })
                    .sum();
                let total_sz: f64 = self
                    .trade_events
                    .iter()
                    .map(|trade_event| trade_event.order.qty.parse::<f64>().unwrap())
                    .sum();
                Some(total_traded_val / total_sz)
            }
        };

        avg_entry
    }

    fn get_sum_fills(&self) -> f64 {
        //returns total fills in notional
        if self.trade_events.len() == 0 {
            return 0.0;
        }
        let total_fills = self
            .trade_events
            .iter()
            .map(|x| {
                let qty_last_filled_trade = x
                    .order
                    .qty_last_filled_trade
                    .parse::<f64>()
                    .expect("parsable string. wont fail unless binance breaks it");

                let average_price = x
                    .order
                    .average_price
                    .parse::<f64>()
                    .expect("parsable string. wont fail unless binance breaks it");

                average_price * qty_last_filled_trade
            })
            .reduce(|a, b| a + b)
            .unwrap();

        match self.is_long() {
            true => total_fills,
            false => total_fills * -1.0,
        }
    }

    fn get_trades(&self) -> Vec<Trade> {
        self.trade_events
            .iter()
            .map(|x| Trade {
                px: x
                    .order
                    .price_last_filled_trade
                    .parse::<f64>()
                    .expect("parsable string. wont fail unless binance breaks it"),
                qty: x
                    .order
                    .qty_last_filled_trade
                    .parse::<f64>()
                    .expect("parsable string. wont fail unless binance breaks it"),
                timestamp: x.transaction_time,
            })
            .collect()
    }

    fn is_long(&self) -> bool {
        self.position.notional > 0.0
    }

    fn is_stale(&mut self) -> bool {
        let price_info = self.get_price_info();
        match self.is_long() {
            true => self.order.as_ref().expect("will always exist").px < price_info.best_bid,
            false => self.order.as_ref().expect("will always exist").px > price_info.best_ask,
        }
    }

    fn calc_is_filled(&self) -> bool {
        let min_notional_filter = self.get_min_notional();
        self.get_remaining_notional().abs() < min_notional_filter
    }

    fn on_orderbook_update(&mut self) {
        let price_info = self.get_price_info(); // this will get called again if the resting order is stale, so ... a TODO
        self.mids.push(Mid {
            mid: price_info.mid,
            timestamp: price_info.timestamp,
        });
        match &self.order {
            Some(_) => {
                match self.is_stale() {
                    true => self.cancel_order(),
                    false => (),
                };
            }
            None => {
                self.place_marketable_order();
            }
        }
    }

    fn get_remaining_notional(&self) -> f64 {
        let total_fills_notional = self.get_sum_fills();
        self.position.notional - total_fills_notional
    }

    fn get_sz_px(&mut self) -> (f64, f64) {
        let price_info = self.get_price_info();
        let remaining_notional = self.get_remaining_notional();

        println!(
            "symbol: {} self.position.notional: {}, remaining notinal:{}",
            self.position.symbol, self.position.notional, &remaining_notional
        );
        let sz =
            (remaining_notional / price_info.mid).round_to_n(self.info.quantity_precision as i32);
        let px = match self.is_long() {
            true => price_info.best_bid,
            false => price_info.best_ask,
        };
        (sz, px)
    }

    fn get_price_info(&mut self) -> PriceInfo {
        let latest_orderbook_event = self.latest_orderbook_event.as_ref().unwrap();
        let best_bid = latest_orderbook_event.bids.get(0).unwrap().price;
        let best_ask = latest_orderbook_event.asks.get(0).unwrap().price;

        let mid = (best_bid + best_ask) / 2.0;

        if self.price_at_start.is_none() {
            self.price_at_start = Some(mid);
        }

        PriceInfo {
            best_bid: best_bid,
            best_ask: best_ask,
            mid: mid,
            timestamp: latest_orderbook_event.event_time,
        }
    }

    fn cancel_order(&mut self) {
        if self.inflight.load(std::sync::atomic::Ordering::Acquire) {
            println!("cancelledorder inflight");
            return ();
        }
        self.inflight
            .swap(true, std::sync::atomic::Ordering::Release);
        let cancel_order = self.account.cancel_order(
            &self.position.symbol,
            self.order.clone().unwrap().transaction.order_id,
        );
        let success: Result<(), Error> = match cancel_order {
            Ok(_) => {
                self.order = None;
                Ok(())
            }
            Err(e) => match &e.0 {
                ErrorKind::BinanceError(BinanceContentError { code, msg }) => match code {
                    -2011 => {
                        //println!("the order either have been matched or cancelled");
                        self.order = None;
                        Ok(())
                    }
                    _ => {
                        dbg!(code);
                        dbg!(msg);
                        todo!("error at cancel");
                    }
                },
                _ => {
                    dbg!("unhandled error");
                    dbg!(e);
                    panic!("handle");
                }
            },
        };

        self.inflight
            .swap(success.is_err(), std::sync::atomic::Ordering::Release);
    }

    fn place_marketable_order(&mut self) {
        if self.inflight.load(std::sync::atomic::Ordering::Acquire) {
            return ();
        }
        self.inflight
            .swap(true, std::sync::atomic::Ordering::Release);
        let (sz, px) = self.get_sz_px();
        println!("{} @ {}", &sz, &px);
        let side_enum = match self.position.notional < 0.0 {
            true => OrderSide::Sell,
            false => OrderSide::Buy,
        };
        let order = CustomOrderRequest {
            activation_price: None,
            callback_rate: None,
            close_position: Some(false),
            order_type: OrderType::Limit,
            time_in_force: Some(binance::futures::account::TimeInForce::GTX),
            position_side: None,
            price_protect: None,
            price: Some(px),
            qty: Some(sz.abs()),
            reduce_only: Some(self.position.reduce_only),
            side: side_enum,
            stop_price: None,
            symbol: self.position.symbol.to_string(),
            working_type: None,
        };

        let success = match self.account.custom_order(order) {
            Ok(transaction) => {
                self.order = Some(Order::new(transaction, px.clone()));
                Ok(())
            }
            Err(e) => match &e.0 {
                ErrorKind::BinanceError(BinanceContentError { code, msg }) => {
                    match code {
                        -5022 => {
                            self.order = None;
                            Ok(())
                        }
                        -4003 => match self.calc_is_filled() {
                            true => {
                                self.set_filled();
                                Err(e)
                            }
                            false => {
                                dbg!(code);
                                dbg!(msg);
                                self.set_filled();
                                Err(e)
                            }
                        },
                        -2022 => {
                            // "reduce only is rejected
                            //"Quantity less than or equal to zero."
                            dbg!(self.get_remaining_notional(), self.position.notional);
                            match self.position.reduce_only {
                                true => {
                                    self.set_filled();
                                    println!(
                                        "position successfully reduced for, {}",
                                        self.position.symbol
                                    );
                                    Err(e)
                                }
                                false => {
                                    dbg!(code);
                                    dbg!(msg);
                                    panic!("shouldnt happen");
                                }
                            }
                        }
                        -4164 => {
                            // "Order's notional must be no smaller than 20 (unless you choose reduce only)."
                            println!("{} sz is too small, consider filled", self.position.symbol);
                            dbg!(self.get_sz_px());
                            self.set_filled();
                            Err(e)
                        }
                        -1102 => {
                            //"Mandatory parameter 'quantity' was not sent, was empty/null, or malformed."
                            dbg!(code);
                            dbg!(msg);
                            panic!("bug");
                        }
                        _ => {
                            dbg!(code);
                            dbg!(msg);
                            todo!("unhandled error")
                        }
                    }
                }
                _ => {
                    panic!("the lib has a bug");
                }
            },
        };

        self.inflight
            .swap(success.is_err(), std::sync::atomic::Ordering::Release);
    }

    fn set_filled(&mut self) {
        println!("fully filled, exiting for {:#?}", self.position.symbol);
        self.filled = true;
    }

    pub fn on_trade_update(&mut self, event: OrderTradeEvent) {
        self.inflight
            .swap(true, std::sync::atomic::Ordering::Release);
        match (
            OrderStatus::from(event.order.execution_type.as_str()),
            OrderStatus::from(event.order.order_status.as_str()),
        ) {
            (OrderStatus::New, OrderStatus::New) => {}
            (OrderStatus::Cancelled, OrderStatus::Cancelled) => {}
            _ => {
                self.trade_events.push(event.clone());

                if let Some(resting_order) = self.order.as_ref() {
                    match resting_order.transaction.order_id == event.order.order_id {
                        true => {
                            //only handle a fully filled order
                            //partial fills means the order is not stale and will be processed by other
                            // event handlers
                            if OrderStatus::from(event.order.order_status.as_str())
                                == OrderStatus::Filled
                            {
                                self.order = None;
                            }
                        }
                        false => {}
                    }
                }
            }
        }
        self.inflight
            .swap(false, std::sync::atomic::Ordering::Release);
    }
}

pub fn check_if_filled(
    traders: HashMap<String, Arc<Mutex<SymbolTrader>>>,
    running: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        while running.load(std::sync::atomic::Ordering::Acquire) {
            thread::sleep(Duration::from_millis(100)); //no need for low latency here, sleep to reduce cpu use
            {
                let are_filled: bool = traders.iter().all(|x| x.1.lock().unwrap().filled);
                if are_filled {
                    println!("all filled, exiting ...");
                    running.swap(false, std::sync::atomic::Ordering::Release);
                }
            }
        }
    });
}

#[pyclass]
#[derive(Debug, Clone)]
pub struct TraderSummary {
    #[pyo3(get, set)]
    pub position: Position,
    #[pyo3(get, set)]
    pub price_at_start: Option<f64>,
    #[pyo3(get, set)]
    pub avg_entry: Option<f64>,
    #[pyo3(get, set)]
    pub matched_qty: Option<f64>,
    #[pyo3(get, set)]
    pub completed_at: Option<u64>,
    #[pyo3(get, set)]
    pub mids: Vec<Mid>,
    #[pyo3(get, set)]
    pub trades: Vec<Trade>,
}

#[pymethods]
impl TraderSummary {
    pub fn as_dict(&self, py: Python) -> PyObject {
        let key_vals: Vec<(&str, PyObject)> = vec![
            ("symbol", self.position.symbol.to_object(py)),
            ("price_at_start", self.price_at_start.to_object(py)),
            ("avg_entry", self.avg_entry.to_object(py)),
            ("matched_qty", self.matched_qty.to_object(py)),
            ("completed_at", self.completed_at.to_object(py)),
            (
                "trades",
                self.trades
                    .iter()
                    .map(|x| (x.px, x.qty, x.timestamp))
                    .collect::<Vec<(f64, f64, u64)>>()
                    .to_object(py),
            ),
            (
                "mids",
                self.mids
                    .iter()
                    .map(|x| (x.mid, x.timestamp))
                    .collect::<Vec<(f64, u64)>>()
                    .to_object(py),
            ),
        ];
        key_vals.into_py(py)
    }
}

#[pyfunction]
pub fn run_binance(positions: Vec<Position>) -> Vec<TraderSummary> {
    let keep_running = Arc::new(AtomicBool::new(true));

    //get tick size etc
    let (account, general) = get_futures_account();
    let exchange_info = general.exchange_info().unwrap();

    //init traders...
    let traders: HashMap<String, Arc<Mutex<SymbolTrader>>> = positions
        .iter()
        .map(|x| {
            let info = exchange_info
                .symbols
                .iter()
                .find(|y| y.symbol == x.symbol)
                .expect(&format!("{} not found in exchange info", &x.symbol));

            let key = x.symbol.to_string();
            (
                key,
                Arc::new(Mutex::new(SymbolTrader::new(
                    x.clone(),
                    Arc::clone(&keep_running),
                    info.clone(),
                    account.clone(),
                ))),
            )
        })
        .collect();

    let summary = traders.clone();
    check_if_filled(traders.clone(), Arc::clone(&keep_running));
    init_stream(traders, Arc::clone(&keep_running));
    let summary: Vec<TraderSummary> = summary
        .values()
        .map(|x| x.lock().unwrap())
        .map(|x| {
            let summary = TraderSummary {
                position: x.position.clone(),
                avg_entry: x.get_avg_entry(),
                price_at_start: x.price_at_start,
                matched_qty: x.get_matched_qty(),
                completed_at: x.get_last_ts(),
                trades: x.get_trades(),
                mids: x.mids.clone(),
            };

            summary
        })
        .collect();

    println!("done");
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_binance_single_position() {
        dbg!("running test_binance_single_position");
        let positions: Vec<Position> = vec![Position {
            symbol: "BTCUSDT".parse().unwrap(),
            notional: 10000.0,
            reduce_only: false,
        }];

        run_binance(positions);
    }
}
