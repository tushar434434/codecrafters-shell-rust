#[allow(unused_imports)]
use std::io::{self, Write};

fn main() {
    loop{
    // TODO: Uncomment the code below to pass the first stage
     print!("$ ");
     io::stdout().flush().unwrap();
     let mut command = String::new();
     io::stdin().read_line(&mut command).unwrap();
     if command.trim() == "exit"{
        break;
     }
     else{
     println!("{}: command not found",command.trim());}
    }
    }
