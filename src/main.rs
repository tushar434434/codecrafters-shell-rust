//dont change my 
#![allow(unused_imports)]
use std::io::{self, Write};
use std::env;
use std::path::PathBuf;
use std::os::unix::fs::PermissionsExt;
use std::process::Command; // Required to run external binaries
use std::fs::File;//for file reading and writing
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
//Hinter:Provides gray suggestions while typing.
//Completer:A trait (interface) that allows us to define our own tab-completion behavior.
//Editor: Provides readline functionality.
//Pair:Represents one completion suggestion.
//Context:Contains information about the current state of the line editor.
//Helper:A marker trait used by rustyline.
// Helper function to scan PATH for an executable
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
/*
#[derive(Default)]
struct ShellHelper{ //because self is immutable
    last_prefix:String,
    tab_count:usize,
}
*/
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

impl Hinter for ShellHelper {
    type Hint = String;
}

impl Highlighter for ShellHelper {}

impl Validator for ShellHelper {}

impl Completer for ShellHelper {
    type Candidate = Pair;
/*
    fn complete(
        &self,
        line: &str,
        pos: usize,
        _: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {

        let builtins = ["echo", "exit"];

        let prefix = &line[..pos];

        let matches = builtins
            .iter()
            .filter(|cmd| cmd.starts_with(prefix))
            .map(|cmd| Pair {
                display: cmd.to_string(),
                replacement: format!("{} ", cmd),
            })
            .collect::<Vec<Pair>>();

        Ok((0, matches))
    }
}
*/
fn complete(
    &self,
    line: &str,
    pos: usize,
    _: &Context<'_>,
) -> rustyline::Result<(usize, Vec<Pair>)> {
    let prefix = &line[..pos];

    // Check if we are completing an argument or a command name
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
            } else if self.tab_count.get() == 2 {
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

    // Start with builtins
    let mut commands = vec![
        "echo".to_string(),
        "exit".to_string(),
        "type".to_string(),
        "pwd".to_string(),
        "cd".to_string(),
    ];
    // Add executables from PATH
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
    } else if self.tab_count.get() == 2 {
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
fn main() {
    /*
    loop {
        print!("$ ");
        io::stdout().flush().unwrap();

        let mut command = String::new();
        io::stdin().read_line(&mut command).unwrap();
        command = command.trim().to_string();
*/
        let mut r1 = Editor::<ShellHelper,DefaultHistory>::new().unwrap();
        r1.set_helper(Some(ShellHelper::default()));
        loop{
            let command = match r1.readline("$ "){
                Ok(line) => line.trim().to_string(),
                Err(_)=>break,
            };
        if command.is_empty() {
            continue;
        }

        // 1. Split the command string into parts (program name + arguments)
        // let parts: Vec<&str> = command.split_whitespace().collect();

        let mut parts: Vec<String> = Vec::new();
        let mut current = String::new();
        let mut in_quotes = false;
        let mut double_quotes =false;
        let mut escape=false;

        for c in command.chars() {
            if escape {
                current.push(c);
                escape = false;
            }

            /* else if c == '\\' && !in_quotes && !double_quotes {
                escape = true;
            }*/else if c == '\\' {

                if double_quotes {

                    // In double quotes, only " and \ are escapable in this stage
                    escape = true;

                } else if !in_quotes {

                    // Outside quotes, everything can be escaped
                    escape = true;

                } else {

                    // Inside single quotes, backslash is literal
                    current.push(c);

                }
            }

            else if c == '\'' && !double_quotes {
                in_quotes = !in_quotes;
            }
            else if c == '"' && !in_quotes{
                double_quotes =! double_quotes;
            }
            else if c.is_whitespace() && !in_quotes && !double_quotes {
                if !current.is_empty() {
                    parts.push(current.clone());
                    current.clear();
                }
            }
            else {
                current.push(c);
            }
        }

        if !current.is_empty() {
            parts.push(current);
        }

        let cmd_name = &parts[0];
        //   let args = &parts[1..];
        // let mut redirect_file=None;
        let mut stdout_file=None;
        let mut stderr_file=None;
        let mut append_stdout=false;
        let mut append_stderr=false;

        let mut args =Vec::new();
        let mut i=1;
        /*
        while i < parts.len(){
           if parts[i] == ">" || parts[i]=="1>"{
               stdout_file =Some(parts[i+1].clone());
               break;
           }
           else if parts[i]=="2>"{
               stderr_file=Some(parts[i+1].clone());
               break;
           }

           args.push(parts[i].clone());
           i+=1;
        }*/ while i < parts.len(){
            if parts[i] == ">" || parts[i]=="1>"{
                stdout_file =Some(parts[i+1].clone());
                i+=2;
                continue;
            }
            else if parts[i] == ">>" || parts[i] == "1>>"{
                stdout_file =Some(parts[i+1].clone());
                append_stdout=true;
                i+=2;
                continue;
            }/*
            else if parts[i]=="2>"{
             stderr_file=Some(parts[i+1].clone());
             i+=2;
             continue;*/
            else if parts[i]=="2>"{
                stderr_file=Some(parts[i+1].clone());
                i+=2;
                continue;
            }
            else if parts[i]=="2>>"{
                stderr_file=Some(parts[i+1].clone());
                append_stderr=true;
                i+=2;
                continue;
            }
            

            args.push(parts[i].clone());
            i+=1;
        }
        // 2. Evaluate builtins or look for external commands
        if cmd_name == "exit" {
            break;
        }/*
        else if cmd_name == "echo" {
           // println!("{}", args.join(" "));
           let output =args.join(" ");
           if let Some(file_name) = &stdout_file{
            std::fs::write(file_name,format!("{}\n",output)).unwrap();
           }
           else {
            println!("{}",output);
           }
        }*/
        else if cmd_name == "echo" {
            // println!("{}", args.join(" "));
            let output =args.join(" ");

            if let Some(file_name) = &stdout_file{
                if append_stdout{
                    let mut file = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(file_name)
                        .unwrap();

                    writeln!(file,"{}",output).unwrap();
                }
                else{
                    std::fs::write(file_name,format!("{}\n",output)).unwrap();
                }
            }
            else {
                println!("{}",output);
            }

            // if let Some(file_name) = &stderr_file{
                // let _file = File::create(file_name).unwrap();//agr file nhi hai to nai bana do
            //}
            if let Some(file_name) = &stderr_file{
                if append_stderr{
                    let _file = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(file_name)
                        .unwrap();
                }
                else{
                    let _file = File::create(file_name).unwrap();//agr file nhi hai to nai bana do
                }
            }
        }
        else if cmd_name == "type" {
            let arg = &args[0];

            if arg == "echo" || arg == "exit" || arg == "type" || arg == "pwd" || arg == "cd"||arg=="complete" {
                println!("{} is a shell builtin", arg);
            }
            else if let Some(path) = find_executable(arg) {
                println!("{} is {}", arg, path.display());
            }
            else {
                println!("{}: not found", arg);
            }
        }
        else if cmd_name == "complete" {
         if args.len() >= 2 && args[0] == "-p" {
        let cmd = &args[1];
        println!("complete: {}: no completion specification", cmd);
         } else {
        if !args.is_empty() && args[0] != "-p" {
            eprintln!("complete: flags other than -p are not yet supported");
            }
            }
        }
        else if cmd_name == "pwd" {
            match env::current_dir() { //builtin function hota hai
                Ok(path) => println!("{}", path.display()), //agr path hai to dispaly kr diya hai
                Err(_) => eprintln!("pwd: unable to get current directory"), //agr path ni milla to error handling kr li
            }
        }
        else if cmd_name == "cd" {
            let dir = &args[0];

            if dir == "~" {
                if let Ok(home) = env::var("HOME") {
                    env::set_current_dir(home).unwrap();
                }
            }

            else if let Err(_) = env::set_current_dir(dir) { //"Please make /usr/local/bin the current working directory."
                //if successfull it will return Ok(()) otherwise will give error
                println!("cd: {}: No such file or directory", dir);
            }
        }

        else {
            // 3. Global fallback: Check if the base command exists in PATH
            if let Some(_path) = find_executable(cmd_name) {

                /* let args_ref: Vec<&str> = args
                       .iter()
                       .map(|s| s.as_str())
                       .collect();

                   // Spawn the process using the command name and pass the arguments slice
                   let mut child = Command::new(cmd_name)
                       .args(args_ref)
                       .spawn()
                       .unwrap();

                   // Wait for the program to finish before displaying the next prompt
                   child.wait().unwrap();*/
                let args_ref: Vec<&str> = args
                        .iter()
                        .map(|s| s.as_str())
                        .collect();
                let mut cmd = Command::new(cmd_name);
                cmd.args(args_ref);
                /*
                if let Some(file_name) = &redirect_file {
                let file = File::create(file_name).unwrap();
                cmd.stdout(Stdio::from(file));
                }
                */
                /*
                if let Some(file_name) = &stdout_file {
                let file = File::create(file_name).unwrap();//agr file nhi hai to nai bana do
                cmd.stdout(Stdio::from(file));
                }

                if let Some(file_name) = &stderr_file {
                let file = File::create(file_name).unwrap();
                cmd.stderr(Stdio::from(file));
                }*/
                
                if let Some(file_name) = &stdout_file {
                    if append_stdout{
                        let file = OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(file_name)
                            .unwrap();

                        cmd.stdout(Stdio::from(file));
                    }
                    else{
                        let file = File::create(file_name).unwrap();//agr file nhi hai to nai bana do
                        cmd.stdout(Stdio::from(file));
                    }
                }/*
                if let Some(file_name) = &stderr_file {
                    let file = File::create(file_name).unwrap();
                    cmd.stderr(Stdio::from(file));*/
                if let Some(file_name) = &stderr_file {
                    if append_stderr{
                        let file = OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(file_name)
                            .unwrap();

                        cmd.stderr(Stdio::from(file));
                    }
                    else{
                        let file = File::create(file_name).unwrap();
                        cmd.stderr(Stdio::from(file));
                    }
                }
                // Spawn the process using the command name and pass the arguments slice
                let mut child = cmd
                    .spawn()
                    .unwrap();

                // Wait for the program to finish before displaying the next prompt
                child.wait().unwrap();
            }
            else {
                println!("{}: command not found", command);
            }
        }
    }
}