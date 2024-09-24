use binance::{
    api::Binance,
    futures::{account::FuturesAccount, general::FuturesGeneral},
};

use crate::subscriber::{get_config, Keys};

pub fn get_futures_account() -> (FuturesAccount, FuturesGeneral) {
    let keys = Keys::new();
    let general = FuturesGeneral::new_with_config(
        Some(keys.clone().api_key),
        Some(keys.clone().secret_key),
        &get_config(),
    );
    let account =
        Binance::new_with_config(Some(keys.api_key), Some(keys.secret_key), &get_config());
    (account, general)
}

pub trait Round {
    fn round_to_n(self, decimal_places: i32) -> f64;
}

impl Round for f64 {
    fn round_to_n(self, decimal_places: i32) -> f64 {
        let mult = 10_f64.powi(decimal_places);
        (self * mult).round() / mult
    }
}
