use rustyline::completion::{Completer, FilenameCompleter, Pair};
use rustyline::history::DefaultHistory;
use rustyline::{Changeset, Config, Editor};
use rustyline_derive::{Helper, Highlighter, Hinter, Validator};
#[allow(unused_imports)]
use std::collections::HashMap;
use std::env::{current_dir, set_current_dir};
use std::ffi::OsStr;
use std::fs::{File, read_dir};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::usize::MAX;
use std::{env, fmt};

struct CommandList;

#[derive(Helper, Highlighter, Hinter, Validator)]
struct ShellHelper {
    file_comp: FilenameCompleter,
}

impl ShellHelper {
    fn new() -> Self {
        Self {
            file_comp: FilenameCompleter::new(),
        }
    }
}

impl Completer for ShellHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let vec_of_commands = vec!["echo", "exit", "type", "pwd", "cd", "cat"];
        let mut candidates = Vec::new();
        
        // Isolate the prefix up to the cursor position
        let prefix = &line[..pos];

        // Check if we are completing a command argument or the base command
        if let Some(last_space_idx) = prefix.rfind(' ') {
            // ARGUMENT COMPLETION
            let (file_pos, mut file_candidates) = self.file_comp.complete(line, pos, ctx)?;
            
            for pair in &mut file_candidates {
                if !pair.replacement.ends_with('/') {
                    pair.replacement.push(' ');
                }
            }
            
            file_candidates.sort_by(|a, b| a.display.cmp(&b.display));
            file_candidates.dedup_by(|a, b| a.display == b.display);
            
            return Ok((file_pos, file_candidates));
        }

        // COMMAND COMPLETION
        if let Ok(paths) = std::env::var("PATH") {
            for directory in env::split_paths(&paths) {
                if let Ok(entries) = std::fs::read_dir(directory) {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().into_owned();
                        if name.starts_with(prefix) {
                            candidates.push(Pair {
                                display: format!("{} ", name),
                                replacement: format!("{} ", name),
                            });
                        }
                    }
                }
            }
        }

        for cmd in vec_of_commands {
            if cmd.starts_with(prefix) {
                candidates.push(Pair {
                    display: format!("{} ", cmd),
                    replacement: format!("{} ", cmd),
                });
            }
        }

        candidates.sort_by(|a, b| a.display.cmp(&b.display));
        candidates.dedup_by(|a, b| a.display == b.display);

        Ok((0, candidates))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CommandType {
    BuiltIn,
    Executable(String),
}

