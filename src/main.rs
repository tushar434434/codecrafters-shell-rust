#![allow(unused_imports)]
use std::io::{self, Write};
use std::env;
use std::path::PathBuf;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Child, Stdio}; 
use std::fs::File;
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
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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
    completions: Arc<Mutex<HashMap<String, String>>>, 
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
        _: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let prefix = &line[..pos];
        let cmd_name = line.split_whitespace().next().unwrap_or("");
        if let Ok(comps) = self.completions.lock() {
            if let Some(path) = comps.get(cmd_name) {
                let words: Vec<&str> = prefix.split_whitespace().collect();
                
                let arg1 = cmd_name;
                
                let arg2 = if prefix.ends_with(' ') {
                    ""
                } else {
                    words.last().cloned().unwrap_or("")
                };
                
                let arg3 = if prefix.ends_with(' ') {
                    words.last().cloned().unwrap_or("")
                } else if words.len() >= 2 {
                    words[words.len() - 2]
                } else {
                    ""
                };

                if let Ok(output) = Command::new(path)
                    .args(&[arg1, arg2, arg3])
                    .env("COMP_LINE", line)
                    .env("COMP_POINT", pos.to_string())
                    .output() 
                {
                    let raw_stdout = String::from_utf8_lossy(&output.stdout);
                    let mut candidates: Vec<String> = raw_stdout
                        .lines()
                        .map(|l| l.trim().to_string())
                        .filter(|l| !l.is_empty())
                        .collect();

                    if candidates.len() == 1 {
                        let candidate = &candidates[0];
                        let replace_pos = pos - arg2.len();
                        return Ok((replace_pos, vec![Pair {
                            display: candidate.clone(),
                            replacement: format!("{} ", candidate),
                        }]));
                    } else if candidates.len() > 1 {
                        let mut lcp = candidates[0].clone();
                        for name in candidates.iter().skip(1) {
                            let mut common_len = 0;
                            for (c1, c2) in lcp.chars().zip(name.chars()) {
                                if c1 == c2 {
                                    common_len += c1.len_utf8();
                                } else {
                                    break;
                                }
                            }
                            lcp.truncate(common_len);
                        }

                        let replace_pos = pos - arg2.len();

                        if lcp.len() > arg2.len() {
                            return Ok((replace_pos, vec![Pair {
                                display: lcp.clone(),
                                replacement: lcp,
                            }]));
                        }

                        let mut last_p = self.last_prefix.borrow_mut();
                        if *last_p == prefix {
                            self.tab_count.set(self.tab_count.get() + 1);
                        } else {
                            self.tab_count.set(1);
                            *last_p = prefix.to_string();
                        }

                        if self.tab_count.get() == 1 {
                            print!("\x07"); 
                            io::stdout().flush().unwrap();
                            return Ok((pos, Vec::new()));
                        } else if self.tab_count.get() >= 2 {
                            println!();
                            candidates.sort();
                            println!("{}", candidates.join("  "));
                            print!("$ {}", prefix);
                            io::stdout().flush().unwrap();
                            self.tab_count.set(0);
                            return Ok((pos, Vec::new()));
                        }
                    }
                }
            }
        }
        if let Some(last_space_idx) = prefix.rfind(' ') {
            let file_prefix = &prefix[last_space_idx + 1..];

            let (search_dir, file_part, replace_pos) = if file_prefix.ends_with('/') {
                (
                    PathBuf::from(file_prefix),
                    "",
                    pos,
                )
            } else if let Some((d, f)) = file_prefix.rsplit_once('/') {
                let dir_str = if d.is_empty() { "." } else { d };
                (
                    PathBuf::from(dir_str),
                    f,
                    last_space_idx + 1 + d.len() + 1,
                )
            } else {
                (
                    env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
                    file_prefix,
                    last_space_idx + 1,
                )
            };

            let mut files = Vec::new();
            if let Ok(entries) = std::fs::read_dir(search_dir) {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        if name.starts_with('.') && !file_part.starts_with('.') {
                            continue;
                        }
                        if name.starts_with(file_part) {
                            let is_dir = entry.path().is_dir();
                            files.push((name.to_string(), is_dir));
                        }
                    }
                }
            }

            files.sort_by(|a, b| a.0.cmp(&b.0));
            files.dedup_by(|a, b| a.0 == b.0);

            if files.len() == 1 {
                let (matched_file, is_dir) = &files[0];

                let replacement = if file_prefix.ends_with('/') {
                    format!(
                        "{}{}",
                        matched_file,
                        if *is_dir { "/" } else { " " }
                    )
                } else {
                    format!(
                        "{}{}",
                        matched_file,
                        if *is_dir { "/" } else { " " }
                    )
                };

                let pairs = vec![Pair {
                    display: replacement.clone(),
                    replacement,
                }];
                return Ok((replace_pos, pairs));
            } else if files.len() > 1 {
                let mut lcp = files[0].0.clone();
                for (name, _) in files.iter().skip(1) {
                    let mut common_len = 0;
                    for (c1, c2) in lcp.chars().zip(name.chars()) {
                        if c1 == c2 {
                            common_len += c1.len_utf8();
                        } else {
                            break;
                        }
                    }
                    lcp.truncate(common_len);
                }
                if lcp.len() > file_part.len() {
                    let replacement = lcp.clone();

                    let pairs = vec![Pair {
                        display: replacement.clone(),
                        replacement,
                    }];
                    return Ok((replace_pos, pairs));
                }
                let mut last_p = self.last_prefix.borrow_mut();
                if *last_p == prefix {
                    self.tab_count.set(self.tab_count.get() + 1);
                } else {
                    self.tab_count.set(1);
                    *last_p = prefix.to_string();
                }

                if self.tab_count.get() == 1 {
                    print!("\x07");
                    io::stdout().flush().unwrap();
                    return Ok((pos, Vec::new()));
                } else if self.tab_count.get() >= 2 {
                    println!();
                    let display_names: Vec<String> = files.iter().map(|(name, is_dir)| {
                        if *is_dir {
                            format!("{}/", name)
                        } else {
                            name.clone()
                        }
                    }).collect();
                    println!("{}", display_names.join("  "));
                    print!("$ {}", prefix);
                    io::stdout().flush().unwrap();
                    self.tab_count.set(0);
                    return Ok((pos, Vec::new()));
                }
            }

            return Ok((pos, Vec::new()));
        }
        let mut commands = vec![
            "echo".to_string(),
            "exit".to_string(),
            "type".to_string(),
            "pwd".to_string(),
            "cd".to_string(),
            "jobs".to_string(),
        ];
        if let Ok(path_env) = env::var("PATH") {
            for dir in env::split_paths(&path_env) {
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        if let Some(name) = entry.file_name().to_str() {
                            commands.push(name.to_string());
                        }
                    }
                }
            }
        }
        commands.sort();
        commands.dedup();
        let matching_names: Vec<String> = commands
            .into_iter()
            .filter(|cmd| cmd.starts_with(prefix))
            .collect();
        if matching_names.is_empty() {
            return Ok((0, Vec::new()));
        }
        if matching_names.len() == 1 {
            let cmd = &matching_names[0];
            let pairs = vec![Pair {
                display: cmd.clone(),
                replacement: format!("{} ", cmd),
            }];
            return Ok((0, pairs));
        }
        let mut lcp = matching_names[0].clone();
        for name in matching_names.iter().skip(1) {
            let mut common_len = 0;
            for (c1, c2) in lcp.chars().zip(name.chars()) {
                if c1 == c2 {
                    common_len += c1.len_utf8();
                } else {
                    break;
                }
            }
            lcp.truncate(common_len);
        }
        if lcp.len() > prefix.len() {
            let pairs = vec![Pair {
                display: lcp.clone(),
                replacement: lcp,
            }];
            return Ok((0, pairs));
        }
        let mut last_p = self.last_prefix.borrow_mut();
        if *last_p == prefix {
            self.tab_count.set(self.tab_count.get() + 1);
        } else {
            self.tab_count.set(1);
            *last_p = prefix.to_string();
        }
        if self.tab_count.get() == 1 {
            print!("\x07");
            io::stdout().flush().unwrap();
            return Ok((0, Vec::new()));
        } else if self.tab_count.get() >= 2 {
            println!();
            println!("{}", matching_names.join("  "));
            print!("$ {}", prefix);
            io::stdout().flush().unwrap();
            self.tab_count.set(0);
            return Ok((0, Vec::new()));
        }
        Ok((0, Vec::new()))
    }
}

