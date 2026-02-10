use std::collections::HashMap;

use rustyline::{
    Helper, completion::Completer, highlight::Highlighter, hint::Hinter, validate::Validator,
};

#[derive(Debug)]
pub struct MyHelper(HashMap<char, MyHelper>); // Trie

impl MyHelper {
    pub fn new() -> Self {
        MyHelper(HashMap::new())
    }

    pub fn insert(&mut self, word: &str) {
        word.chars()
            .chain(std::iter::once('\0'))
            .fold(self, |acc, c| acc.0.entry(c).or_insert_with(MyHelper::new));
    }

    pub fn starts_with(&self, prefix: &str) -> Vec<String> {
        let mut result = Vec::new();

        if let Some(trie_root) = prefix.chars().try_fold(self, |acc, c| acc.0.get(&c)) {
            let mut v: Vec<(String, &MyHelper)> = trie_root
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

impl<'a, T> From<T> for MyHelper
where
    T: Iterator<Item = &'a str>,
{
    fn from(words: T) -> Self {
        words.fold(MyHelper::new(), |mut acc, word| {
            acc.insert(word);
            acc
        })
    }
}

impl Helper for MyHelper {}
impl Highlighter for MyHelper {}
impl Validator for MyHelper {}

impl Completer for MyHelper {
    type Candidate = String;

    fn complete(
        &self,
        line: &str,
        _pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        let mut candidates = self.starts_with(line);

        if candidates.len() == 1
            && let Some(x) = candidates.get_mut(0)
        {
            x.push(' ')
        }

        Ok((0, candidates))
    }
}

impl Hinter for MyHelper {
    type Hint = &'static str;

    fn hint(&self, _line: &str, _pos: usize, _ctx: &rustyline::Context<'_>) -> Option<Self::Hint> {
        None
    }
}
