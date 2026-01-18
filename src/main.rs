use std::collections::HashSet;
use std::env::{set_current_dir, split_paths, var};
use std::ffi::OsStr;
use std::fs::DirEntry;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
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

fn read_user_input(buffer: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut open_single_quote = false;
    let mut open_double_quote = false;
    let mut escaped = false;
    let mut arg_buffer = String::new();

    for c in buffer.chars() {
        match c {
            x if escaped => {
                arg_buffer.push(x);
                escaped = false;
            }
            '\\' => {
                if open_single_quote || open_double_quote {
                    arg_buffer.push(c);
                } else {
                    escaped = true
                }
            }
            '\'' => {
                if open_double_quote {
                    arg_buffer.push(c);
                } else {
                    open_single_quote = !open_single_quote;
                }
            }
            '\"' => {
                if open_single_quote {
                    arg_buffer.push(c);
                } else {
                    open_double_quote = !open_double_quote;
                }
            }
            x if x.is_whitespace() => {
                if open_single_quote || open_double_quote {
                    arg_buffer.push(x);
                } else if !arg_buffer.is_empty() {
                    result.push(arg_buffer.clone());
                    arg_buffer.clear();
                }
            }
            x => {
                arg_buffer.push(x);
            }
        }
    }

    result
}

fn main() {
    let mut buffer = String::new();
    let mut user_inputs;

    let path_variable: Vec<DirEntry> = var("PATH")
        .map(|paths| {
            split_paths(&paths)
                .filter_map(|x| Path::new(&x).read_dir().ok())
                .flat_map(Iterator::flatten)
                .collect()
        })
        .unwrap_or_default();

    let builtins = HashSet::from(["exit", "echo", "type", "pwd", "cd"]);

    loop {
        buffer.clear();

        print!("$ ");
        io::stdout().flush().unwrap();

        let _ = io::stdin().read_line(&mut buffer);

        user_inputs = read_user_input(&buffer);

        match user_inputs.first().map(String::as_str) {
            Some("exit") => return,
            Some("echo") => println!("{}", user_inputs[1..].join(" ")),
            Some("type") => {
                if let Some(command) = user_inputs.get(1) {
                    if builtins.contains(command.as_str()) {
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
            Some("pwd") => println!(
                "{}",
                std::env::current_dir()
                    .expect("Should be a valid working directory")
                    .to_str()
                    .expect("Should be valid UTF-8")
            ),
            Some("cd") => {
                let path = if let Some(path) = user_inputs.get(1)
                    && *path != "~"
                {
                    PathBuf::from(path)
                } else {
                    std::env::home_dir().expect("Home directory env variable should be set")
                };

                if set_current_dir(&path).is_err() {
                    println!(
                        "cd: {}: No such file or directory",
                        path.to_str().expect("Should be valid UTF-8")
                    );
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
