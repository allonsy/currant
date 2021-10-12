use std::io::BufRead;
use std::io::BufReader;
use std::process;
use std::thread;

pub struct Command {
    name: String,
    command: String,
}

pub struct CommandHandle {
    handle: thread::JoinHandle<()>,
}

impl Command {
    pub fn new(name: String, command: String) -> Command {
        Command { name, command }
    }
}

pub fn run_commands(commands: Vec<Command>) -> CommandHandle {
    let handle = thread::spawn(|| {
        for cmd in commands {
            run_command(&cmd);
        }
    });

    CommandHandle { handle }
}

pub fn run_command(command: &Command) {
    let arguments = command.command.split_whitespace().collect::<Vec<&str>>();
    if arguments.is_empty() {
        panic!("no arguments given to command");
    }

    let mut command_process = process::Command::new(arguments[0]);
    command_process.args(&arguments[1..]);
    command_process.stdout(process::Stdio::piped());
    let command_name = command.name.clone();
    let cmd_handle = command_process
        .spawn()
        .unwrap_or_else(|_| panic!("Unable to spawn process: {}", command.command.clone()));

    thread::spawn(move || {
        let std_out = cmd_handle.stdout;

        if std_out.is_some() {
            let mut buffered_stdout = BufReader::new(std_out.unwrap());
            let mut line = String::new();

            let mut num_bytes_read = buffered_stdout
                .read_line(&mut line)
                .expect("Unable to read standard out");
            while num_bytes_read != 0 {
                println!("{}: {}", command_name, line);
                line = String::new();
                num_bytes_read = buffered_stdout
                    .read_line(&mut line)
                    .expect("Unable to read standard out");
            }
        }
    });
}
