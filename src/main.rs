#![allow(unused_imports)]
use std::io::{self, Write};
use std::env;
use std::path::PathBuf;
use std::os::unix::fs::PermissionsExt;
use std::process::Command; 
use std::fs::File;
use std::process::Stdio;
use std::fs::OpenOptions;
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
                    if metadata.permissions().mode() & 0o111 != 0 {
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
        let mut commands = vec![
            "echo".to_string(), "exit".to_string(), "type".to_string(),
            "pwd".to_string(), "cd".to_string(), "complete".to_string(),
        ];

        if let Ok(path_env) = env::var("PATH") {
            for dir in env::split_paths(&path_env) {
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        if let Ok(name) = entry.file_name().into_string() {
                            commands.push(name);
                        }
                    }
                }
            }
        }
        commands.sort();
        commands.dedup();

        let matches: Vec<Pair> = commands
            .iter()
            .filter(|cmd| cmd.starts_with(prefix))
            .map(|cmd| Pair {
                display: cmd.clone(),
                replacement: cmd.clone(), // Return exact match for completion
            })
            .collect();

        if matches.len() == 1 {
            return Ok((0, matches));
        }
        
        Ok((0, matches))
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
            if escape { current.push(c); escape = false; }
            else if c == '\\' { if double_quotes || !in_quotes { escape = true; } else { current.push(c); } }
            else if c == '\'' && !double_quotes { in_quotes = !in_quotes; }
            else if c == '"' && !in_quotes { double_quotes = !double_quotes; }
            else if c.is_whitespace() && !in_quotes && !double_quotes {
                if !current.is_empty() { parts.push(current.clone()); current.clear(); }
            } else { current.push(c); }
        }
        if !current.is_empty() { parts.push(current); }

        let cmd_name = &parts[0];
        let mut stdout_file = None;
        let mut stderr_file = None;
        let mut append_stdout = false;
        let mut append_stderr = false;
        let mut args = Vec::new();
        let mut i = 1;
        while i < parts.len() {
            if parts[i] == ">" || parts[i] == "1>" { stdout_file = Some(parts[i + 1].clone()); i += 2; continue; }
            else if parts[i] == ">>" || parts[i] == "1>>" { stdout_file = Some(parts[i + 1].clone()); append_stdout = true; i += 2; continue; }
            else if parts[i] == "2>" { stderr_file = Some(parts[i + 1].clone()); i += 2; continue; }
            else if parts[i] == "2>>" { stderr_file = Some(parts[i + 1].clone()); append_stderr = true; i += 2; continue; }
            args.push(parts[i].clone()); i += 1;
        }

        if cmd_name == "exit" { break; }
        else if cmd_name == "complete" { }
        else if cmd_name == "echo" {
            let output = args.join(" ");
            if let Some(f) = stdout_file {
                if append_stdout { writeln!(OpenOptions::new().create(true).append(true).open(f).unwrap(), "{}", output).unwrap(); }
                else { std::fs::write(f, format!("{}\n", output)).unwrap(); }
            } else { println!("{}", output); }
        } else if cmd_name == "type" {
            let arg = &args[0];
            if ["echo", "exit", "type", "pwd", "cd", "complete"].contains(&arg.as_str()) {
                println!("{} is a shell builtin", arg);
            } else if let Some(path) = find_executable(arg) { println!("{} is {}", arg, path.display()); }
            else { println!("{}: not found", arg); }
        } else if cmd_name == "pwd" {
            if let Ok(p) = env::current_dir() { println!("{}", p.display()); }
        } else if cmd_name == "cd" {
            if let Err(_) = env::set_current_dir(&args[0]) { println!("cd: {}: No such file or directory", args[0]); }
        } else {
            if let Some(path) = find_executable(cmd_name) {
                let mut cmd = Command::new(path);
                cmd.args(args);
                let _ = cmd.status();
            } else { println!("{}: command not found", command); }
        }
    }
}