use std::collections::HashMap;

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
}

impl MyHelper {
    fn new() -> Self {
        Self {
            commands: Trie::new(),
            file_completer: FilenameCompleter::new(),
        }
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
            let (n, mut candidates) = self.file_completer.complete(line, pos, ctx)?;

            if candidates.len() == 1
                && let Some(x) = candidates.get_mut(0)
                && !x.replacement.ends_with("/")
            {
                x.display.push(' ');
                x.replacement.push(' ');
            }

            candidates
                .iter_mut()
                .for_each(|x| x.display = x.replacement.clone());

            Ok((n, candidates))
        } else {
            let mut candidates = self.commands.starts_with(line);

            if candidates.len() == 1
                && let Some(x) = candidates.get_mut(0)
            {
                x.push(' ')
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
}

impl Hinter for MyHelper {
    type Hint = &'static str;

    fn hint(&self, _line: &str, _pos: usize, _ctx: &rustyline::Context<'_>) -> Option<Self::Hint> {
        None
    }
}