impl fmt::Display for CommandType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandType::BuiltIn => write!(f, "builtin"),
            CommandType::Executable(_path) => write!(f, "executable"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CommandName {
    Exit,
    Echo(String),
    Type(String, CommandType),
    NoOp,
    Exec,
    Pwd(String),
    Cd,
    Cat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandResult {
    name: CommandName,
    cmd_type: CommandType,
}

impl CommandResult {
    pub const EXIT: CommandResult = CommandResult {
        name: CommandName::Exit,
        cmd_type: CommandType::BuiltIn,
    };
    pub const NOOP: CommandResult = CommandResult {
        name: CommandName::NoOp,
        cmd_type: CommandType::BuiltIn,
    };
    pub const CD: CommandResult = CommandResult {
        name: CommandName::Cd,
        cmd_type: CommandType::BuiltIn,
    };
    pub const CAT: CommandResult = CommandResult {
        name: CommandName::Cat,
        cmd_type: CommandType::BuiltIn,
    };

    pub fn new_echo(msg: String) -> CommandResult {
        CommandResult {
            name: CommandName::Echo(msg),
            cmd_type: CommandType::BuiltIn,
        }
    }
}

enum CommandNotFound {
    NotFound(String),
}

impl CommandList {
    fn new() -> Self {
        Self
    }

    fn execute_command(
        &self,
        command_vec: &Vec<&str>,
    ) -> std::result::Result<CommandResult, CommandNotFound> {
        if command_vec.is_empty() || command_vec[0].is_empty() {
            return Ok(CommandResult::NOOP);
        }

        if command_vec.contains(&">") || command_vec.contains(&"1>") {
            self.redirect_stdout(command_vec);
            return Ok(CommandResult::NOOP);
        } else if command_vec.contains(&">>") || command_vec.contains(&"1>>") {
            self.append_stdout(command_vec);
            return Ok(CommandResult::NOOP);
        } else if command_vec.contains(&"2>") {
            self.redirect_stderr(command_vec);
            return Ok(CommandResult::NOOP);
        } else if command_vec.contains(&"2>>") {
            self.append_stderr(command_vec);
            return Ok(CommandResult::NOOP);
        }

        match command_vec[0] {
            "exit" => {
                self.exit_cmd();
                Ok(CommandResult::EXIT)
            }
            "echo" => match command_vec.get(1..) {
                Some(expression) => self.echo_cmd(expression),
                None => Err(CommandNotFound::NotFound(format!(
                    "Command {} not implemented",
                    command_vec[0]
                ))),
            },
            "cat" => match command_vec.get(1..) {
                Some(expression) => self.cat_cmd(expression),
                None => Err(CommandNotFound::NotFound(format!(
                    "Command {} not implemented",
                    command_vec[0]
                ))),
            },
            "type" => {
                let clean_vec = self.clean_args(&command_vec);
                match clean_vec.get(1) {
                    Some(cmd) => self.type_cmd(cmd),
                    None => Err(CommandNotFound::NotFound(format!(
                        "Command {} not implemented",
                        command_vec[0]
                    ))),
                }
            }
            "pwd" => self.pwd_cmd(),
            "cd" => {
                let clean_vec = self.clean_args(&command_vec);
                match clean_vec.get(1) {
                    Some(dir) => self.cd_cmd(dir),
                    None => Err(CommandNotFound::NotFound(format!(
                        "Command {} not implemented",
                        command_vec[0]
                    ))),
                }
            }

            _ => {
                let parsed_arguments = self.parse_arguments(&command_vec);
                match self.check_if_file_executable(&parsed_arguments[0]) {
                    Some(map) => match parsed_arguments.get(1..) {
                        Some(string_of_options) => {
                            let res: CommandResult = CommandResult {
                                name: CommandName::Exec,
                                cmd_type: CommandType::Executable("".to_string()),
                            };

                            for (key, val) in map.iter() {
                                let output_res = Command::new(key.to_string())
                                    .args(string_of_options.iter().map(OsStr::new))
                                    .status();

                                match output_res {
                                    Ok(_) => {
                                        CommandResult {
                                            name: CommandName::Exec,
                                            cmd_type: CommandType::Executable(val.to_string()),
                                        };
                                    }
                                    Err(_) => {
                                        return Err(CommandNotFound::NotFound(format!(
                                            "Failed to execute {}",
                                            key
                                        )));
                                    }
                                }
                            }
                            Ok(res)
                        }
                        None => Err(CommandNotFound::NotFound(format!(
                            "{}: not found",
                            command_vec[0]
                        ))),
                    },
                    None => Err(CommandNotFound::NotFound(format!(
                        "{}: not found",
                        command_vec[0]
                    ))),
                }
            }
        }
    }

    fn eval_command(
        &self,
        editor: &mut Editor<ShellHelper, DefaultHistory>,
    ) -> std::result::Result<CommandResult, CommandNotFound> {
        let line = editor.readline("$ ");
        match line {
            Ok(input) => {
                let cleaned = input.trim();
                if cleaned.is_empty() {
                    return Ok(CommandResult::NOOP);
                }
                let parts: Vec<&str> = cleaned.split(' ').collect();
                self.execute_command(&parts)
            }
            Err(e) => Err(CommandNotFound::NotFound(format!(
                "Erro de leitura: {}",
                e.to_string()
            ))),
        }
    }

    fn append_stderr(&self, command_vec: &Vec<&str>) {
        let split_position = match command_vec.iter().position(|string| *string == "2>>") {
            Some(pos) => pos,
            None => MAX,
        };

        let first_part = &command_vec[0..split_position];
        let second_part = &command_vec[split_position..];

        let command = self.parse_arguments(first_part);

        let output_res = Command::new(command[0].clone())
            .args(command[1..].iter().map(OsStr::new))
            .stdout(Stdio::inherit())
            .output();

        let path_string = second_part[1..].join(" ");
        let path = Path::new(&path_string);

        let output = output_res.expect("Falha ao executar o comando");

        if path.exists() {
            let buffer = File::options().append(true).open(path);

            if let Ok(mut file_descriptor) = buffer {
                file_descriptor
                    .write(&output.stderr)
                    .expect("Falha ao escrever no arquivo");
            }
        } else {
            let mut buffer = File::create(path).expect("Falha ao criar arquivo");
            buffer
                .write_all(&output.stderr)
                .expect("Falha ao escrever no arquivo");
        }
    }

    fn append_stdout(&self, command_vec: &Vec<&str>) {
        let split_position = match command_vec
            .iter()
            .position(|string| *string == ">>" || *string == "1>>")
        {
            Some(pos) => pos,
            None => MAX,
        };

        let first_part = &command_vec[0..split_position];
        let second_part = &command_vec[split_position..];

        let command = self.parse_arguments(first_part);

        let output_res = Command::new(command[0].clone())
            .args(command[1..].iter().map(OsStr::new))
            .stderr(Stdio::inherit())
            .output();

        let path_string = second_part[1..].join(" ");
        let path = Path::new(&path_string);

        let output = output_res.expect("Falha ao executar o comando");

        if path.exists() {
            let buffer = File::options().append(true).open(path);

            if let Ok(mut file_descriptor) = buffer {
                file_descriptor
                    .write(&output.stdout)
                    .expect("Falha ao escrever no arquivo");
            }
        } else {
            let mut buffer = File::create(path).expect("Falha ao criar arquivo");
            buffer
                .write_all(&output.stdout)
                .expect("Falha ao escrever no arquivo");
        }
    }

    fn redirect_stderr(&self, command_vec: &Vec<&str>) {
        let split_position = match command_vec.iter().position(|string| *string == "2>") {
            Some(pos) => pos,
            None => MAX,
        };

        let first_part = &command_vec[0..split_position];
        let second_part = &command_vec[split_position..];

        let command = self.parse_arguments(first_part);

        let output_res = Command::new(command[0].clone())
            .args(command[1..].iter().map(OsStr::new))
            .stdout(Stdio::inherit())
            .output();

        let path_string = second_part[1..].join(" ");
        let path = Path::new(&path_string);

        let output = output_res.expect("Falha ao executar o comando");

        let mut buffer = File::create(path).expect("Falha ao criar arquivo");
        buffer
            .write_all(&output.stderr)
            .expect("Falha ao escrever no arquivo");
    }

    fn redirect_stdout(&self, command_vec: &Vec<&str>) {
        let split_position = match command_vec
            .iter()
            .position(|string| *string == ">" || *string == "1>")
        {
            Some(pos) => pos,
            None => MAX,
        };

        let first_part = &command_vec[0..split_position];
        let second_part = &command_vec[split_position..];

        let command = self.parse_arguments(first_part);

        let output_res = Command::new(command[0].clone())
            .args(command[1..].iter().map(OsStr::new))
            .stderr(Stdio::inherit())
            .output();

        let path_string = second_part[1..].join(" ");
        let path = Path::new(&path_string);

        let output = output_res.expect("Falha ao executar o comando");

        let mut buffer = File::create(path).expect("Falha ao criar arquivo");
        buffer
            .write_all(&output.stdout)
            .expect("Falha ao escrever no arquivo");
    }

    fn check_if_file_executable(&self, cmd: &str) -> Option<HashMap<String, String>> {
        if let Ok(paths) = std::env::var("PATH") {
            for directory in env::split_paths(&paths) {
                let full_path = directory.join(cmd);

                if let Ok(metadata) = std::fs::metadata(&full_path) {
                    if metadata.is_file() && (metadata.permissions().mode() & 0o111 != 0) {
                        let mut record = HashMap::new();
                        record.insert(cmd.to_string(), full_path.display().to_string());
                        return Some(record);
                    }
                }
            }
        }
        return None;
    }

    fn clean_args<'a>(&self, command_vec: &Vec<&'a str>) -> Vec<&'a str> {
        let clean_args: Vec<&str> = command_vec
            .iter()
            .copied()
            .filter(|s| !s.is_empty())
            .collect();

        clean_args
    }

    fn parse_arguments(&self, expression: &[&str]) -> Vec<String> {
        let full_expression = expression.join(" ");
        let mut args = Vec::new();
        let mut current_arg = String::new();

        let mut in_single = false;
        let mut in_double = false;
        let mut escaped = false;

        for c in full_expression.chars() {
            if escaped {
                if in_double {
                    if c == '"' || c == '\\' || c == '$' {
                        current_arg.push(c);
                    } else {
                        current_arg.push('\\');
                        current_arg.push(c);
                    }
                } else {
                    current_arg.push(c);
                }
                escaped = false;
                continue;
            }

            if c == '\\' && !in_single {
                escaped = true;
                continue;
            }

            if c == '\'' && !in_double {
                in_single = !in_single;
                continue;
            }

            if c == '"' && !in_single {
                in_double = !in_double;
                continue;
            }

            if c == ' ' && !in_single && !in_double {
                if !current_arg.is_empty() {
                    args.push(current_arg.clone());
                    current_arg.clear();
                }
                continue;
            }

            current_arg.push(c);
        }

        if !current_arg.is_empty() {
            args.push(current_arg);
        }

        args
    }

    fn exit_cmd(&self) {
        std::process::exit(0);
    }

    fn echo_cmd(&self, expression: &[&str]) -> std::result::Result<CommandResult, CommandNotFound> {
        let res_vec = self.parse_arguments(expression);
        let res_string = res_vec.join(" ");
        Ok(CommandResult::new_echo(res_string))
    }

    fn cat_cmd(&self, expression: &[&str]) -> std::result::Result<CommandResult, CommandNotFound> {
        let parsed_args = self.parse_arguments(expression);

        let output = std::process::Command::new("cat")
            .args(&parsed_args)
            .status();

        match output {
            Ok(_) => {
                return Ok(CommandResult {
                    name: CommandName::Cat,
                    cmd_type: CommandType::BuiltIn,
                });
            }
            Err(_) => return Err(CommandNotFound::NotFound(format!("Failed to execute cat"))),
        }
    }

    fn cd_cmd(&self, dir: &str) -> std::result::Result<CommandResult, CommandNotFound> {
        if dir == "~" {
            if let Ok(home) = std::env::var("HOME") {
                let dir_path = Path::new(home.as_str());
                match set_current_dir(dir_path) {
                    Ok(_) => return Ok(CommandResult::CD),
                    Err(_) => {
                        return Err(CommandNotFound::NotFound(format!("Failed to execute cd")));
                    }
                }
            }
        }

        let dir_path = Path::new(dir);
        match dir_path.is_dir() {
            true => match set_current_dir(dir_path) {
                Ok(_) => Ok(CommandResult::CD),
                Err(_) => Err(CommandNotFound::NotFound(format!("Failed to execute cd"))),
            },
            false => Err(CommandNotFound::NotFound(format!(
                "cd: {}: No such file or directory",
                dir_path.display().to_string()
            ))),
        }
    }

    fn pwd_cmd(&self) -> std::result::Result<CommandResult, CommandNotFound> {
        let path = current_dir();

        match path {
            Ok(p) => Ok(CommandResult {
                name: CommandName::Pwd(p.display().to_string()),
                cmd_type: CommandType::BuiltIn,
            }),
            Err(_) => Err(CommandNotFound::NotFound(format!("Failed to execute pwd"))),
        }
    }

    fn type_cmd(&self, cmd: &str) -> std::result::Result<CommandResult, CommandNotFound> {
        match cmd {
            "type" | "echo" | "exit" | "pwd" => Ok(CommandResult {
                name: CommandName::Type(cmd.to_string(), CommandType::BuiltIn),
                cmd_type: CommandType::BuiltIn,
            }),
            _ => {
                match self.check_if_file_executable(cmd) {
                    Some(map) => {
                        for (key, val) in map.iter() {
                            return Ok(CommandResult {
                                name: CommandName::Type(
                                    key.to_string(),
                                    CommandType::Executable(val.to_string()),
                                ),
                                cmd_type: CommandType::Executable(val.to_string()),
                            });
                        }
                    }
                    None => {}
                }

                Err(CommandNotFound::NotFound(format!("{}: not found", cmd)))
            }
        }
    }
}

fn main() {
    let commands = CommandList::new();
    let config = Config::builder()
        .completion_type(rustyline::CompletionType::List)
        .build();
    let mut editor: Editor<ShellHelper, DefaultHistory> =
        Editor::<ShellHelper, DefaultHistory>::with_config(config).unwrap();
    editor.set_helper(Some(ShellHelper::new()));

    loop {
        match commands.eval_command(&mut editor) {
            Ok(result) => match result {
                CommandResult::EXIT => {}
                CommandResult {
                    name: CommandName::Type(alvo, tipo),
                    ..
                } => match tipo {
                    CommandType::BuiltIn => println!("{} is a shell {}", alvo, tipo),
                    CommandType::Executable(path) => println!("{} is {}", alvo, path),
                },
                CommandResult::NOOP => {}
                CommandResult {
                    name: CommandName::Echo(msg),
                    ..
                } => println!("{}", msg),
                CommandResult {
                    name: CommandName::Exec,
                    ..
                } => {}
                CommandResult {
                    name: CommandName::Pwd(msg),
                    ..
                } => println!("{}", msg),
                CommandResult::CD => {}
                CommandResult::CAT => {}
            },

            Err(CommandNotFound::NotFound(msg)) => println!("{}", msg),
        }
    }
}