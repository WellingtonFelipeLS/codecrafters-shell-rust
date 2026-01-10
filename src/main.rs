#[allow(unused_imports)]
use std::io::{self, Write};

fn main() {
    let mut buffer = String::new();
    let mut trimmer_buffer;

    loop {
        buffer.clear();

        print!("$ ");
        io::stdout().flush().unwrap();

        let _ = io::stdin().read_line(&mut buffer);

        trimmer_buffer = buffer.trim();

        if trimmer_buffer == "exit" {
            return;
        }

        println!("{}: command not found", trimmer_buffer);
    }
}
