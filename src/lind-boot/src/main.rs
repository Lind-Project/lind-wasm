use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "lind-boot", version, author)]
struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[arg(short, long)]
    config: Option<PathBuf>,

    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum Format {
    Text,
    Json,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Sum {
        nums: Vec<i64>,
    },
}

fn main() {
    let cli = Cli::parse();

    if cli.verbose > 0 {
        eprintln!(
            "[debug] verbose={} config={:?} format={:?}",
            cli.verbose, cli.config, cli.format
        );
    }

    match cli.command {
        Commands::Greet { name, times } => {
            for _ in 0..times {
                match cli.format {
                    Format::Text => println!("Hello, {}!", name),
                    Format::Json => println!(r#"{{"msg":"Hello, {}!"}}"#, name),
                }
            }
        }
        Commands::Sum { nums } => {
            let sum: i64 = nums.iter().sum();
            match cli.format {
                Format::Text => println!("sum = {}", sum),
                Format::Json => println!(r#"{{"sum": {}}}"#, sum),
            }
        }
    }
}
