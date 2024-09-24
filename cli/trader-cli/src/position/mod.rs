use pyo3::prelude::*;

#[derive(Debug, serde::Deserialize, Clone)]
#[pyclass]
pub struct Position {
    // ---
    // if you want to liquidate a position, overstate the notional
    // tested with notional value at 2x the current position and has run fine
    // ---
    #[pyo3(get, set)]
    pub symbol: String,

    #[pyo3(get, set)]
    pub notional: f64,

    #[pyo3(get, set)]
    pub reduce_only: bool,
}

#[pymethods]
impl Position {
    #[new]
    fn new(symbol: String, notional: f64, reduce_only: bool) -> Self {
        Position {
            symbol: symbol,
            notional: notional,
            reduce_only: reduce_only,
        }
    }
}
