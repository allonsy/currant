mod kill_barrier;
mod line_parse;
mod standard_out_api;
mod writer_api;

use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::process;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;

pub use standard_out_api::parse_command_string;
pub use standard_out_api::run_commands_stdout;
pub use standard_out_api::run_commands_stdout_with_options;
pub use standard_out_api::Color;
pub use standard_out_api::StandardOutCommand;

pub use writer_api::run_commands_writer;
pub use writer_api::run_commands_writer_with_options;

pub struct Command {
    name: String,
    command: String,
    args: Vec<String>,
}

impl Command {
    pub fn new<S, C, ArgType, Cmds>(name: S, command: C, args: Cmds) -> Command
    where
        S: AsRef<str>,
        C: AsRef<str>,
        ArgType: AsRef<str>,
        Cmds: IntoIterator<Item = ArgType>,
    {
        let converted_args = args
            .into_iter()
            .map(|s| s.as_ref().to_string())
            .collect::<Vec<String>>();
        Command {
            name: name.as_ref().to_string(),
            command: command.as_ref().to_string(),
            args: converted_args,
        }
    }

    pub fn new_command_string<S, C>(name: S, command_string: C) -> Command
    where
        S: AsRef<str>,
        C: AsRef<str>,
    {
        let (command, args) = parse_command_string(command_string);
        Command {
            name: name.as_ref().to_string(),
            command,
            args,
        }
    }
}

pub struct OutputMessage {
    pub name: String,
    pub message: OutputMessagePayload,
}

pub enum OutputMessagePayload {
    Start,
    Done(Option<i32>),
    Stdout(line_parse::LineEnding, Vec<u8>),
    Stderr(line_parse::LineEnding, Vec<u8>),
    Error(io::Error),
}

pub struct CommandHandle {
    handle: thread::JoinHandle<()>,
    channel: mpsc::Receiver<OutputMessage>,
    kill_trigger: kill_barrier::KillBarrier,
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

    pub fn kill(&self) {
        let _ = self.kill_trigger.initiate_kill();
    }
}

pub struct ControlledCommandHandle {
    handle: thread::JoinHandle<()>,
    kill_trigger: kill_barrier::KillBarrier,
}

impl ControlledCommandHandle {
    pub fn join(self) {
        self.handle
            .join()
            .unwrap_or_else(|_| panic!("Unable to join on handle"));
    }

    pub fn kill(&self) {
        let _ = self.kill_trigger.initiate_kill();
    }
}

#[derive(Clone)]
pub enum RestartOptions {
    Continue,
    Restart,
    Kill,
}

#[derive(Clone)]
pub struct Options {
    restart: RestartOptions,
}

impl Options {
    pub fn new() -> Options {
        Options {
            restart: RestartOptions::Continue,
        }
    }

    pub fn restart(&mut self, restart: RestartOptions) {
        self.restart = restart;
    }
}

impl Default for Options {
    fn default() -> Self {
        Self::new()
    }
}

pub fn run_commands<Cmds>(commands: Cmds, options: Options) -> CommandHandle
where
    Cmds: IntoIterator<Item = Command>,
{
    let actual_cmds = commands.into_iter().collect::<Vec<Command>>();
    run_commands_internal(actual_cmds, options)
}

fn run_commands_internal(commands: Vec<Command>, options: Options) -> CommandHandle {
    let (send, recv) = mpsc::channel();
    let kill_trigger = kill_barrier::KillBarrier::new();
    let kill_trigger_clone = kill_trigger.clone();

    let handle = thread::spawn(move || {
        let mut handles = Vec::new();
        for cmd in commands {
            handles.push(run_command(
                cmd,
                send.clone(),
                options.clone(),
                kill_trigger_clone.clone(),
            ));
        }

        for handle in handles {
            let _ = handle.join();
        }
    });

    CommandHandle {
        handle,
        channel: recv,
        kill_trigger,
    }
}

pub fn run_command(
    command: Command,
    send_chan: mpsc::Sender<OutputMessage>,
    options: Options,
    kill_trigger: kill_barrier::KillBarrier,
) -> thread::JoinHandle<()> {
    thread::spawn(move || loop {
        let mut command_process = process::Command::new(&command.command);
        command_process.args(&command.args);
        command_process.stdout(process::Stdio::piped());
        let command_name = command.name.clone();

        let _ = send_chan.send(OutputMessage {
            name: command_name.clone(),
            message: OutputMessagePayload::Start,
        });

        let mut cmd_handle = command_process
            .spawn()
            .unwrap_or_else(|_| panic!("Unable to spawn process: {}", command.command.clone()));
        let std_out = cmd_handle.stdout.take();
        let std_err = cmd_handle.stderr.take();
        let mut std_out_handle = None;
        let mut std_err_handle = None;

        let shared_handle = Arc::new(Mutex::new(cmd_handle));

        let child_clone = shared_handle.clone();
        let kill_trigger_clone = kill_trigger.clone();
        thread::spawn(move || kill_thread(kill_trigger_clone, child_clone));

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

        let exit_status = shared_handle.lock().unwrap().wait();
        match exit_status {
            Ok(status) => {
                let _ = send_chan.send(OutputMessage {
                    name: command_name.clone(),
                    message: OutputMessagePayload::Done(status.code()),
                });

                match options.restart {
                    RestartOptions::Continue => {
                        break;
                    }
                    RestartOptions::Restart => {
                        if status.success() {
                            break;
                        }
                    }
                    RestartOptions::Kill => {
                        if !status.success() {
                            let _ = kill_trigger.initiate_kill();
                        }
                        break;
                    }
                };
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

fn kill_thread(kill_trigger: kill_barrier::KillBarrier, child: Arc<Mutex<process::Child>>) {
    let _ = kill_trigger.wait();

    let lock_res = child.lock();
    if let Ok(mut locked_child) = lock_res {
        let _ = locked_child.kill();
    }
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