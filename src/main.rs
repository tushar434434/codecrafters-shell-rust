#[allow(unused_imports)]
use std::io::{self, Write};

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
            
            // Check if the argument is one of our known builtins
            if arg == "exit" || arg == "echo" || arg == "type" {
                println!("{} is a shell builtin", arg);
            } else {
                println!("{}: not found", arg); // Note the exact error format required for this stage
            }
        } else {
            let mut found =false;
            if let Ok(path_env) = env::var("Path"){
                for path in env::splits_path(&path_env){
                    let exe_path=path.join(args);
                    if exe_path.exists(){
                       println!("{} is {}", arg, exe_path.display());
                            found = true;
                            break; 
                    }
                }
            }
            if !found {
                    println!("{}: not found", arg);
                }
            }
            else {
            println!("{}: command not found", command);
        }
    }
}