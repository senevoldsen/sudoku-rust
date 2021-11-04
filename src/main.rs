pub mod lib;

use clap::{App, Arg};
use std::time::Instant;

use lib::{parse_grid, solve_recursive, solve_recursive_par};

fn main() -> Result<(), String> {
    let matches = App::new("Sudoku solver")
        .version("0.1")
        .about("Solves Sudokus")
        .arg(
            Arg::new("parallel")
                .short('p')
                .long("parallel")
                .takes_value(false),
        )
        .arg(
            Arg::new("input_file")
                .about("Sets the input source file")
                .required(true)
                .value_name("FILE")
                .index(1),
        )
        .get_matches();
    let filename: &str = matches.value_of("input_file").unwrap();
    let run_parallel = matches.is_present("parallel");

    // Load from file path
    let file_content = std::fs::read_to_string(filename).map_err(|e| e.to_string())?;
    let grid = parse_grid(&file_content).ok_or("Unable to parse Sudoku grid from file")?;

    if run_parallel {
        println!("Using parallism");
    }
    println!("Grid Input:\n{}", grid);

    let start_time = Instant::now();

    let solved = if run_parallel {
        solve_recursive_par(grid)
    } else {
        solve_recursive(grid)
    };

    println!("Time elapsed [ms]: {}", start_time.elapsed().as_millis());

    match solved {
        Some(solved_grid) => {
            println!("One solution is\n{}", solved_grid);
        }
        None => {
            println!("Unable to solve puzzle");
        }
    };

    Ok(())
}
