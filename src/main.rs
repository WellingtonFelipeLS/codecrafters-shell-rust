use std::collections::HashMap;
use std::env;
use std::env::{split_paths, var};
use std::io::{self};
use std::path::Path;
use std::{collections::HashSet, fs::DirEntry};

use rustyline::config::{BellStyle, Configurer};
use rustyline::history::FileHistory;
use rustyline::{Cmd, CompletionType, Config, Editor, KeyCode, KeyEvent, Modifiers};

mod commands;
mod helper;
mod utils;

use crate::helper::MyHelper;

fn main_loop(
    editor: &mut Editor<MyHelper, FileHistory>,
    builtins: &HashSet<&str>,
    executable_paths: &[&DirEntry],
    variable_map: &mut HashMap<String, String>,
) -> Result<(), io::Error> {
    let readline = match editor.readline("$ ") {
        Ok(x) => x,
        Err(err) => panic!("{err:?}"),
    };

    let mut processed_user_inputs = Vec::new();

    let last_input =
        utils::read_user_input(&readline)
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

    if !readline.is_empty() {
        let _ = editor.add_history_entry(readline);
    }

    let len = processed_user_inputs.len();

    let mut children = Vec::new();

    processed_user_inputs.into_iter().enumerate().try_fold(
        None,
        |input_reader, (idx, user_inputs)| {
            commands::call_command_with_args(
                user_inputs,
                builtins,
                executable_paths,
                editor.history_mut(),
                input_reader,
                &mut children,
                variable_map,
                utils::ProcessPosition::new(idx, len),
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

    let builtins = HashSet::from(["exit", "echo", "type", "pwd", "cd", "history", "declare"]);

    let executable_paths = path_variable
        .iter()
        .filter(|x| utils::is_executable(x))
        .collect::<Vec<_>>();

    let executable_names = executable_paths
        .iter()
        .filter_map(|x| x.file_name().into_string().ok())
        .collect::<Vec<_>>();

    let config = Config::builder()
        .completion_type(CompletionType::List)
        .bell_style(BellStyle::Audible)
        .build();

    let mut variable_map = HashMap::new();

    let mut editor: Editor<MyHelper, _> = Editor::with_config(config)?;
    editor.set_helper(Some(MyHelper::from(
        builtins
            .iter()
            .cloned()
            .chain(executable_names.iter().map(String::as_str)),
    )));

    editor.bind_sequence(KeyEvent(KeyCode::Up, Modifiers::NONE), Cmd::PreviousHistory);

    editor.set_history_ignore_dups(false)?;

    if let Some(history_filepath) =
        env::vars().find_map(|(k, v)| if k == "HISTFILE" { Some(v) } else { None })
    {
        editor.load_history(&history_filepath)?;
    }

    loop {
        let _ = main_loop(&mut editor, &builtins, &executable_paths, &mut variable_map);
    }
}
