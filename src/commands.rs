use crate::utils;
use rustyline::history::{self, History};
use std::{
    collections::HashSet,
    env, ffi, fs,
    io::{self, Read, Write},
    path, process,
};

pub fn call_command_with_args<I>(
    mut user_inputs: Vec<String>,
    builtins: &HashSet<&str>,
    executable_paths: &[&fs::DirEntry],
    history: &mut history::FileHistory,
    input_reader: Option<I>,
    children: &mut Vec<process::Child>,
    process_position: utils::ProcessPosition,
) -> io::Result<Option<io::PipeReader>>
where
    I: Into<process::Stdio> + Read,
{
    let (pipe_reader, pipe_writer) = if process_position.is_last() {
        (None, None)
    } else {
        let (a, b) = io::pipe()?;
        (Some(a), Some(b))
    };

    let (output_direction, err_direction) =
        utils::verify_out_and_err_direction(&mut user_inputs, pipe_writer)?;

    match user_inputs.first().map(String::as_str) {
        Some("exit") => exit(history),
        Some("echo") => echo(output_direction, &user_inputs[1..]),
        Some("type") => type_command(
            output_direction,
            err_direction,
            &user_inputs[1..],
            builtins,
            executable_paths,
        ),
        Some("pwd") => pwd(output_direction),
        Some("cd") => cd(&user_inputs[1..], err_direction),
        Some("history") => {
            history_command(&user_inputs[1..], history, output_direction, err_direction)
        }
        Some("declare") => declare(&user_inputs[1..], err_direction),
        Some(command) => {
            if let Some(child) = command_exec(
                command,
                &user_inputs[1..],
                process_position,
                input_reader,
                output_direction,
                err_direction,
            ) {
                children.push(child)
            } else {
                println!("{command}: not found")
            }

            Ok(())
        }
        _ => Ok(()),
    }
    .map(|_| pipe_reader)
}

fn exit(history: &mut history::FileHistory) -> io::Result<()> {
    if let Some(history_filepath) =
        env::vars().find_map(|(k, v)| if k == "HISTFILE" { Some(v) } else { None })
    {
        let history_filepath = path::Path::new(&history_filepath);

        let _ = history.append(history_filepath);

        let new_content = std::fs::read_to_string(history_filepath)?
            .lines()
            .filter(|line| line != &"#V2")
            .fold(String::new(), |acc, x| acc + x + "\n");

        fs::write(history_filepath, new_content)?;
    }

    process::exit(0);
}

fn echo(mut output_direction: utils::OutputDirection, user_inputs: &[String]) -> io::Result<()> {
    writeln!(output_direction, "{}", user_inputs.join(" "))
}

fn type_command(
    mut output_direction: utils::OutputDirection,
    mut err_direction: utils::ErrDirection,
    user_inputs: &[String],
    builtins: &HashSet<&str>,
    executable_paths: &[&fs::DirEntry],
) -> Result<(), io::Error> {
    if let Some(command) = user_inputs.first() {
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
    };

    Ok(())
}

fn pwd(mut output_direction: utils::OutputDirection) -> io::Result<()> {
    writeln!(
        output_direction,
        "{}",
        std::env::current_dir()
            .expect("Should be a valid working directory")
            .to_str()
            .expect("Should be valid UTF-8")
    )
}

fn cd(user_inputs: &[String], mut err_direction: utils::ErrDirection) -> io::Result<()> {
    let path = if let Some(path) = user_inputs.first()
        && *path != "~"
    {
        path::PathBuf::from(path)
    } else {
        std::env::home_dir().expect("Home directory env variable should be set")
    };

    env::set_current_dir(&path).or_else(|_| {
        writeln!(
            err_direction,
            "cd: {}: No such file or directory",
            path.to_str().expect("Should be valid UTF-8")
        )
    })
}

fn history_command(
    user_inputs: &[String],

    history: &mut history::FileHistory,
    mut output_direction: utils::OutputDirection,
    mut err_direction: utils::ErrDirection,
) -> io::Result<()> {
    match user_inputs.first().map(String::as_str) {
        None => history
            .iter()
            .zip(1..)
            .try_for_each(|(line, idx)| writeln!(output_direction, "{idx} {line}")),
        Some("-r") => {
            if let Some(path) = user_inputs.get(1).map(path::Path::new) {
                if history.load(path).is_err() {
                    writeln!(err_direction, "history: invalid file path")
                } else {
                    Ok(())
                }
            } else {
                writeln!(err_direction, "history: invalid file path")
            }
        }
        Some("-w") => {
            if let Some(path) = user_inputs.get(1).map(path::Path::new) {
                let _ = history.save(path);

                let new_content = std::fs::read_to_string(path)?
                    .lines()
                    .filter(|line| line != &"#V2")
                    .fold(String::new(), |acc, x| acc + x + "\n");

                fs::write(path, new_content)
            } else {
                writeln!(err_direction, "history: invalid file path")
            }
        }
        Some("-a") => {
            if let Some(path) = user_inputs.get(1).map(path::Path::new) {
                let _ = history.append(path);

                let new_content = std::fs::read_to_string(path)?
                    .lines()
                    .filter(|line| line != &"#V2")
                    .fold(String::new(), |acc, x| acc + x + "\n");

                fs::write(path, new_content)
            } else {
                writeln!(err_direction, "history: invalid file path")
            }
        }
        Some(x) => {
            if let Ok(number) = x.parse::<usize>() {
                history
                    .iter()
                    .zip(1..)
                    .skip(history.len().saturating_sub(number))
                    .try_for_each(|(line, idx)| writeln!(output_direction, "{idx} {line}"))
            } else {
                writeln!(err_direction, "history: invalid number")
            }
        }
    }
}

fn declare(user_inputs: &[String], mut err_direction: utils::ErrDirection) -> io::Result<()> {
    match user_inputs.first().map(String::as_str) {
        Some("-p") if let Some(variable) = user_inputs.get(1) => {
            writeln!(err_direction, "declare: {variable}: not found")
        }
        _ => todo!(),
    }
}

fn command_exec<I>(
    command: &str,
    user_inputs: &[String],
    process_position: utils::ProcessPosition,
    input_reader: Option<I>,
    output_direction: utils::OutputDirection,
    err_direction: utils::ErrDirection,
) -> Option<process::Child>
where
    I: Into<process::Stdio> + Read,
{
    let mut child_command = process::Command::new(command);

    child_command.args(user_inputs.iter().map(ffi::OsStr::new));

    if !process_position.is_first() {
        child_command.stdin(input_reader.unwrap());
    }

    child_command.stdout(output_direction).stderr(err_direction);

    child_command.spawn().ok()
}
