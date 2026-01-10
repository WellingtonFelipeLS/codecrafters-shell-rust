#[allow(unused_imports)]
use std::io::{self, Write};

fn main() {
    let mut buffer = String::new();

    loop {
        print!("$ ");
        io::stdout().flush().unwrap();

        let _ = io::stdin().read_line(&mut buffer);

        println!("{}: command not found", buffer.trim());

        buffer.clear();
    }
}
