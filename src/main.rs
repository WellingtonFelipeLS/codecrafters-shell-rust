#[allow(unused_imports)]
use std::io::{self, Write};

fn main() {
    let mut buffer = String::new();
    let mut user_inputs: Vec<&str>;

    loop {
        buffer.clear();

        print!("$ ");
        io::stdout().flush().unwrap();

        let _ = io::stdin().read_line(&mut buffer);

        user_inputs = buffer.split_whitespace().collect();

        match user_inputs.first() {
            Some(&"exit") => return,
            Some(&"echo") => println!("{}", user_inputs[1..].join(" ")),
            Some(x) => println!("{x}: command not found"),
            _ => println!(": command not found"),
        }
    }
}
