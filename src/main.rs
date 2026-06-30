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
    history::History,
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

fn parse_arguments(input: &str, shell_variables: &HashMap<String, String>) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;

    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if escape {
            current.push(c);
            escape = false;
            i += 1;
        } else if c == '\\' {
            if in_double || !in_single {
                escape = true;
            } else {
                current.push(c);
            }
            i += 1;
        } else if c == '\'' && !in_double {
            in_single = !in_single;
            i += 1;
        } else if c == '"' && !in_single {
            in_double = !in_double;
            i += 1;
        } else if c == '$' && !in_single {
            i += 1;
            if i < chars.len() && chars[i] == '{' {
                i += 1; // Skip '{'
                let mut var_name = String::new();
                while i < chars.len() && chars[i] != '}' {
                    var_name.push(chars[i]);
                    i += 1;
                }
                if i < chars.len() && chars[i] == '}' {
                    i += 1; // Skip '}'
                }
                if let Some(val) = shell_variables.get(&var_name) {
                    current.push_str(val);
                }
            } else {
                let mut var_name = String::new();
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    var_name.push(chars[i]);
                    i += 1;
                }
                if !var_name.is_empty() {
                    if let Some(val) = shell_variables.get(&var_name) {
                        current.push_str(val);
                    }
                } else {
                    current.push('$');
                }
            }
        } else if (c == ' ' || c == '\t') && !in_single && !in_double {
            if !current.is_empty() {
                parts.push(current.clone());
                current.clear();
            }
            i += 1;
        } else {
            current.push(c);
            i += 1;
        }
    }

    if !current.is_empty() {
        parts.push(current);
    }

    parts
}

