use clap::Parser;

#[derive(Parser)]
struct Args {
    atomsize: usize,
    regex: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Args { atomsize, regex } = Args::parse();
    let hir = regex_syntax::parse(&regex)?;
    let mut mb = regex_filtered::mapper::Builder::new(atomsize);
    mb.push(regex_filtered::model::Model::new(&hir)?);
    let (_, atoms) = mb.build();

    for atom in atoms {
        println!("{atom}");
    }

    Ok(())
}
