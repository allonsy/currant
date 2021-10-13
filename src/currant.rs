use std::io::BufRead;
use std::io::BufReader;
use std::process;
use std::thread;

pub struct Command {
    name: String,
    command: String,
    args: Vec<String>,
}

pub struct CommandHandle {
    handle: thread::JoinHandle<()>,
}

impl CommandHandle {
    pub fn join(self) {
        self.handle
            .join()
            .unwrap_or_else(|_| panic!("Unable to join on handle"));
    }
}

impl Command {
    pub fn new(name: String, command: String, args: Vec<String>) -> Command {
        Command {
            name,
            command,
            args,
        }
    }
}

pub fn run_commands(commands: Vec<Command>) -> CommandHandle {
    let handle = thread::spawn(|| {
        let mut handles = Vec::new();
        for cmd in commands {
            handles.push(run_command(&cmd));
        }

        for handle in handles {
            handle
                .join()
                .unwrap_or_else(|_| panic!("Unable to join handle"));
        }
    });

    CommandHandle { handle }
}

pub fn run_command(command: &Command) -> thread::JoinHandle<()> {
    let mut command_process = process::Command::new(&command.command);
    command_process.args(&command.args);
    command_process.stdout(process::Stdio::piped());
    let command_name = command.name.clone();
    let mut cmd_handle = command_process
        .spawn()
        .unwrap_or_else(|_| panic!("Unable to spawn process: {}", command.command.clone()));

    thread::spawn(move || {
        let std_out = &mut cmd_handle.stdout;

        match std_out {
            Some(output) => {
                let mut buffered_stdout = BufReader::new(output);
                let mut line = String::new();

                let mut num_bytes_read = buffered_stdout
                    .read_line(&mut line)
                    .expect("Unable to read standard out");
                while num_bytes_read != 0 {
                    print!("{}: {}", command_name, line);
                    line = String::new();
                    num_bytes_read = buffered_stdout
                        .read_line(&mut line)
                        .expect("Unable to read standard out");
                }
            }
            None => {}
        }

        let exit_status = cmd_handle.wait();
        match exit_status {
            Ok(status) => println!(
                "currant: process {} exited with status {}",
                command_name, status
            ),
            Err(e) => panic!("Unable to wait for child process {}", e),
        }
    })
}
