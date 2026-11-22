use clap::Parser;
use regex::Regex;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

type Error = Box<dyn std::error::Error>;

#[derive(Parser)]
struct Args {
    /// regexes file (one per line)
    regexes: PathBuf,
    /// user agents (one per line)
    user_agents: PathBuf,
    #[arg(short, long, default_value_t = 1)]
    repetitions: usize,
    #[arg(short, long, default_value_t = false)]
    quiet: bool,
}

fn main() -> Result<(), Error> {
    let Args {
        regexes,
        user_agents,
        repetitions,
        quiet,
    } = Args::parse();

    let start = std::time::Instant::now();
    let regexes = BufReader::new(std::fs::File::open(regexes)?)
        .lines()
        .map(|r| Ok(Regex::new(&r?)?))
        .collect::<Result<Vec<Regex>, Error>>()?;

    eprintln!(
        "{} regexes in {}s",
        regexes.len(),
        start.elapsed().as_secs_f32()
    );

    let start = std::time::Instant::now();
    let user_agents = BufReader::new(std::fs::File::open(user_agents)?)
        .lines()
        .collect::<Result<Vec<String>, _>>()?;
    eprintln!(
        "{} user agents in {}s",
        user_agents.len(),
        start.elapsed().as_secs_f32()
    );

    let start = std::time::Instant::now();
    for _ in 0..repetitions {
        for ua in user_agents.iter() {
            let n = regexes.iter().position(|r| r.is_match(ua));
            if !quiet {
                if let Some(n) = n {
                    println!("{n:3}");
                } else {
                    println!();
                }
            }
        }
    }
    eprintln!("executed in {}s", start.elapsed().as_secs_f32());

    Ok(())
}
