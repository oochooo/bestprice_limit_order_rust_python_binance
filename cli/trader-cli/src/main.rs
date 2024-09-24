mod position;
mod subscriber;
mod trader;
mod utils;

use core::panic;

use clap::{Parser, Subcommand};
use position::Position;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Trade {
        #[arg(
            // long,
            short,
            value_parser(parse_positions_from_input),
            required = true
        )]
        positions: Vec<Position>,

        #[arg(long, required = true)]
        live: bool,
    },
}

fn parse_positions_from_input(val: &str) -> Result<Position, String> {
    let positions: Position = serde_json::from_str(val).unwrap();
    Ok(positions)
}

fn main() {
    panic!("untested");
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Trade { positions, live }) => {
            // dbg!(&cli.command);
            dbg!(&live);
            // trader::run(positions.clone());
        }
        None => {
            panic!("you did not pass an argument")
        }
    }
}
