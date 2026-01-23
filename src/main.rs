use std::collections::HashSet;
use std::env::{set_current_dir, split_paths, var};
use std::ffi::OsStr;
use std::fs::{DirEntry, File};
use std::io::{self, Stdout, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, exit};

enum Output {
    File(File),
    Stdout(Stdout),
}

impl Write for Output {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Output::File(file) => file.write(buf),
            Output::Stdout(stdout) => stdout.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Output::File(file) => file.flush(),
            Output::Stdout(stdout) => stdout.flush(),
        }
    }
}

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
            '\\' => {
                if open_single_quote {
                    arg_buffer.push(c);
                } else if open_double_quote {
                    if escaped {
                        arg_buffer.push(c);
                        escaped = false;
                    } else {
                        escaped = true;
                    }
                } else {
                    escaped = true;
                }
            }
            '\'' => {
                if open_double_quote {
                    if escaped {
                        arg_buffer.push('\\');
                        escaped = false;
                    }
                    arg_buffer.push(c);
                } else if escaped {
                    arg_buffer.push(c);
                    escaped = false;
                } else {
                    open_single_quote = !open_single_quote;
                }
            }
            '\"' => {
                if open_single_quote {
                    arg_buffer.push(c);
                } else if open_double_quote {
                    if escaped {
                        arg_buffer.push(c);
                        escaped = false;
                    } else {
                        open_double_quote = false;
                    }
                } else if escaped {
                    arg_buffer.push(c);
                    escaped = false;
                } else {
                    open_double_quote = !open_double_quote;
                }
            }
            x if x.is_whitespace() => {
                if open_single_quote {
                    arg_buffer.push(x);
                } else if open_double_quote {
                    if escaped {
                        arg_buffer.push('\\');
                        escaped = false;
                    }
                    arg_buffer.push(x);
                } else if escaped {
                    arg_buffer.push(x);
                    escaped = false;
                } else if !arg_buffer.is_empty() {
                    result.push(arg_buffer.clone());
                    arg_buffer.clear();
                }
            }
            x => {
                if open_double_quote && escaped {
                    arg_buffer.push('\\');
                }

                arg_buffer.push(x);
                escaped = false;
            }
        }
    }

    result
}

fn main_loop(
    buffer: &mut String,
    path_variable: &[DirEntry],
    builtins: &HashSet<&str>,
) -> Result<(), io::Error> {
    buffer.clear();
    print!("$ ");
    io::stdout().flush().unwrap();

    let _ = io::stdin().read_line(buffer);

    let mut user_inputs = read_user_input(buffer);
    let possible_file_name = user_inputs.pop();
    let possible_redirect_operator = user_inputs.pop();

    let mut output = if let (Some(">" | "1>"), Some(file_name)) = (
        possible_redirect_operator.as_deref(),
        possible_file_name.as_ref(),
    ) {
        Output::File(File::create(file_name)?)
    } else {
        if let Some(x) = possible_redirect_operator {
            user_inputs.push(x);
        }

        if let Some(x) = possible_file_name {
            user_inputs.push(x);
        }

        Output::Stdout(io::stdout())
    };

    match user_inputs.first().map(String::as_str) {
        Some("exit") => exit(0),
        Some("echo") => writeln!(output, "{}", user_inputs[1..].join(" ")),
        Some("type") => {
            if let Some(command) = user_inputs.get(1) {
                if builtins.contains(command.as_str()) {
                    writeln!(output, "{command} is a shell builtin")
                } else if let Some(dir_entry) = path_variable
                    .iter()
                    .find(|dir_entry| is_executable_with_name(dir_entry, command))
                    && let Some(path) = dir_entry.path().to_str()
                {
                    writeln!(output, "{} is {}", command, path)
                } else {
                    writeln!(output, "{}: not found", command)
                }
            } else {
                Ok(())
            }
        }
        Some("pwd") => writeln!(
            output,
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
                writeln!(
                    output,
                    "cd: {}: No such file or directory",
                    path.to_str().expect("Should be valid UTF-8")
                )
            } else {
                Ok(())
            }
        }
        Some(command) => {
            match Command::new(command)
                .args(user_inputs.iter().skip(1).map(OsStr::new))
                .output()
            {
                Ok(out) => {
                    let out = if let Some(0) = out.status.code() {
                        out.stdout
                    } else {
                        out.stderr
                    };

                    write!(output, "{}", String::from_utf8_lossy(&out))
                }
                Err(_) => writeln!(output, "{command}: not found"),
            }
        }
        _ => Ok(()),
    }
}

fn main() {
    let mut buffer = String::new();

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
        let _ = main_loop(&mut buffer, &path_variable, &builtins);
    }
}
