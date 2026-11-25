use clap::Parser;
use std::io::BufRead;

#[derive(Parser)]
struct Args {
    regexes: String,
    useragents: String,
}

fn main() {
    let Args {
        regexes,
        useragents,
    } = Args::parse();
    let regexes: Vec<_> = std::io::BufReader::new(std::fs::File::open(regexes).unwrap())
        .lines()
        .map(|l| regex::Regex::new(&l.unwrap()).unwrap())
        .collect();

    let mut uas = std::io::BufReader::new(std::fs::File::open(useragents).unwrap());
    let mut line = String::with_capacity(150);
    while let Ok(n) = uas.read_line(&mut line) {
        if n == 0 {
            break;
        }
        let line_ = line.strip_suffix("\n").unwrap_or(&line);
        let m = regexes
            .iter()
            .enumerate()
            .find(|(_, regex)| regex.is_match(line_));
        if let Some((i, _)) = m {
            println!("{i}");
        } else {
            println!("-");
        }
        line.clear();
    }
}
