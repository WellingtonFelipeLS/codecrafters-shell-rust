use std::{collections::HashMap, process};

use rustyline::{
    Helper,
    completion::{Completer, FilenameCompleter, Pair},
    highlight::Highlighter,
    hint::Hinter,
    validate::Validator,
};

#[derive(Debug)]
struct Trie(HashMap<char, Trie>); // Trie

impl Trie {
    pub fn new() -> Self {
        Trie(HashMap::new())
    }

    pub fn insert(&mut self, word: &str) {
        word.chars()
            .chain(std::iter::once('\0'))
            .fold(self, |acc, c| acc.0.entry(c).or_insert_with(Trie::new));
    }

    pub fn starts_with(&self, prefix: &str) -> Vec<String> {
        let mut result = Vec::new();

        if let Some(trie_root) = prefix.chars().try_fold(self, |acc, c| acc.0.get(&c)) {
            let mut v: Vec<(String, &Trie)> = trie_root
                .0
                .iter()
                .map(|(&c, helper)| {
                    let mut s = String::from(prefix);
                    s.push(c);
                    (s, helper)
                })
                .collect();
            while let Some((mut s, helper)) = v.pop() {
                if helper.0.is_empty() {
                    s.pop();
                    result.push(s);
                } else {
                    v.extend(helper.0.iter().map(|(&c, helper)| {
                        let mut s = s.clone();
                        s.push(c);
                        (s, helper)
                    }))
                }
            }
        }

        result.sort_unstable();

        result
    }
}

pub struct MyHelper {
    commands: Trie,
    file_completer: FilenameCompleter,
    completer_scripts: HashMap<String, String>,
}

impl MyHelper {
    pub fn new() -> Self {
        Self {
            commands: Trie::new(),
            file_completer: FilenameCompleter::new(),
            completer_scripts: HashMap::new(),
        }
    }

    pub fn insert_completer_script(&mut self, key: String, value: String) -> Option<String> {
        self.completer_scripts.insert(key, value)
    }

    pub fn get_completer_script(&mut self, key: &str) -> Option<&String> {
        self.completer_scripts.get(key)
    }
}

impl<'a, T> From<T> for MyHelper
where
    T: Iterator<Item = &'a str>,
{
    fn from(commands: T) -> Self {
        commands.fold(MyHelper::new(), |mut acc, word| {
            acc.commands.insert(word);
            acc
        })
    }
}

impl Helper for MyHelper {}
impl Highlighter for MyHelper {}
impl Validator for MyHelper {}

impl Completer for MyHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        if line.contains(" ") {
            if let Some(script) = self.completer_scripts.get(line.trim_end())
                && let Ok(output) = process::Command::new(script).output()
                && let Ok(stringified_output) = String::from_utf8(output.stdout)
            {
                return Ok((
                    pos,
                    stringified_output
                        .lines()
                        .map(|candidate| Pair {
                            display: String::from_iter([candidate, " "]),
                            replacement: String::from_iter([candidate, " "]),
                        })
                        .collect(),
                ));
            }

            let (n, mut candidates) = self.file_completer.complete(line, pos, ctx)?;

            if let [x] = candidates.as_mut_slice()
                && !x.replacement.ends_with("/")
            {
                x.display.push(' ');
                x.replacement.push(' ');
            }

            candidates
                .iter_mut()
                .for_each(|x| x.display = x.replacement.clone());

            return Ok((n, candidates));
        }

        let mut candidates = self.commands.starts_with(line);

        if let [x] = candidates.as_mut_slice() {
            x.push(' ');
        }

        Ok((
            0,
            candidates
                .into_iter()
                .map(|cmd| Pair {
                    display: cmd.clone(),
                    replacement: cmd,
                })
                .collect(),
        ))
    }
}

impl Hinter for MyHelper {
    type Hint = &'static str;

    fn hint(&self, _line: &str, _pos: usize, _ctx: &rustyline::Context<'_>) -> Option<Self::Hint> {
        None
    }
}
