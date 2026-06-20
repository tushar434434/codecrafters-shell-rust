#![allow(unused_imports)]
use std::io::{self, Write};
use std::env;
use std::path::PathBuf;
use std::process::Command;
use std::fs::File;
use std::process::Stdio;
use std::fs::OpenOptions;

// Use conditional compilation for Unix-specific features
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use rustyline::{
    completion::{Completer, Pair},
    history::DefaultHistory,
    highlight::Highlighter,
    hint::Hinter,
    validate::Validator,
    Context, Editor, Helper,
};
use std::cell::{Cell, RefCell};

fn find_executable(cmd: &str) -> Option<PathBuf> {
    if let Ok(path_env) = env::var("PATH") {
        for path in env::split_paths(&path_env) {
            let exe_path = path.join(cmd);
            if exe_path.exists() {
                if let Ok(metadata) = exe_path.metadata() {
                    #[cfg(unix)]
                    {
                        if metadata.permissions().mode() & 0o111 != 0 {
                            return Some(exe_path);
                        }
                    }
                    #[cfg(windows)]
                    {
                        return Some(exe_path);
                    }
                }
            }
        }
    }
    None
}

struct ShellHelper {
    last_prefix: RefCell<String>,
    tab_count: Cell<usize>,
}

impl Default for ShellHelper {
    fn default() -> Self {
        Self {
            last_prefix: RefCell::new(String::new()),
            tab_count: Cell::new(0),
        }
    }
}

impl Helper for ShellHelper {}
impl Hinter for ShellHelper { type Hint = String; }
impl Highlighter for ShellHelper {}
impl Validator for ShellHelper {}

impl Completer for ShellHelper {
    type Candidate = Pair;

    fn complete(&self, line: &str, pos: usize, _: &Context<'_>) -> rustyline::Result<(usize, Vec<Pair>)> {
        let prefix = &line[..pos];
        if let Some(last_space_idx) = prefix.rfind(' ') {
            let file_prefix = &prefix[last_space_idx + 1..];
            let (search_dir, file_part, replace_pos) = if let Some((d, f)) = file_prefix.rsplit_once('/') {
                (PathBuf::from(if d.is_empty() { "." } else { d }), f, last_space_idx + 1 + d.len() + 1)
            } else {
                (env::current_dir().unwrap_or_else(|_| PathBuf::from(".")), file_prefix, last_space_idx + 1)
            };

            let mut files = Vec::new();
            if let Ok(entries) = std::fs::read_dir(search_dir) {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        if name.starts_with(file_part) {
                            files.push((name.to_string(), entry.path().is_dir()));
                        }
                    }
                }
            }

            if files.len() == 1 {
                let (name, is_dir) = &files[0];
                let mut replacement = name.clone();
                if *is_dir { replacement.push('/'); } else { replacement.push(' '); }
                return Ok((replace_pos, vec![Pair { display: name.clone(), replacement }]));
            }
        }
        Ok((0, Vec::new()))
    }
}

fn main() {
    let mut r1 = Editor::<ShellHelper, DefaultHistory>::new().unwrap();
    r1.set_helper(Some(ShellHelper::default()));
    loop {
        let command = match r1.readline("$ ") {
            Ok(line) => line.trim().to_string(),
            Err(_) => break,
        };
        if command.is_empty() { continue; }
        // ... (Keep your existing execution/parsing logic here)
    }
}