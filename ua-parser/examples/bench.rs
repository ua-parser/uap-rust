use clap::Parser;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

#[derive(Parser, Debug)]
struct Args {
    /// regexes.yaml file to parse the data file with
    regexes: PathBuf,
    /// user agents file
    user_agents: PathBuf,
    /// number of repetitions through the user agent file
    #[arg(short, long, default_value_t = 1)]
    repetitions: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Args {
        regexes,
        user_agents,
        repetitions,
    } = Args::parse();

    let f = std::fs::File::open(regexes)?;
    let r = ua_parser::Extractor::try_from(serde_yaml::from_reader::<_, ua_parser::Regexes>(f)?)?;

    let uas = BufReader::new(std::fs::File::open(user_agents)?)
        .lines()
        .collect::<Result<Vec<String>, _>>()?;

    let duration = std::time::Instant::now();
    for _ in 0..repetitions {
        for ua in &uas {
            drop(r.extract(ua));
        }
    }

    let elapsed = duration.elapsed();
    println!("Lines: {}", repetitions * uas.len());
    println!("Total time: {elapsed:?}");
    println!(
        "{}µs / line",
        elapsed.as_micros() / (repetitions * uas.len()) as u128
    );

    Ok(())
}
