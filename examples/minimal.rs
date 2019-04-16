use encore::*;

fn main() {
    let console = Console::new(
        || |input: &str| input.to_string(),
        |output| {
            if output == "quit" {
                None
            } else {
                Some(output.to_uppercase())
            }
        },
    );
    while console.is_open() {
        if let Some(s) = console.poll() {
            println!("{}", s);
        }
    }
}
