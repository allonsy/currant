use std::fs;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::process;
use std::sync::mpsc;
use std::thread;

pub struct Command {
    name: String,
    command: String,
    args: Vec<String>,
    output: OutputType,
}

pub enum OutputType {
    Stdout,
    Channel,
    File(fs::File),
}

impl OutputType {
    fn is_channel(&self) -> bool {
        match self {
            OutputType::Channel => true,
            _ => false,
        }
    }
}

pub struct OutputMessage {
    pub name: String,
    pub message: OutputMessagePayload,
}

pub enum OutputMessagePayload {
    Done(Option<i32>),
    Stdout(String),
    Stderr(String),
}

pub struct CommandHandle {
    handle: thread::JoinHandle<()>,
    channel: mpsc::Receiver<OutputMessage>,
}

impl CommandHandle {
    pub fn join(self) {
        self.handle
            .join()
            .unwrap_or_else(|_| panic!("Unable to join on handle"));
    }

    pub fn get_output_channel(&self) -> &mpsc::Receiver<OutputMessage> {
        &self.channel
    }
}

impl Command {
    pub fn new(name: String, command: String, args: Vec<String>, output: OutputType) -> Command {
        Command {
            name,
            command,
            args,
            output,
        }
    }
}

pub fn run_commands(commands: Vec<Command>) -> CommandHandle {
    let (send, recv) = mpsc::channel();

    let handle = thread::spawn(move || {
        let mut handles = Vec::new();
        for cmd in commands {
            handles.push(run_command(cmd, send.clone()));
        }

        for handle in handles {
            handle
                .join()
                .unwrap_or_else(|_| panic!("Unable to join handle"));
        }
    });

    CommandHandle {
        handle,
        channel: recv,
    }
}

pub fn run_command(
    command: Command,
    send_chan: mpsc::Sender<OutputMessage>,
) -> thread::JoinHandle<()> {
    let mut command_process = process::Command::new(&command.command);
    command_process.args(&command.args);
    command_process.stdout(process::Stdio::piped());
    let command_name = command.name.clone();
    let mut cmd_handle = command_process
        .spawn()
        .unwrap_or_else(|_| panic!("Unable to spawn process: {}", command.command.clone()));

    thread::spawn(move || {
        let mut command = command;
        let std_out = &mut cmd_handle.stdout;

        match std_out {
            Some(output) => {
                let mut buffered_stdout = BufReader::new(output);
                let mut line = String::new();

                let mut num_bytes_read = buffered_stdout
                    .read_line(&mut line)
                    .expect("Unable to read standard out");
                while num_bytes_read != 0 {
                    match command.output {
                        OutputType::Stdout => {
                            print!("{}: {}", command_name, line);
                        }
                        OutputType::File(ref mut file) => {
                            file.write_all(line.as_bytes())
                                .unwrap_or_else(|_| panic!("Unable to write to file!"));
                        }
                        OutputType::Channel => {
                            send_chan
                                .send(OutputMessage {
                                    name: command_name.clone(),
                                    message: OutputMessagePayload::Stdout(line.clone()),
                                })
                                .unwrap_or_else(|_| panic!("Unable to send to channel"));
                        }
                    }
                    line = String::new();
                    num_bytes_read = buffered_stdout
                        .read_line(&mut line)
                        .expect("Unable to read standard out");
                }

                match command.output {
                    OutputType::Stdout => {}
                    OutputType::File(ref mut file) => {
                        file.flush()
                            .unwrap_or_else(|_| panic!("unable to flush file!"));
                    }
                    OutputType::Channel => {}
                }
            }
            None => {}
        }

        let exit_status = cmd_handle.wait();

        if command.output.is_channel() {}
        match exit_status {
            Ok(status) => match command.output {
                OutputType::Stdout => println!(
                    "currant: process {} exited with status {}",
                    command_name, status
                ),
                OutputType::Channel => send_chan
                    .send(OutputMessage {
                        name: command_name.clone(),
                        message: OutputMessagePayload::Done(status.code()),
                    })
                    .unwrap_or_else(|_| panic!("Unable to send to channel")),
                _ => {}
            },
            Err(e) => panic!("Unable to wait for child process {}", e),
        }
    })
}
