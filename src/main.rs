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
    completion::{Completer, Pair, FilenameCompleter},
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

impl Hinter for ShellHelper {
    type Hint = String;
}

impl Highlighter for ShellHelper {}

impl Validator for ShellHelper {}

impl Completer for ShellHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        // If we are at the start of the line (command completion)
        if !line[..pos].contains(' ') {
            let prefix = &line[..pos];
            let mut commands = vec![
                "echo".to_string(), "exit".to_string(), "type".to_string(),
                "pwd".to_string(), "cd".to_string(), "complete".to_string(),
            ];

            // Scan PATH for executables
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
                    replacement: format!("{} ", cmd),
                })
                .collect();

            return Ok((0, matches));
        }

        // Otherwise, use file completion
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
        if command.is_empty() {
            continue;
        }

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
                if double_quotes || !in_quotes {
                    escape = true;
                } else {
                    current.push(c);
                }
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

        if !current.is_empty() {
            parts.push(current);
        }

        let cmd_name = &parts[0];
        let mut stdout_file = None;
        let mut stderr_file = None;
        let mut append_stdout = false;
        let mut append_stderr = false;

        let mut args = Vec::new();
        let mut i = 1;
        while i < parts.len() {
            if parts[i] == ">" || parts[i] == "1>" {
                stdout_file = Some(parts[i + 1].clone());
                i += 2;
                continue;
            } else if parts[i] == ">>" || parts[i] == "1>>" {
                stdout_file = Some(parts[i + 1].clone());
                append_stdout = true;
                i += 2;
                continue;
            } else if parts[i] == "2>" {
                stderr_file = Some(parts[i + 1].clone());
                i += 2;
                continue;
            } else if parts[i] == "2>>" {
                stderr_file = Some(parts[i + 1].clone());
                append_stderr = true;
                i += 2;
                continue;
            }
            args.push(parts[i].clone());
            i += 1;
        }

        if cmd_name == "exit" {
            break;
        } else if cmd_name == "complete" {
            if args.len() >= 2 && args[0] == "-p" {
                println!("complete: {}: no completion specification", args[1]);
            }
        } else if cmd_name == "echo" {
            let output = args.join(" ");
            if let Some(file_name) = &stdout_file {
                if append_stdout {
                    let mut file = OpenOptions::new().create(true).append(true).open(file_name).unwrap();
                    writeln!(file, "{}", output).unwrap();
                } else {
                    std::fs::write(file_name, format!("{}\n", output)).unwrap();
                }
            } else {
                println!("{}", output);
            }
        } else if cmd_name == "type" {
            let arg = &args[0];
            if arg == "echo" || arg == "exit" || arg == "type" || arg == "pwd" || arg == "cd" || arg == "complete" {
                println!("{} is a shell builtin", arg);
            } else if let Some(path) = find_executable(arg) {
                println!("{} is {}", arg, path.display());
            } else {
                println!("{}: not found", arg);
            }
        } else if cmd_name == "pwd" {
            match env::current_dir() {
                Ok(path) => println!("{}", path.display()),
                Err(_) => eprintln!("pwd: unable to get current directory"),
            }
        } else if cmd_name == "cd" {
            let dir = &args[0];
            if dir == "~" {
                if let Ok(home) = env::var("HOME") {
                    env::set_current_dir(home).unwrap();
                }
            } else if let Err(_) = env::set_current_dir(dir) {
                println!("cd: {}: No such file or directory", dir);
            }
        } else {
            if let Some(path) = find_executable(cmd_name) {
                let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                let mut cmd = Command::new(path);
                cmd.args(args_ref);
                if let Some(file_name) = &stdout_file {
                    if append_stdout {
                        let file = OpenOptions::new().create(true).append(true).open(file_name).unwrap();
                        cmd.stdout(Stdio::from(file));
                    } else {
                        let file = File::create(file_name).unwrap();
                        cmd.stdout(Stdio::from(file));
                    }
                }
                if let Some(file_name) = &stderr_file {
                    if append_stderr {
                        let file = OpenOptions::new().create(true).append(true).open(file_name).unwrap();
                        cmd.stderr(Stdio::from(file));
                    } else {
                        let file = File::create(file_name).unwrap();
                        cmd.stderr(Stdio::from(file));
                    }
                }
                let mut child = cmd.spawn().unwrap();
                child.wait().unwrap();
            } else {
                println!("{}: command not found", command);
            }
        }
    }
}