struct BgJob {
    job_id: u32,
    child: Child,
    command_str: String,
}

fn main() {
    let completions: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
    let mut r1 = Editor::<ShellHelper, DefaultHistory>::new().unwrap();
    
    let mut bg_jobs: Vec<BgJob> = Vec::new();
    
    let mut current_job_id: Option<u32> = None;
    let mut previous_job_id: Option<u32> = None;

    r1.set_helper(Some(ShellHelper {
        last_prefix: RefCell::new(String::new()),
        tab_count: Cell::new(0),
        completions: Arc::clone(&completions),
    }));

    loop {
        let mut i = 0;
        while i < bg_jobs.len() {
            match bg_jobs[i].child.try_wait() {
                Ok(Some(_status)) => {
                    let removed_id = bg_jobs[i].job_id;
                    let cmd_str = bg_jobs[i].command_str.clone();
                    
                    let marker = if current_job_id == Some(removed_id) {
                        "+"
                    } else if previous_job_id == Some(removed_id) {
                        "-"
                    } else {
                        " "
                    };
                    
                    println!("[{}]{}  Done                {} ", removed_id, marker, cmd_str);
                    
                    bg_jobs.remove(i);
                    if current_job_id == Some(removed_id) {
                        current_job_id = None;
                    }
                    if previous_job_id == Some(removed_id) {
                        previous_job_id = None;
                    }
                }
                Ok(None) => {
                    i += 1;
                }
                Err(_) => {
                    let removed_id = bg_jobs[i].job_id;
                    bg_jobs.remove(i);
                    
                    if current_job_id == Some(removed_id) {
                        current_job_id = None;
                    }
                    if previous_job_id == Some(removed_id) {
                        previous_job_id = None;
                    }
                }
            }
        }

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
                if double_quotes {
                    escape = true;
                } else if !in_quotes {
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
        
        if parts.is_empty() {
            continue;
        }

        let mut is_background = false;
        if parts.last().map(|s| s.as_str()) == Some("&") {
            is_background = true;
            parts.pop();
        }

        if parts.is_empty() {
            continue;
        }

        let cmd_name = parts[0].trim().to_string();
        let mut stdout_file = None;
        let mut stderr_file = None;
        let mut append_stdout = false;
        let mut append_stderr = false;
        let mut args = Vec::new();
        let mut idx = 1;
        while idx < parts.len() {
            if parts[idx] == ">" || parts[idx] == "1>" {
                stdout_file = Some(parts[idx + 1].clone());
                idx += 2;
                continue;
            } else if parts[idx] == ">>" || parts[idx] == "1>>" {
                stdout_file = Some(parts[idx + 1].clone());
                append_stdout = true;
                idx += 2;
                continue;
            } else if parts[idx] == "2>" {
                stderr_file = Some(parts[idx + 1].clone());
                idx += 2;
                continue;
            } else if parts[idx] == "2>>" {
                stderr_file = Some(parts[idx + 1].clone());
                append_stderr = true;
                idx += 2;
                continue;
            }
            args.push(parts[idx].clone());
            idx += 1;
        }

        if cmd_name == "exit" {
            break;
        } else if cmd_name == "echo" {
            let output = args.join(" ");

            if let Some(file_name) = &stdout_file {
                if append_stdout {
                    let mut file = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(file_name)
                        .unwrap();

                    writeln!(file, "{}", output).unwrap();
                } else {
                    std::fs::write(file_name, format!("{}\n", output)).unwrap();
                }
            } else {
                println!("{}", output);
            }
            if let Some(file_name) = &stderr_file {
                if append_stderr {
                    let _file = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(file_name)
                        .unwrap();
                } else {
                    let _file = File::create(file_name).unwrap();
                }
            }
        } else if cmd_name == "type" {
            if args.is_empty() {
                println!("type: missing arguments");
                continue;
            }
            let arg = &args[0];

            if arg == "echo" || arg == "exit" || arg == "type" || arg == "pwd" || arg == "cd" || arg == "complete" || arg == "jobs" {
                println!("{} is a shell builtin", arg);
            } else if let Some(path) = find_executable(arg) {
                println!("{} is {}", arg, path.display());
            } else {
                println!("{}: not found", arg);
            }
        } else if cmd_name == "complete" {
            if args.len() >= 3 && args[0] == "-C" {
                let path = args[1].clone();
                let cmd = args[2].clone();
                if let Ok(mut comps) = completions.lock() {
                    comps.insert(cmd, path);
                }
            } else if args.len() >= 2 && args[0] == "-r" {
                let cmd = &args[1];
                if let Ok(mut comps) = completions.lock() {
                    comps.remove(cmd);
                }
            } else if args.len() >= 2 && args[0] == "-p" {
                let cmd = &args[1];
                if let Ok(comps) = completions.lock() {
                    if let Some(path) = comps.get(cmd) {
                        println!("complete -C '{}' {}", path, cmd);
                    } else {
                        println!("complete: {}: no completion specification", cmd);
                    }
                }
            }
        } else if cmd_name == "pwd" {
            match env::current_dir() {
                Ok(path) => println!("{}", path.display()),
                Err(_) => eprintln!("pwd: unable to get current directory"),
            }
        } else if cmd_name == "cd" {
            if args.is_empty() {
                continue;
            }
            let dir = &args[0];
            if dir == "~" {
                if let Ok(home) = env::var("HOME") {
                    env::set_current_dir(home).unwrap();
                }
            } else if let Err(_) = env::set_current_dir(dir) {
                println!("cd: {}: No such file or directory", dir);
            }
        } else if cmd_name == "jobs" {
            for job in &bg_jobs {
                let marker = if current_job_id == Some(job.job_id) {
                    "+"
                } else if previous_job_id == Some(job.job_id) {
                    "-"
                } else {
                    " "
                };
                println!("[{}]{}  Running                {} &", job.job_id, marker, job.command_str);
            }
        } else {
            if let Some(_path) = find_executable(&cmd_name) {
                let args_ref: Vec<&str> = args
                    .iter()
                    .map(|s| s.as_str())
                    .collect();
                let mut cmd = Command::new(&cmd_name);
                cmd.args(args_ref);
                if let Some(file_name) = &stdout_file {
                    if append_stdout {
                        let file = OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(file_name)
                            .unwrap();

                        cmd.stdout(Stdio::from(file));
                    } else {
                        let file = File::create(file_name).unwrap();
                        cmd.stdout(Stdio::from(file));
                    }
                }
                if let Some(file_name) = &stderr_file {
                    if append_stderr {
                        let file = OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(file_name)
                            .unwrap();

                        cmd.stderr(Stdio::from(file));
                    } else {
                        let file = File::create(file_name).unwrap();
                        cmd.stderr(Stdio::from(file));
                    }
                } 
                let mut child = cmd
                    .spawn()
                    .unwrap();

                if is_background {
                    let next_job_id = if bg_jobs.is_empty() {
                        1
                    } else {
                        bg_jobs.iter().map(|j| j.job_id).max().unwrap_or(0) + 1
                    };

                    println!("[{}] {}", next_job_id, child.id());
                    
                    let trailing_args = args.join(" ");
                    let full_cmd_str = if trailing_args.is_empty() {
                        cmd_name
                    } else {
                        format!("{} {}", cmd_name, trailing_args)
                    };
                    bg_jobs.push(BgJob {
                        job_id: next_job_id,
                        child,
                        command_str: full_cmd_str.trim().to_string(),
                    });
                    
                    previous_job_id = current_job_id;
                    current_job_id = Some(next_job_id);
                } else {
                    child.wait().unwrap();
                }
            } else {
                println!("{}: command not found", cmd_name);
            }
        }
    }
}