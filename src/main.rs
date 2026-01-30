use std::collections::HashSet;
use std::fs::{DirEntry, File};
use std::io::{self, Stderr, Stdout, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, exit};
use std::{
    env::{set_current_dir, split_paths, var},
    ffi::OsStr,
};

use rustyline::history::FileHistory;
use rustyline::{CompletionType, Config, Editor};

mod helper;

use crate::helper::MyHelper;

enum OutputDirection {
    File(File),
    Stdout(Stdout),
}

enum ErrDirection {
    File(File),
    Stderr(Stderr),
}

impl Write for OutputDirection {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            OutputDirection::File(file) => file.write(buf),
            OutputDirection::Stdout(stdout) => stdout.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            OutputDirection::File(file) => file.flush(),
            OutputDirection::Stdout(stdout) => stdout.flush(),
        }
    }
}

impl Write for ErrDirection {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            ErrDirection::File(file) => file.write(buf),
            ErrDirection::Stderr(stderr) => stderr.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            ErrDirection::File(file) => file.flush(),
            ErrDirection::Stderr(stderr) => stderr.flush(),
        }
    }
}

fn is_executable(dir_entry: &DirEntry) -> bool {
    if let Ok(metadata) = dir_entry.metadata()
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
                } else if escaped {
                    arg_buffer.push(c);
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

    if !arg_buffer.is_empty() {
        result.push(arg_buffer);
    }

    result
}

fn main_loop(
    editor: &mut Editor<MyHelper, FileHistory>,
    builtins: &HashSet<&str>,
    executable_paths: &[&DirEntry],
) -> Result<(), io::Error> {
    let readline = match editor.readline("$ ") {
        Ok(x) => x,
        Err(err) => panic!("{err:?}"),
    };

    let mut user_inputs = read_user_input(&readline);
    let possible_file_name = user_inputs.pop();
    let possible_redirect_operator = user_inputs.pop();

    let (mut output_direction, mut err_direction) = match (
        possible_redirect_operator.as_deref(),
        possible_file_name.as_ref(),
    ) {
        (Some(">" | "1>"), Some(file_name)) => (
            OutputDirection::File(File::create(file_name)?),
            ErrDirection::Stderr(io::stderr()),
        ),
        (Some(">>" | "1>>"), Some(file_name)) => (
            OutputDirection::File(File::options().append(true).create(true).open(file_name)?),
            ErrDirection::Stderr(io::stderr()),
        ),
        (Some("2>"), Some(file_name)) => (
            OutputDirection::Stdout(io::stdout()),
            ErrDirection::File(File::create(file_name)?),
        ),
        (Some("2>>"), Some(file_name)) => (
            OutputDirection::Stdout(io::stdout()),
            ErrDirection::File(File::options().append(true).create(true).open(file_name)?),
        ),
        _ => {
            if let Some(x) = possible_redirect_operator {
                user_inputs.push(x);
            }

            if let Some(x) = possible_file_name {
                user_inputs.push(x);
            }

            (
                OutputDirection::Stdout(io::stdout()),
                ErrDirection::Stderr(io::stderr()),
            )
        }
    };

    match user_inputs.first().map(String::as_str) {
        Some("exit") => exit(0),
        Some("echo") => writeln!(output_direction, "{}", user_inputs[1..].join(" ")),
        Some("type") => {
            if let Some(command) = user_inputs.get(1) {
                if builtins.contains(command.as_str()) {
                    writeln!(output_direction, "{command} is a shell builtin")
                } else if let Some(dir_entry) = executable_paths
                    .iter()
                    .find(|dir_entry| dir_entry.file_name() == command.as_str())
                    && let Some(path) = dir_entry.path().to_str()
                {
                    writeln!(output_direction, "{} is {}", command, path)
                } else {
                    writeln!(err_direction, "{}: not found", command)
                }
            } else {
                Ok(())
            }
        }
        Some("pwd") => writeln!(
            output_direction,
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
                    err_direction,
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
                    write!(output_direction, "{}", String::from_utf8_lossy(&out.stdout))?;
                    write!(err_direction, "{}", String::from_utf8_lossy(&out.stderr))
                }
                Err(_) => {
                    println!("{command}: not found");
                    Ok(())
                }
            }
        }
        _ => Ok(()),
    }
}

fn main() -> rustyline::Result<()> {
    let path_variable: Vec<DirEntry> = var("PATH")
        .map(|paths| {
            split_paths(&paths)
                .filter_map(|x| Path::new(&x).read_dir().ok())
                .flat_map(Iterator::flatten)
                .collect()
        })
        .unwrap_or_default();

    let builtins = HashSet::from(["exit", "echo", "type", "pwd", "cd"]);

    let executable_paths = path_variable
        .iter()
        .filter(|x| is_executable(x))
        .collect::<Vec<_>>();

    let executable_names = executable_paths
        .iter()
        .filter_map(|x| x.file_name().into_string().ok())
        .collect::<Vec<_>>();

    let config = Config::builder()
        .completion_type(CompletionType::List)
        .build();

    let mut editor: Editor<MyHelper, _> = Editor::with_config(config)?;
    editor.set_helper(Some(MyHelper::from(
        builtins
            .iter()
            .cloned()
            .chain(executable_names.iter().map(String::as_str)),
    )));

    loop {
        let _ = main_loop(&mut editor, &builtins, &executable_paths);
    }
}
