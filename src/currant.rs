use super::line_parse;
use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::process;
use std::sync::mpsc;
use std::thread;

pub struct Command {
    name: String,
    command: String,
    args: Vec<String>,
}

pub struct OutputMessage {
    pub name: String,
    pub message: OutputMessagePayload,
}

pub enum OutputMessagePayload {
    Done(Option<i32>),
    Stdout(line_parse::LineEnding, Vec<u8>),
    Stderr(line_parse::LineEnding, Vec<u8>),
    Error(io::Error),
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
    pub fn new(name: String, command: String, args: Vec<String>) -> Command {
        Command {
            name,
            command,
            args,
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
        let std_out = cmd_handle.stdout.take();
        let std_err = cmd_handle.stderr.take();
        let mut std_out_handle = None;
        let mut std_err_handle = None;

        if let Some(output) = std_out {
            let mut buffered_stdout = BufReader::new(output);
            let new_name = command_name.clone();
            let new_chan = send_chan.clone();
            std_out_handle = Some(thread::spawn(move || {
                read_stream(new_name, new_chan, &mut buffered_stdout, true);
            }));
        }

        if let Some(output) = std_err {
            let mut buffered_stdout = BufReader::new(output);
            let new_name = command_name.clone();
            let new_chan = send_chan.clone();
            std_err_handle = Some(thread::spawn(move || {
                read_stream(new_name, new_chan, &mut buffered_stdout, false);
            }));
        }

        if let Some(handle) = std_out_handle {
            let _ = handle.join();
        }

        if let Some(handle) = std_err_handle {
            let _ = handle.join();
        }

        let exit_status = cmd_handle.wait();
        match exit_status {
            Ok(status) => {
                let _ = send_chan.send(OutputMessage {
                    name: command_name.clone(),
                    message: OutputMessagePayload::Done(status.code()),
                });
            }
            Err(e) => {
                let _ = send_chan.send(OutputMessage {
                    name: command_name.clone(),
                    message: OutputMessagePayload::Error(e),
                });
            }
        }
    })
}

fn read_stream<R>(
    cmd_name: String,
    send_chan: mpsc::Sender<OutputMessage>,
    reader: &mut R,
    is_stdout: bool,
) where
    R: BufRead,
{
    loop {
        let line = line_parse::get_line(reader);
        match line {
            Ok(Some(line_vec)) => {
                let _ = send_chan.send(OutputMessage {
                    name: cmd_name.clone(),
                    message: if is_stdout {
                        OutputMessagePayload::Stdout(line_vec.0, line_vec.1)
                    } else {
                        OutputMessagePayload::Stderr(line_vec.0, line_vec.1)
                    },
                });
            }
            Ok(None) => {
                return;
            }
            Err(e) => {
                let _ = send_chan.send(OutputMessage {
                    name: cmd_name.clone(),
                    message: OutputMessagePayload::Error(e),
                });
            }
        }
    }
}
