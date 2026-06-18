#[allow(unused_imports)]
use std::io::{self, Write};
use std::env;
use std::path::PathBuf;
use std::os::unix::fs::PermissionsExt;
use std::process::Command; // Required to run external binaries

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

fn main() {
    loop {
        print!("$ ");
        io::stdout().flush().unwrap();

        let mut command = String::new();
        io::stdin().read_line(&mut command).unwrap();
        command = command.trim().to_string();

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

    else if c == '\\' && !in_quotes && !double_quotes {
        escape = true;
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
        let args = &parts[1..];

        // 2. Evaluate builtins or look for external commands
        if cmd_name == "exit" {
            break;
        }
        else if cmd_name == "echo" {
            println!("{}", args.join(" "));
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

                let args_ref: Vec<&str> = args
                    .iter()
                    .map(|s| s.as_str())
                    .collect();

                // Spawn the process using the command name and pass the arguments slice
                let mut child = Command::new(cmd_name)
                    .args(args_ref)
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