use position::Position;
use pyo3::prelude::*;
use trader::run_binance;

mod position;
mod subscriber;
mod trader;
mod utils;

#[pymodule]
#[pyo3(name = "rust_trader")]
fn my_extension(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(run_binance, m)?)?;
    m.add_class::<Position>()?;
    Ok(())
}
