mod color;
mod kill_barrier;
mod line_parse;
mod standard_out_api;
mod writer_api;

use std::collections::HashMap;
use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::path::Path;
use std::path::PathBuf;
use std::process;
use std::process::ExitStatus;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;

pub use color::Color;
pub use standard_out_api::parse_command_string;
pub use standard_out_api::run_commands_stdout;
pub use standard_out_api::run_commands_stdout_with_options;
pub use standard_out_api::ConsoleCommand;

pub use writer_api::run_commands_writer;
pub use writer_api::run_commands_writer_with_options;

#[derive(Debug)]
pub enum CommandError {
    EmptyCommand,
    ParseError(String),
}

pub struct Command {
    name: String,
    command: String,
    args: Vec<String>,
    cur_dir: Option<PathBuf>,
    env: HashMap<String, String>,
}

impl Command {
    pub fn new<S, C, ArgType, Cmds>(
        name: S,
        command: C,
        args: Cmds,
    ) -> Result<Command, CommandError>
    where
        S: AsRef<str>,
        C: AsRef<str>,
        ArgType: AsRef<str>,
        Cmds: IntoIterator<Item = ArgType>,
    {
        if name.as_ref().is_empty() {
            return Err(CommandError::EmptyCommand);
        }
        let converted_args = args
            .into_iter()
            .map(|s| s.as_ref().to_string())
            .collect::<Vec<String>>();
        Ok(Command {
            name: name.as_ref().to_string(),
            command: command.as_ref().to_string(),
            args: converted_args,
            cur_dir: None,
            env: HashMap::new(),
        })
    }

    pub fn full_cmd<S, C>(name: S, command_string: C) -> Result<Command, CommandError>
    where
        S: AsRef<str>,
        C: AsRf<str>,
    {
        let (command, args) = parse_command_string(command_string)?;
        Ok(Command {
            name: name.as_ref().to_string(),
            command,
            args,
            cur_dir: None,
            env: HashMap::new(),
        })
    }

    pub fn cur_dir<D>(mut self, dir: D) -> Self
    where
        D: AsRef<Path>,
    {
        self.cur_dir = Some(dir.as_ref().to_path_buf());
        self
    }

    pub fn env<K, V>(mut self, key: K, val: V) -> Self
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        self.env
            .insert(key.as_ref().to_string(), val.as_ref().to_string());
        self
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
    handle: thread::JoinHandle<Vec<Option<ExitStatus>>>,
    channel: mpsc::Receiver<OutputMessage>,
    kill_trigger: kill_barrier::KillBarrier,
}

impl CommandHandle {
    pub fn join(self) -> Result<Vec<Option<ExitStatus>>, String> {
        self.handle
            .join()
            .map_err(|_| "Thread panic'ed before exit".to_string())
    }

    pub fn get_output_channel(&self) -> &mpsc::Receiver<OutputMessage> {
        &self.channel
    }

    pub fn kill(&self) {
        let _ = self.kill_trigger.initiate_kill();
    }
}

pub struct ControlledCommandHandle {
    handle: thread::JoinHandle<Vec<Option<ExitStatus>>>,
    kill_trigger: kill_barrier::KillBarrier,
}

impl ControlledCommandHandle {
    pub fn join(self) -> Result<Vec<Option<ExitStatus>>, String> {
        self.handle
            .join()
            .map_err(|_| "thread panic'ed before exit".to_string())
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
    verbose: bool,
    file_handle_flags: bool,
}

impl Options {
    pub fn new() -> Options {
        Options {
            restart: RestartOptions::Continue,
            verbose: true,
            file_handle_flags: true,
        }
    }

    pub fn verbose(&mut self, verbose: bool) {
        self.verbose = verbose;
    }

    pub fn restart(&mut self, restart: RestartOptions) {
        self.restart = restart;
    }

    pub fn file_handle_flags(&mut self, file_handle_flags: bool) {
        self.file_handle_flags = file_handle_flags;
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
        let mut statuses = Vec::new();
        for cmd in commands {
            handles.push(run_command(
                cmd,
                send.clone(),
                options.clone(),
                kill_trigger_clone.clone(),
            ));
        }

        for handle in handles {
            statuses.push(handle.join().unwrap_or(None));
        }

        statuses
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
) -> thread::JoinHandle<Option<ExitStatus>> {
    thread::spawn(move || loop {
        let mut command_process = process::Command::new(&command.command);
        command_process.args(&command.args);
        if command.cur_dir.is_some() {
            command_process.current_dir(command.cur_dir.clone().unwrap());
        }
        command_process.envs(command.env.clone());
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
                        return Some(status);
                    }
                    RestartOptions::Restart => {
                        if status.success() {
                            return Some(status);
                        }
                    }
                    RestartOptions::Kill => {
                        if !status.success() {
                            let _ = kill_trigger.initiate_kill();
                        }
                        return Some(status);
                    }
                };
            }
            Err(e) => {
                let _ = send_chan.send(OutputMessage {
                    name: command_name,
                    message: OutputMessagePayload::Error(e),
                });
                return None;
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
