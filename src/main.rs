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
        let parts: Vec<&str> = command.split_whitespace().collect();
        let cmd_name = parts[0];
        let args = &parts[1..];

        // 2. Evaluate builtins or look for external commands
        if command == "exit" {
            break;
        } else if command.starts_with("echo ") {
            println!("{}", &command[5..]);
        } else if command.starts_with("type ") {
            let arg = &command[5..];

            if arg == "echo" || arg == "exit" || arg == "type" || arg=="pwd" {
                println!("{} is a shell builtin", arg);
            } else if let Some(path) = find_executable(arg) {
                println!("{} is {}", arg, path.display());
            } else {
                println!("{}: not found", arg);
            }
         } else if command == "pwd" {
                match env::current_dir(){
                    Ok(path)=>println!("{}",path.display()),
                    Err(_)=>eprintln!("pwd: unable to get current directory"),
                
            }
        } else {
            // 3. Global fallback: Check if the base command exists in PATH
            if let Some(_path) = find_executable(cmd_name) {
                // Spawn the process using the command name and pass the arguments slice
                let mut child = Command::new(cmd_name)
                    .args(args)
                    .spawn()
                    .unwrap();
                
                // Wait for the program to finish before displaying the next prompt
                child.wait().unwrap();
            } else {
                println!("{}: command not found", command);
            }
        }
    }
}