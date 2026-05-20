use crate::utils;
use rustyline::history::{self, History};
use std::{
    collections::{HashMap, HashSet},
    env, ffi, fs,
    io::{self, Read, Write},
    path, process,
};

#[allow(clippy::too_many_arguments)]
pub fn call_command_with_args<I>(
    mut user_inputs: Vec<String>,
    builtins: &HashSet<&str>,
    executable_paths: &[&fs::DirEntry],
    history: &mut history::FileHistory,
    input_reader: Option<I>,
    children: &mut Vec<process::Child>,
    shell_variables: &mut HashMap<String, String>,
    background_jobs: &mut utils::BackGroundJobs,
    completion_scripts: &mut HashMap<String, String>,
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
        Some("echo") => echo(&user_inputs[1..], output_direction),
        Some("type") => type_command(
            &user_inputs[1..],
            builtins,
            executable_paths,
            output_direction,
            err_direction,
        ),
        Some("pwd") => pwd(output_direction),
        Some("cd") => cd(&user_inputs[1..], err_direction),
        Some("history") => {
            history_command(&user_inputs[1..], history, output_direction, err_direction)
        }
        Some("declare") => declare(
            &user_inputs[1..],
            shell_variables,
            output_direction,
            err_direction,
        ),
        Some("jobs") => jobs(background_jobs, output_direction),
        Some("complete") => complete(
            &user_inputs[1..],
            completion_scripts,
            output_direction,
            err_direction,
        ),
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

fn echo(user_inputs: &[String], mut output_direction: utils::OutputDirection) -> io::Result<()> {
    writeln!(output_direction, "{}", user_inputs.join(" "))
}

fn type_command(
    user_inputs: &[String],
    builtins: &HashSet<&str>,
    executable_paths: &[&fs::DirEntry],
    mut output_direction: utils::OutputDirection,
    mut err_direction: utils::ErrDirection,
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

fn declare(
    user_inputs: &[String],
    shell_variables: &mut HashMap<String, String>,
    mut output_direction: utils::OutputDirection,
    mut err_direction: utils::ErrDirection,
) -> io::Result<()> {
    match user_inputs.first().map(String::as_str) {
        Some(x) if let Some((name, value)) = x.split_once('=') => {
            let mut chars = name.chars();
            let first_char = chars.next();
            if first_char.is_some_and(|x| x.is_ascii_alphabetic() || x == '_')
                && chars.all(|x| x.is_ascii_alphanumeric() || x == '_')
            {
                shell_variables.insert(name.into(), value.into());
                Ok(())
            } else {
                writeln!(
                    err_direction,
                    "declare: `{name}={value}': not a valid identifier"
                )
            }
        }
        Some("-p") if let Some(name) = user_inputs.get(1) => {
            if let Some(value) = shell_variables.get(name) {
                writeln!(output_direction, "declare -- {name}={value:?}")
            } else {
                writeln!(err_direction, "declare: {name}: not found")
            }
        }
        _ => shell_variables
            .iter()
            .try_for_each(|(name, value)| writeln!(output_direction, "{name}={value:?}")),
    }
}

fn jobs(
    background_jobs: &mut utils::BackGroundJobs,
    mut output_direction: utils::OutputDirection,
) -> io::Result<()> {
    background_jobs.list(&mut output_direction)
}

fn complete(
    user_inputs: &[String],
    completion_scripts: &mut HashMap<String, String>,
    mut output_direction: utils::OutputDirection,
    mut err_direction: utils::ErrDirection,
) -> io::Result<()> {
    match user_inputs.first().map(String::as_str) {
        Some("-C") if let Some([path, command]) = user_inputs.get(1..=2) => {
            completion_scripts.insert(command.clone(), path.clone());
            Ok(())
        }
        Some("-p") if let Some(command) = user_inputs.get(1) => {
            if let Some(completion) = completion_scripts.get(command) {
                writeln!(output_direction, "complete -C '{completion}' {command}")
            } else {
                writeln!(
                    err_direction,
                    "complete: {command}: no completion specification"
                )
            }
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
        child_command.stdin(input_reader.expect("Expect input reader in first process"));
    }

    child_command.stdout(output_direction).stderr(err_direction);

    child_command.spawn().ok()
}
