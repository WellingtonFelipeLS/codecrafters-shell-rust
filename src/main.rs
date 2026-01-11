use std::env::{split_paths, var};
use std::ffi::OsStr;
use std::fs::DirEntry;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

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
            Some(command) => {
                match Command::new(command)
                    .args(user_inputs.iter().skip(1).map(OsStr::new))
                    .output()
                {
                    Ok(output) => print!("{}", String::from_utf8_lossy(&output.stdout)),
                    Err(_) => println!("{command}: not found"),
                }
            }
            _ => (),
        }
    }
}
