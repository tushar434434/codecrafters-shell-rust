#[allow(unused_imports)]
use std::io::{self, Write};

fn main() {
    loop{
    // TODO: Uncomment the code below to pass the first stage
     print!("$ type");
     io::stdout().flush().unwrap();
     let mut command = String::new();
     io::stdin().read_line(&mut command).unwrap();
    /* if command.trim() == "exit"{
        break;
     }
     else if command.starts_with("echo ") {//
    println!("{}", &command[5..].trim());//trim otherwise give error
}
     else{
     println!("{}: command not found",command.trim());
    }*/
    if command.trim == "type" || "echo" || "echo" {
        println!("{} is shell builtin",command.trim() );
    }
    else {
        println!("{}: command not found",command.trim());
    }
    }
    }


