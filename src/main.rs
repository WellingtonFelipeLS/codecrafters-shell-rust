use std::io::{self, PipeReader, PipeWriter, Read, Stderr, Stdout, Write, pipe, stdout};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio, exit};
use std::{
    collections::HashSet,
    fs::{DirEntry, File},
};
use std::{
    env::{set_current_dir, split_paths, var},
    ffi::OsStr,
};

use rustyline::config::BellStyle;
use rustyline::history::FileHistory;
use rustyline::{CompletionType, Config, Editor};

mod helper;

use crate::helper::MyHelper;

enum OutputDirection {
    File(File),
    PipeWriter(PipeWriter),
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
            OutputDirection::PipeWriter(pipe_writer) => pipe_writer.write(buf),
            OutputDirection::Stdout(stdout) => stdout.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            OutputDirection::File(file) => file.flush(),
            OutputDirection::PipeWriter(pipe_writer) => pipe_writer.flush(),
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

impl From<OutputDirection> for Stdio {
    fn from(value: OutputDirection) -> Self {
        match value {
            OutputDirection::File(file) => file.into(),
            OutputDirection::PipeWriter(pipe_writer) => pipe_writer.into(),
            OutputDirection::Stdout(stdout) => stdout.into(),
        }
    }
}

impl From<ErrDirection> for Stdio {
    fn from(value: ErrDirection) -> Self {
        match value {
            ErrDirection::File(file) => file.into(),
            ErrDirection::Stderr(stderr) => stderr.into(),
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

fn verify_out_and_err_direction(
    user_inputs: &mut Vec<String>,
    pipe_writer: Option<PipeWriter>,
) -> io::Result<(OutputDirection, ErrDirection)> {
    let possible_file_name = user_inputs.pop();
    let possible_redirect_operator = user_inputs.pop();

    let output_direction = match pipe_writer {
        Some(pipe_writer) => OutputDirection::PipeWriter(pipe_writer),
        None => OutputDirection::Stdout(stdout()),
    };

    let result = match (
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
            output_direction,
            ErrDirection::File(File::create(file_name)?),
        ),
        (Some("2>>"), Some(file_name)) => (
            output_direction,
            ErrDirection::File(File::options().append(true).create(true).open(file_name)?),
        ),
        _ => {
            if let Some(x) = possible_redirect_operator {
                user_inputs.push(x);
            }

            if let Some(x) = possible_file_name {
                user_inputs.push(x);
            }

            (output_direction, ErrDirection::Stderr(io::stderr()))
        }
    };

    Ok(result)
}

fn call_command_with_args<I>(
    mut user_inputs: Vec<String>,
    builtins: &HashSet<&str>,
    executable_paths: &[&DirEntry],
    input_reader: Option<I>,
    children: &mut Vec<Child>,
    idx: usize,
    len: usize,
) -> Result<Option<PipeReader>, io::Error>
where
    I: Into<Stdio> + Read,
{
    let (pipe_reader, pipe_writer) = if idx == len - 1 {
        (None, None)
    } else {
        let (a, b) = pipe()?;
        (Some(a), Some(b))
    };

    let (mut output_direction, mut err_direction) =
        verify_out_and_err_direction(&mut user_inputs, pipe_writer)?;

    match user_inputs.first().map(String::as_str) {
        Some("exit") => exit(0),
        Some("echo") => {
            writeln!(output_direction, "{}", user_inputs[1..].join(" "))?;
        }
        Some("type") => {
            if let Some(command) = user_inputs.get(1) {
                if builtins.contains(command.as_str()) {
                    writeln!(output_direction, "{command} is a shell builtin")?;
                } else if let Some(dir_entry) = executable_paths
                    .iter()
                    .find(|dir_entry| dir_entry.file_name() == command.as_str())
                    && let Some(path) = dir_entry.path().to_str()
                {
                    writeln!(output_direction, "{} is {}", command, path)?;
                } else {
                    writeln!(err_direction, "{}: not found", command)?;
                }
            }
        }
        Some("pwd") => {
            writeln!(
                output_direction,
                "{}",
                std::env::current_dir()
                    .expect("Should be a valid working directory")
                    .to_str()
                    .expect("Should be valid UTF-8")
            )?;
        }
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
                )?;
            }
        }
        Some(command) => {
            let mut child_command = Command::new(command);

            child_command.args(user_inputs.iter().skip(1).map(OsStr::new));

            if idx != 0 {
                child_command.stdin(input_reader.unwrap());
            }

            child_command.stdout(output_direction).stderr(err_direction);

            match child_command.spawn() {
                Ok(child) => {
                    children.push(child);
                }
                Err(_) => {
                    println!("{command}: not found");
                }
            }
        }
        _ => (),
    };

    Ok(pipe_reader)
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

    let mut processed_user_inputs = Vec::new();

    let last_input = read_user_input(&readline)
        .into_iter()
        .fold(Vec::new(), |mut acc, x| {
            if x.as_str() == "|" {
                processed_user_inputs.push(acc);
                Vec::new()
            } else {
                acc.push(x);
                acc
            }
        });

    processed_user_inputs.push(last_input);

    let len = processed_user_inputs.len();

    let mut children = Vec::new();

    processed_user_inputs.into_iter().enumerate().try_fold(
        None,
        |input_reader, (idx, user_inputs)| {
            call_command_with_args(
                user_inputs,
                builtins,
                executable_paths,
                input_reader,
                &mut children,
                idx,
                len,
            )
        },
    )?;

    children
        .into_iter()
        .rev()
        .try_for_each(|mut child| child.wait().map(|_| ()))?;

    Ok(())
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
        .bell_style(BellStyle::Audible)
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
