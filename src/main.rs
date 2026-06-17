#[allow(unused_imports)]
use std::io::{self, Write};
use std::env;

fn main() {
    loop {
        print!("$ "); // Keep this exactly "$ "
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let command = input.trim();

        if command == "exit" {
            break;
        } else if command.starts_with("echo ") {
            println!("{}", &command[5..]);
        } else if command.starts_with("type ") {
            // Grab the argument after "type "
            let arg = &command[5..]; 
            
            // 1. Check if the argument is one of our known builtins
            if arg == "exit" || arg == "echo" || arg == "type" {
                println!("{} is a shell builtin", arg);
            } else {
                // 2. Not a builtin? Scan the PATH directories for the file
                let mut found = false;
                
                if let Ok(path_env) = env::var("PATH") { // Must be uppercase "PATH"
                    for path in env::split_paths(&path_env) { // Fixed typo to split_paths
                        let exe_path = path.join(arg);
                        if exe_path.exists() {
                            println!("{} is {}", arg, exe_path.display());
                            found = true;
                            break; 
                        }
                    }
                }
                
                // 3. If missing from every scanned path directory
                if !found {
                    println!("{}: not found", arg); 
                }
            }
        } else {
            // Global fallback for executing an unrecognized command directly
            println!("{}: command not found", command);
        }
    }
}