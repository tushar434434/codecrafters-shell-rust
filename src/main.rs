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

// 1. Updated struct to include FilenameCompleter
struct ShellHelper {
    file_comp: FilenameCompleter,
    last_prefix: RefCell<String>,
    tab_count: Cell<usize>,
}

impl Default for ShellHelper {
    fn default() -> Self {
        Self {
            file_comp: FilenameCompleter::new(),
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

    fn complete(&self, line: &str, pos: usize, ctx: &Context<'_>) -> rustyline::Result<(usize, Vec<Pair>)> {
        // 2. Delegate file completion to the built-in FilenameCompleter
        let (file_pos, mut file_candidates) = self.file_comp.complete(line, pos, ctx)?;
        
        // 3. Apply the trailing space logic required by the shell
        for pair in &mut file_candidates {
            if !pair.replacement.ends_with('/') {
                pair.replacement.push(' ');
            }
        }
        
        Ok((file_pos, file_candidates))
    }
}
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

// ... (Keep your existing find_executable and main function logic)
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

            if arg == "echo" || arg == "exit" || arg == "type" || arg == "pwd" || arg == "cd" {
                println!("{} is a shell builtin", arg);
            }
            else if let Some(path) = find_executable(arg) {
                println!("{} is {}", arg, path.display());
            }
            else {
                println!("{}: not found", arg);
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