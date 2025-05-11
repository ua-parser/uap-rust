use clap::Parser;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Args {
        regexes,
        user_agents,
        repetitions,
        quiet,
    } = Args::parse();

    let start = std::time::Instant::now();
    let regexes = BufReader::new(std::fs::File::open(regexes)?)
        .lines()
        .collect::<Result<Vec<String>, _>>()?;

    let f = regex_filtered::Builder::new().push_all(&regexes)?.build()?;
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

    for _ in 0..repetitions {
        for ua in user_agents.iter() {
            let n = f.matching(ua).next();
            if !quiet {
                if let Some((n, _)) = n {
                    println!("{n:3}");
                } else {
                    println!();
                }
            }
        }
    }

    Ok(())
}
