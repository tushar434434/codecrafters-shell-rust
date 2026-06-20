#![allow(unused_imports)]
use std::io::{self, Write};
use std::env;
use std::path::PathBuf;
use std::process::Command;
use std::fs::File;
use std::process::Stdio;
use std::fs::OpenOptions;
use rustyline::{
    completion::{Completer, Pair, FilenameCompleter},
    history::DefaultHistory,
    highlight::Highlighter,
    hint::Hinter,
    validate::Validator,
    Context, Editor, Helper,
};
use std::cell::{Cell, RefCell};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

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
    file_comp: FilenameCompleter,
}

impl Default for ShellHelper {
    fn default() -> Self {
        Self {
            file_comp: FilenameCompleter::new(),
        }
    }
}

impl Helper for ShellHelper {}
impl Hinter for ShellHelper { type Hint = String; }
impl Highlighter for ShellHelper {}
impl Validator for ShellHelper {}

impl Completer for ShellHelper {
    type Candidate = Pair;

    fn complete(&self, line: &str, pos: usize, ctx: &Context<'_>) -> rustyline::Result<(usize, Vec<Pair>)> {
        // Use the default FilenameCompleter directly
        self.file_comp.complete(line, pos, ctx)
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

        let mut parts: Vec<String> = Vec::new();
        let mut current = String::new();
        let mut in_quotes = false;
        let mut double_quotes = false;
        let mut escape = false;

        for c in command.chars() {
            if escape {
                current.push(c);
                escape = false;
            } else if c == '\\' {
                if double_quotes || !in_quotes { escape = true; } else { current.push(c); }
            } else if c == '\'' && !double_quotes {
                in_quotes = !in_quotes;
            } else if c == '"' && !in_quotes {
                double_quotes = !double_quotes;
            } else if c.is_whitespace() && !in_quotes && !double_quotes {
                if !current.is_empty() {
                    parts.push(current.clone());
                    current.clear();
                }
            } else {
                current.push(c);
            }
        }
        if !current.is_empty() { parts.push(current); }

        let cmd_name = &parts[0];
        if cmd_name == "exit" { break; }

        // Execution logic remains the same...
        if cmd_name == "echo" {
            println!("{}", parts[1..].join(" "));
        } else if cmd_name == "pwd" {
            if let Ok(p) = env::current_dir() { println!("{}", p.display()); }
        } else if cmd_name == "cd" {
            if let Some(dir) = parts.get(1) {
                if let Err(_) = env::set_current_dir(dir) {
                    println!("cd: {}: No such file or directory", dir);
                }
            }
        } else if let Some(path) = find_executable(cmd_name) {
            let mut cmd = Command::new(path);
            cmd.args(&parts[1..]);
            let _ = cmd.status();
        } else {
            println!("{}: command not found", cmd_name);
        }
    }
}