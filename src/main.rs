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
struct ShellHelper;
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

fn complete(
&self,
line: &str,
pos: usize,
_: &Context<'_>,
) -> rustyline::Result<(usize, Vec<Pair>)> {
let prefix = &line[..pos];
// Start with builtins
let mut commands = vec![
"echo".to_string(),
"exit".to_string(),
        "type".to_string(),
        "pwd".to_string(),
        "cd".to_string(),
        "complete".to_string(),
];
// Add executables from PATH
if let Ok(path_env) = env::var("PATH") {
for dir in env::split_paths(&path_env) {//path ko split kr diya 
// Ignore invalid directories
if let Ok(entries) = std::fs::read_dir(dir) {//read each line
for entry in entries.flatten() {//each entry is one file
if let Some(name) = entry.file_name().to_str() {//extracting file name
                        commands.push(name.to_string());//command vector mein  jod diye sb ko
}
}
}
}
}

    commands.sort();
    commands.dedup();

let matches = commands
.iter()
.filter(|cmd| cmd.starts_with(prefix))//comparing kr rhe hai
.map(|cmd| Pair {// converting into pair display and replacement
display: cmd.clone(),
// Add a trailing space for completion
replacement: format!("{} ", cmd),
})
.collect::<Vec<Pair>>();

    // Track sequential tab presses
    let mut last_p = self.last_prefix.borrow_mut();
    if *last_p == prefix {
        self.tab_count.set(self.tab_count.get() + 1);
    } else {
        self.tab_count.set(1);
        *last_p = prefix.to_string();
    }

    if matches.len() > 1 {
        if self.tab_count.get() == 1 {
            // First tab press: ring the bell
            print!("\x07");
            io::stdout().flush().unwrap();
            return Ok((0, Vec::new()));
        } else if self.tab_count.get() == 2 {
            // Second tab press: print options cleanly alphabetically
            println!();
            let names: Vec<String> = matches.iter().map(|m| m.display.clone()).collect();
            println!("{}", names.join("  "));
            // Clear counts so the next tab cycle repeats safely
            self.tab_count.set(0);
            return Ok((0, Vec::new()));
        }
    }

Ok((0, matches))
}
}
fn main() {
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
else if c == '\\' {
if double_quotes { escape = true; } else if !in_quotes { escape = true; } else { current.push(c); }
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
let mut stdout_file=None;
let mut stderr_file=None;
let mut append_stdout=false;
let mut append_stderr=false;

let mut args =Vec::new();
let mut i=1;
while i < parts.len(){
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
}
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

if cmd_name == "exit" {
break;
}
else if cmd_name == "complete" {
    // Stage #NE7: Register as builtin
}
else if cmd_name == "echo" {
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
if let Some(file_name) = &stderr_file{
if append_stderr{
let _file = OpenOptions::new()
.create(true)
.append(true)
.open(file_name)
.unwrap();
}
else{
let _file = File::create(file_name).unwrap();
}
}
}
else if cmd_name == "type" {
let arg = &args[0];

if arg == "echo" || arg == "exit" || arg == "type" || arg == "pwd" || arg == "cd" || arg == "complete" {
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
match env::current_dir() {
Ok(path) => println!("{}", path.display()),
Err(_) => eprintln!("pwd: unable to get current directory"),
}
}
else if cmd_name == "cd" {
let dir = &args[0];
if dir == "~" {
if let Ok(home) = env::var("HOME") {
env::set_current_dir(home).unwrap();
}
}
else if let Err(_) = env::set_current_dir(dir) {
println!("cd: {}: No such file or directory", dir);
}
}
else {
if let Some(path) = find_executable(cmd_name) {
let args_ref: Vec<&str> = args
.iter()
.map(|s| s.as_str())
.collect();
let mut cmd = Command::new(path);
cmd.args(args_ref);
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
let file = File::create(file_name).unwrap();
cmd.stdout(Stdio::from(file));
}
}
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
let mut child = cmd
.spawn()
.unwrap();
child.wait().unwrap();
}
else {
println!("{}: command not found", command);
}
}
}
}