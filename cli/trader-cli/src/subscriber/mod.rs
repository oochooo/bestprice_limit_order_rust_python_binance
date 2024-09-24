use binance::{
    api::Binance,
    config::Config,
    errors::Error,
    futures::{userstream::FuturesUserStream, websockets::*},
};
use std::{
    collections::HashMap,
    env,
    fmt::Debug,
    sync::{atomic::AtomicBool, Arc, Mutex},
};
use tungstenite::connect;

use crate::trader::SymbolTrader;

#[derive(Debug, Clone)]
pub struct Keys {
    pub api_key: String,
    pub secret_key: String,
}

impl Keys {
    pub fn new() -> Self {
        Keys {
            api_key: env::var("BINANCE_API_KEY").unwrap(),
            secret_key: env::var("BINANCE_SECRET_KEY").unwrap(),
        }
    }
}

pub fn get_config() -> Config {
    // -----
    // only use combined streams
    // the single is untested and probably is broken
    // -----

    let debug = match env::var("DEBUG")
        .expect("DEBUG doesnt exist")
        .parse::<i32>()
        .unwrap()
    {
        0 => false,
        1 => true,
        _ => {
            panic!("wrong debug val in .env")
        }
    };
    if debug {
        let testnet_config = Config::testnet();
        let testnet_config =
            testnet_config.set_futures_ws_endpoint("wss://stream.binancefuture.com");
        return testnet_config;
    }
    Config::default().set_futures_ws_endpoint("wss://fstream.binance.com")
}

pub fn keep_user_stream_alive() -> String {
    let keys = Keys::new();
    let config = get_config();
    let user_stream: FuturesUserStream =
        Binance::new_with_config(Some(keys.api_key), Some(keys.secret_key), &config);

    let listen_key = user_stream.start().unwrap().listen_key;
    user_stream.keep_alive(&listen_key).unwrap();

    listen_key
}

trait ExtendedFuturesWebSocketsTrait<'a> {
    fn connect_multiple_streams_with_config(
        &mut self,
        endpoints: &[String],
        config: Config,
    ) -> Result<(), Error>;
}
impl<'a> ExtendedFuturesWebSocketsTrait<'a> for FuturesWebSockets<'a> {
    fn connect_multiple_streams_with_config(
        &mut self,
        endpoints: &[String],
        config: Config,
    ) -> Result<(), Error> {
        let url = format!(
            "{}/stream?streams={}",
            config.futures_ws_endpoint.clone(),
            &endpoints.join("/")
        );

        println!("{}", &url);

        match connect(url) {
            Ok(answer) => {
                self.socket = Some(answer);
                Ok(())
            }
            Err(e) => {
                let err = Err(Error::from_kind(binance::errors::ErrorKind::Tungstenite(e)));
                err
            }
        }
    }
}

pub fn init_stream<'a>(
    traders: HashMap<String, Arc<Mutex<SymbolTrader>>>,
    keep_running: Arc<AtomicBool>,
) {
    let mut streams: Vec<String> = traders
        .clone()
        .iter()
        .map(|f| {
            format!(
                "{}@depth5@100ms",
                f.1.lock()
                    .unwrap()
                    .position
                    .symbol
                    .to_string()
                    .to_lowercase()
            )
        })
        .collect();
    let callback_fn = {
        let mut traders = traders;

        move |event: FuturesWebsocketEvent| {
            match event {
                FuturesWebsocketEvent::DepthOrderBook(event) => {
                    traders
                        .get_mut(&event.symbol)
                        .unwrap()
                        .lock()
                        .unwrap()
                        .handle_price_event(event);
                }
                FuturesWebsocketEvent::UserDataStreamExpiredEvent(event) => {
                    match event.event_type.to_string().as_str() {
                        "TRADE_LITE" => {
                            //binance new event type, ignore
                        }
                        _ => {
                            dbg!(event);
                            println!("keep_user_stream_alive");
                            keep_user_stream_alive();
                        }
                    }
                }
                FuturesWebsocketEvent::OrderTrade(event) => {
                    traders
                        .get_mut(&event.order.symbol)
                        .unwrap()
                        .lock()
                        .unwrap()
                        .on_trade_update(event);
                }
                FuturesWebsocketEvent::AccountUpdate(account_update_event) => {}
                _ => {
                    dbg!(&event);
                    panic!("unexpected callbackevent");
                }
            }
            Ok(())
        }
    };

    let mut web_socket = FuturesWebSockets::new(callback_fn);

    let listen_key = keep_user_stream_alive();
    dbg!(&listen_key);
    streams.push(listen_key);

    println!("listening to streams {:#?}", &streams);
    let config = get_config();
    web_socket
        .connect_multiple_streams_with_config(&streams, config)
        .expect("bug");

    match web_socket.event_loop(&keep_running).unwrap_err() {
        err => match err.0.description() {
            "running loop closed" => {}
            _ => {
                dbg!(err);
                panic!("web_socket.event_loop has unhandled err");
            }
        },
    }
}