fn is_valid_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars();
    if let Some(first) = chars.next() {
        if !first.is_ascii_alphabetic() && first != '_' {
            return false;
        }
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn execute_declare(args: &[String], shell_variables: &mut HashMap<String, String>) -> String {
    if args.len() >= 2 && args[0] == "-p" {
        let var_name = &args[1];
        if !is_valid_identifier(var_name) {
            format!("declare: `{}': not a valid identifier\n", var_name)
        } else if let Some(val) = shell_variables.get(var_name) {
            format!("declare -- {}=\"{}\"\n", var_name, val)
        } else {
            format!("declare: {}: not found\n", var_name)
        }
    } else if !args.is_empty() && args[0].contains('=') {
        if let Some((name, value)) = args[0].split_once('=') {
            let trimmed_name = name.trim();
            if is_valid_identifier(trimmed_name) {
                shell_variables.insert(trimmed_name.to_string(), value.to_string());
                String::new()
            } else {
                format!("declare: `{}': not a valid identifier\n", args[0])
            }
        } else {
            String::new()
        }
    } else if !args.is_empty() {
        if !is_valid_identifier(&args[0]) {
            format!("declare: `{}': not a valid identifier\n", args[0])
        } else {
            String::new()
        }
    } else {
        String::new()
    }
}

struct ShellHelper {
    last_prefix: RefCell<String>,
    tab_count: Cell<usize>,
    completions: Arc<Mutex<HashMap<String, String>>>, 
}

impl Helper for ShellHelper {}
impl Hinter for ShellHelper { type Hint = String; }
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
        // --- DEBUG PRINTS FOR TRACING ---
        eprintln!("LINE='{}' POS={}", line, pos);

        let prefix = &line[..pos];
        let cmd_name = line.split_whitespace().next().unwrap_or("");
        
        // 1. Unify Token and Replacement Position Extraction
        let last_space = prefix.rfind(' ');
        let token = match last_space {
            Some(i) => &prefix[i + 1..],
            None => prefix,
        };
        let replace_pos = pos - token.len();

        // 2. Custom command completion scripts spec (-C)
        if let Ok(comps) = self.completions.lock() {
            if let Some(path) = comps.get(cmd_name) {
                let words: Vec<&str> = prefix.split_whitespace().collect();
                
                let arg1 = cmd_name;
                let arg2 = if prefix.ends_with(' ') { "" } else { words.last().cloned().unwrap_or("") };
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
                        return Ok((pos - arg2.len(), vec![Pair {
                            display: candidate.clone(),
                            replacement: format!("{} ", candidate),
                        }]));
                    } else if candidates.len() > 1 {
                        let mut lcp = candidates[0].clone();
                        for name in candidates.iter().skip(1) {
                            let mut common_len = 0;
                            for (c1, c2) in lcp.chars().zip(name.chars()) {
                                if c1 == c2 { common_len += c1.len_utf8(); } else { break; }
                            }
                            lcp.truncate(common_len);
                        }

                        let custom_replace_pos = pos - arg2.len();

                        if lcp.len() > arg2.len() {
                            return Ok((custom_replace_pos, vec![Pair {
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

        // 3. Command Completion (No paths inside the first word token)
        if last_space.is_none() && !token.contains('/') {
            let mut commands = vec![
                "echo".to_string(), "exit".to_string(), "type".to_string(),
                "pwd".to_string(), "cd".to_string(), "jobs".to_string(), "declare".to_string(),
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
            let matching_names: Vec<String> = commands.into_iter().filter(|cmd| cmd.starts_with(token)).collect();
            if matching_names.is_empty() { return Ok((pos, Vec::new())); }
            if matching_names.len() == 1 {
                let cmd = &matching_names[0];
                return Ok((replace_pos, vec![Pair { display: cmd.clone(), replacement: format!("{} ", cmd) }]));
            }
            let mut lcp = matching_names[0].clone();
            for name in matching_names.iter().skip(1) {
                let mut common_len = 0;
                for (c1, c2) in lcp.chars().zip(name.chars()) {
                    if c1 == c2 { common_len += c1.len_utf8(); } else { break; }
                }
                lcp.truncate(common_len);
            }
            if lcp.len() > token.len() {
                return Ok((replace_pos, vec![Pair { display: lcp.clone(), replacement: lcp }]));
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
                println!("{}", matching_names.join("  "));
                print!("$ {}", prefix);
                io::stdout().flush().unwrap();
                self.tab_count.set(0);
                return Ok((pos, Vec::new()));
            }
            return Ok((pos, Vec::new()));
        }

        // 4. Robust Path/Filename Completion Logic
        let (search_dir, partial_name, prefix_path) = if let Some((dir, file)) = token.rsplit_once('/') {
            (
                PathBuf::from(if dir.is_empty() { "/" } else { dir }),
                file,
                format!("{}/", dir),
            )
        } else {
            (
                env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
                token,
                String::new(),
            )
        };

        // --- DEBUG PRINTS FOR PATH TRACING ---
        eprintln!("TOKEN='{}'", token);
        eprintln!("SEARCH_DIR={:?}", search_dir);
        eprintln!("FILE_PART='{}'", partial_name);

        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&search_dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with('.') && !partial_name.starts_with('.') { continue; }
                    if name.starts_with(partial_name) {
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
            let replacement = format!(
                "{}{}{}",
                prefix_path,
                matched_file,
                if *is_dir { "/" } else { " " }
            );

            return Ok((
                replace_pos,
                vec![Pair {
                    display: replacement.clone(),
                    replacement,
                }],
            ));
        } else if files.len() > 1 {
            let mut lcp = files[0].0.clone();
            for (name, _) in files.iter().skip(1) {
                let mut common_len = 0;
                for (c1, c2) in lcp.chars().zip(name.chars()) {
                    if c1 == c2 { common_len += c1.len_utf8(); } else { break; }
                }
                lcp.truncate(common_len);
            }
            if lcp.len() > partial_name.len() {
                let replacement = format!("{}{}", prefix_path, lcp);
                return Ok((
                    replace_pos,
                    vec![Pair {
                        display: replacement.clone(),
                        replacement,
                    }],
                ));
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
                    if *is_dir { format!("{}/", name) } else { name.clone() }
                }).collect();
                println!("{}", display_names.join("  "));
                print!("$ {}", prefix);
                io::stdout().flush().unwrap();
                self.tab_count.set(0);
                return Ok((pos, Vec::new()));
            }
        }
        Ok((pos, Vec::new()))
    }
}

struct BgJob {
    job_id: u32,
    child: Child,
    command_str: String,
}

fn get_markers(bg_jobs: &[BgJob]) -> (Option<u32>, Option<u32>) {
    let mut ids: Vec<u32> = bg_jobs.iter().map(|j| j.job_id).collect();
    ids.sort_unstable();
    let current = ids.pop();
    let previous = ids.pop();
    (current, previous)
}

fn reap_jobs(bg_jobs: &mut Vec<BgJob>) {
    let mut i = 0;
    while i < bg_jobs.len() {
        match bg_jobs[i].child.try_wait() {
            Ok(Some(_status)) => {
                let removed_id = bg_jobs[i].job_id;
                let cmd_str = bg_jobs[i].command_str.clone();
                let (current_id, previous_id) = get_markers(bg_jobs);
                let marker = if current_id == Some(removed_id) { "+" } else if previous_id == Some(removed_id) { "-" } else { " " };
                println!("[{}]{}  Done                {} ", removed_id, marker, cmd_str);
                bg_jobs.remove(i);
            }
            Ok(None) => { i += 1; }
            Err(_) => { bg_jobs.remove(i); }
        }
    }
}

fn handle_pipeline(command_str: &str, shell_variables: &mut HashMap<String, String>) {
    let stages: Vec<&str> = command_str.split('|').collect();
    let num_stages = stages.len();
    if num_stages < 2 {
        return;
    }  

    let parse_cmd = |cmd_part: &str, vars: &HashMap<String, String>| -> Option<(String, Vec<String>)> {
        let args = parse_arguments(cmd_part, vars);
        if args.is_empty() {
            None
        } else {
            Some((args[0].clone(), args[1..].to_vec()))
        }
    };

    let is_builtin = |cmd: &str| -> bool {
        matches!(cmd, "echo" | "exit" | "type" | "pwd" | "cd" | "jobs" | "declare")
    };

    let mut previous_stdout: Option<std::process::ChildStdout> = None;
    let mut builtin_output_buffer: Option<String> = None;
    let mut children: Vec<Child> = Vec::new();

    for (i, stage) in stages.iter().enumerate() {
        let (cmd_name, cmd_args) = match parse_cmd(stage, shell_variables) {
            Some(val) => val,
            None => continue,
        };

        if is_builtin(&cmd_name) {
            let mut current_builtin_output = String::new();
            if cmd_name == "echo" {
                current_builtin_output = format!("{}\n", cmd_args.join(" "));
            } else if cmd_name == "pwd" {
                if let Ok(path) = env::current_dir() {
                    current_builtin_output = format!("{}\n", path.display());
                }
            } else if cmd_name == "declare" {
                current_builtin_output = execute_declare(&cmd_args, shell_variables);
            } else if cmd_name == "type" && !cmd_args.is_empty() {
                let arg = &cmd_args[0];
                if is_builtin(arg) {
                    current_builtin_output = format!("{} is a shell builtin\n", arg);
                } else if let Some(path) = find_executable(arg) {
                    current_builtin_output = format!("{} is {}\n", arg, path.display());
                } else {
                    current_builtin_output = format!("{}: not found\n", arg);
                }
            }     
            if i == num_stages - 1 {
                if current_builtin_output.contains("not a valid identifier") || current_builtin_output.contains("not found") {
                    eprint!("{}", current_builtin_output);
                    let _ = io::stderr().flush();
                } else {
                    print!("{}", current_builtin_output);
                    let _ = io::stdout().flush();
                }
            } else {
                builtin_output_buffer = Some(current_builtin_output);
            }
            continue;
        }

        let cmd_path = match find_executable(&cmd_name) {
            Some(path) => path,
            None {
                println!("{}: command not found", cmd_name);
                return;
            }
        };

        let mut cmd = Command::new(cmd_path);
        cmd.args(&cmd_args);
        if builtin_output_buffer.is_some() {
            cmd.stdin(Stdio::piped());
        } else if let Some(stdout) = previous_stdout.take() {
            cmd.stdin(Stdio::from(stdout));
        } else {
            cmd.stdin(Stdio::inherit());
        }

        if i < num_stages - 1 {
            cmd.stdout(Stdio::piped());
        } else {
            cmd.stdout(Stdio::inherit());
        }

        match cmd.spawn() {
            Ok(mut child) => {
                if let Some(buf) = builtin_output_buffer.take() {
                    if let Some(mut stdin) = child.stdin.take() {
                        let _ = stdin.write_all(buf.as_bytes());
                    }
                }
                if i < num_stages - 1 {
                    previous_stdout = child.stdout.take();
                }
                children.push(child);
            }
            Err(_) => {
                println!("{}: command not found", cmd_name);
                return;
            }
        }
    }

    for mut child in children {
        let _ = child.wait();
    }
}

fn main() {
    let completions: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
    let mut r1 = Editor::<ShellHelper, DefaultHistory>::new().unwrap();
    let mut bg_jobs: Vec<BgJob> = Vec::new();
    let mut shell_variables: HashMap<String, String> = HashMap::new();
    let mut custom_history: Vec<String> = Vec::new();

    if let Ok(hist_file) = env::var("HISTFILE") {
        use std::io::{BufRead, BufReader};
        if let Ok(file) = File::open(&hist_file) {
            let reader = BufReader::new(file);
            for line in reader.lines().flatten() {
                if !line.trim().is_empty() {
                    let _ = r1.add_history_entry(&line);
                    custom_history.push(line);
                }
            }
        }
    }

    r1.set_helper(Some(ShellHelper {
        last_prefix: RefCell::new(String::new()),
        tab_count: Cell::new(0),
        completions: Arc::clone(&completions),
    }));
    let mut last_appended_idx = custom_history.len();

    loop {
        reap_jobs(&mut bg_jobs);

        let command = match r1.readline("$ ") {
            Ok(line) => line.trim().to_string(),
            Err(_) => break,
        };
        if command.is_empty() {
            continue;
        }
        let _ = r1.add_history_entry(&command);
        custom_history.push(command.clone());

        if command.contains('|'){
            handle_pipeline(&command, &mut shell_variables);
            continue;
        }
        let parts = parse_arguments(&command, &shell_variables);
        if parts.is_empty() {
            continue;
        }

        let mut is_background = false;
        let mut final_parts = parts.clone();
        if final_parts.last().map(|s| s.as_str()) == Some("&") {
            is_background = true;
            final_parts.pop();
        }

        if final_parts.is_empty() {
            continue;
        }

        let cmd_name = final_parts[0].trim().to_string();
        let mut stdout_file = None;
        let mut stderr_file = None;
        let mut append_stdout = false;
        let mut append_stderr = false;
        let mut args = Vec::new();
        let mut idx = 1;
        while idx < final_parts.len() {
            if final_parts[idx] == ">" || final_parts[idx] == "1>" {
                stdout_file = Some(final_parts[idx + 1].clone());
                idx += 2;
                continue;
            } else if final_parts[idx] == ">>" || final_parts[idx] == "1>>" {
                stdout_file = Some(final_parts[idx + 1].clone());
                append_stdout = true;
                idx += 2;
                continue;
            } else if final_parts[idx] == "2>" {
                stderr_file = Some(final_parts[idx + 1].clone());
                idx += 2;
                continue;
            } else if final_parts[idx] == "2>>" {
                stderr_file = Some(final_parts[idx + 1].clone());
                append_stderr = true;
                idx += 2;
                continue;
            }
            args.push(final_parts[idx].clone());
            idx += 1;
        }

        if cmd_name == "exit" {
            if let Ok(hist_file) = env::var("HISTFILE") {
                if let Ok(mut file) = File::create(&hist_file) {
                    for entry in &custom_history {
                        if let Err(e) = writeln!(file, "{}", entry) {
                            eprintln!("history: error writing to HISTFILE on exit: {}", e);
                            break;
                        }
                    }
                }
            }
            break;
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
            if let Some(file_name) = &stderr_file {
                if append_stderr {
                    let _file = OpenOptions::new().create(true).append(true).open(file_name).unwrap();
                } else {
                    let _file = File::create(file_name).unwrap();
                }
            }
       } else if cmd_name == "history" {
            if args.len() >= 2 && args[0] == "-a" {
                let file_path = &args[1];
                match OpenOptions::new().create(true).append(true).open(file_path) {
                    Ok(mut file) => {
                        for entry in custom_history.iter().skip(last_appended_idx) {
                            if let Err(e) = writeln!(file, "{}", entry) {
                                eprintln!("history: error appending to file: {}", e);
                                break;
                            }
                        }
                        last_appended_idx = custom_history.len();
                    }
                    Err(e) => {
                        eprintln!("history: {}: {}", file_path, e);
                    }
                }
            } else if args.len() >= 2 && args[0] == "-r" {
                let file_path = &args[1];
                use std::fs::File;
                use std::io::{BufRead, BufReader};
                
                if let Ok(file) = File::open(file_path) {
                    let reader = BufReader::new(file);
                    for line in reader.lines().flatten() {
                        if !line.trim().is_empty() {
                            let _ = r1.add_history_entry(&line);
                            custom_history.push(line);
                        }
                    }
                    last_appended_idx = custom_history.len();
                } else {
                    eprintln!("history: {}: No such file or directory", file_path);
                }
            } else if args.len() >= 2 && args[0] == "-w" {
                let file_path = &args[1];
                match File::create(file_path) {
                    Ok(mut file) => {
                        for entry in &custom_history {
                            if let Err(e) = writeln!(file, "{}", entry) {
                                eprintln!("history: error writing to file: {}", e);
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("history: {}: {}", file_path, e);
                    }
                }
            } else {
                let total_entries = custom_history.len();
                let limit = if !args.is_empty() {
                    args[0].parse::<usize>().unwrap_or(total_entries)
                } else {
                    total_entries
                };
                let skip_count = total_entries.saturating_sub(limit);
                for (index, entry) in custom_history.iter().enumerate().skip(skip_count) {
                    println!("{:>5}  {}", index + 1, entry);
                }
            }
        } else if cmd_name == "declare" {
            let output = execute_declare(&args, &mut shell_variables);
            if !output.is_empty() {
                if output.contains("not a valid identifier") || output.contains("not found") {
                    eprint!("{}", output);
                    let _ = io::stderr().flush();
                } else {
                    print!("{}", output);
                    let _ = io::stdout().flush();
                }
            }
        } else if cmd_name == "type" {
             if args.is_empty() {
                println!("type: missing arguments");
                continue;
            }
            let arg = &args[0];
            if arg == "echo" || arg == "exit" || arg == "type" || arg == "pwd" || arg == "cd" || arg == "complete" || arg == "jobs" || arg == "history" || arg == "declare" {
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
                if let Ok(mut comps) = completions.lock() { comps.insert(cmd, path); }
            } else if args.len() >= 2 && args[0] == "-r" {
                let cmd = &args[1];
                if let Ok(mut comps) = completions.lock() { comps.remove(cmd); }
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
            if args.is_empty() { continue; }
            let dir = &args[0];
            if dir == "~" {
                if let Ok(home) = env::var("HOME") { env::set_current_dir(home).unwrap(); }
            } else if let Err(_) = env::set_current_dir(dir) {
                println!("cd: {}: No such file or directory", dir);
            }
        } else if cmd_name == "jobs" {
            let mut finished = Vec::new();
            let (current_id, previous_id) = get_markers(&bg_jobs);
            for job in &mut bg_jobs {
                let marker = if current_id == Some(job.job_id) { "+" } else if previous_id == Some(job.job_id) { "-" } else { " " };
                match job.child.try_wait() {
                    Ok(Some(_status)) => {
                        println!("[{}]{}  Done                 {}", job.job_id, marker, job.command_str);
                        finished.push(job.job_id);
                    }
                    Ok(None) => {
                        println!("[{}]{}  Running                {} &", job.job_id, marker, job.command_str);
                    }
                    Err(_) => { finished.push(job.job_id); }
                }
            }
            bg_jobs.retain(|j| !finished.contains(&j.job_id));
        } else {
            if let Some(_path) = find_executable(&cmd_name) {
                let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                let mut cmd = Command::new(&cmd_name);
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
                if is_background {
                    let next_job_id = if bg_jobs.is_empty() { 1 } else { bg_jobs.iter().map(|j| j.job_id).max().unwrap_or(0) + 1 };
                    println!("[{}] {}", next_job_id, child.id());
                    let trailing_args = args.join(" ");
                    let full_cmd_str = if trailing_args.is_empty() { cmd_name } else { format!("{} {}", cmd_name, trailing_args) };
                    bg_jobs.push(BgJob { job_id: next_job_id, child, command_str: full_cmd_str.trim().to_string() });
                } else {
                    child.wait().unwrap();
                }
            } else {
                println!("{}: command not found", cmd_name);
            }
        }
    }
}