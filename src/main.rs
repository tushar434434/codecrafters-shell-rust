#[allow(unused_imports)]
use std::io::{self, Write};
use std::env;
// 1. Import PermissionsExt to check for executable permissions on Unix systems
use std::os::unix::fs::PermissionsExt;

fn main() {
    loop {
        print!("$ ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let command = input.trim();

        if command == "exit" {
            break;
        } else if command.starts_with("echo ") {
            println!("{}", &command[5..]);
        } else if command.starts_with("type ") {
            let arg = &command[5..]; 
            
            if arg == "exit" || arg == "echo" || arg == "type" {
                println!("{} is a shell builtin", arg);
            } else {
                let mut found = false;
                
                if let Ok(path_env) = env::var("PATH") {
                    for path in env::split_paths(&path_env) {
                        let exe_path = path.join(arg);
                        
                        // 2. Check if file exists AND has execute permissions
                        if exe_path.exists() {
                            if let Ok(metadata) = exe_path.metadata() {
                                let permissions = metadata.permissions();
                                // Check if the user execution bit (0o100) is set
                                if permissions.mode() & 0o111 != 0 {
                                    println!("{} is {}", arg, exe_path.display());
                                    found = true;
                                    break; 
                                }
                            }
                        }
                    }
                }
                
                if !found {
                    println!("{}: not found", arg); 
                }
            }
        } else {
            println!("{}: command not found", command);
        }
    }
}