use std::env::{split_paths, var};
use std::fs::DirEntry;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn is_executable_with_name(dir_entry: &DirEntry, name: &str) -> bool {
    if dir_entry.file_name() == name
        && let Ok(metadata) = dir_entry.metadata()
        && metadata.permissions().mode() & 0o001 == 1
    {
        true
    } else {
        false
    }
}

fn main() {
    let mut buffer = String::new();
    let mut user_inputs: Vec<&str>;

    let path_variable: Vec<DirEntry> = var("PATH")
        .map(|paths| {
            split_paths(&paths)
                .filter_map(|x| Path::new(&x).read_dir().ok())
                .flat_map(Iterator::flatten)
                .collect()
        })
        .unwrap_or_default();

    let builtins = ["exit", "echo", "type"];

    loop {
        buffer.clear();

        print!("$ ");
        io::stdout().flush().unwrap();

        let _ = io::stdin().read_line(&mut buffer);

        user_inputs = buffer.split_whitespace().collect();

        match user_inputs.first() {
            Some(&"exit") => return,
            Some(&"echo") => println!("{}", user_inputs[1..].join(" ")),
            Some(&"type") => {
                if let Some(command) = user_inputs.get(1) {
                    if builtins.contains(command) {
                        println!("{command} is a shell builtin")
                    } else if let Some(dir_entry) = path_variable
                        .iter()
                        .find(|dir_entry| is_executable_with_name(dir_entry, command))
                        && let Some(path) = dir_entry.path().to_str()
                    {
                        println!("{command} is {path}",);
                    } else {
                        println!("{command}: not found");
                    }
                }
            }
            Some(x) => println!("{x}: command not found"),
            _ => (),
        }
    }
}
