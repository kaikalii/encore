use encore::{clap::*, *};

fn main() {
    // A function that builds the app
    let app = || {
        App::new("example").about("This is an example app").arg(
            Arg::with_name("INPUT")
                .help("The input string")
                .required(true),
        )
    };

    // Create the console
    let console = Console::new(app, |matches| match matches {
        Ok(matches) => {
            let input = matches.value_of("INPUT").unwrap();
            if input == "quit" {
                None
            } else {
                Some(input.to_uppercase())
            }
        }
        Err(e) => Some(format!("{}", e)),
    });

    // Poll and print the output
    while console.is_open() {
        if let Some(output) = console.poll() {
            println!("{}", output);
        }
    }
}
