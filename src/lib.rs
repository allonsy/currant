mod color;
mod kill_barrier;
mod line_parse;
mod standard_out_api;
mod writer_api;

use std::collections::HashMap;
use std::io
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::path::PathBuf;
use std::process;
use std::process::ExitStatus;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;

pub use color::Color;
pub use standard_out_api::parse_command_string;
pub use standard_out_api::ConsoleCommand;
pub use writer_api::WriterCommand;

#[derive(Debug)]
pub enum CommandError {
    EmptyCommand,
    ParseError(String),
}

#[derive(Clone)]
pub struct ChannelCommand {
    inner_command: InnerCommand,
}

impl Command for ChannelCommand {
    fn insert_command(cmd: InnerCommand) -> Self {
        ChannelCommand { inner_command: cmd }
    }

    fn get_command(&self) -> &InnerCommand {
        &self.inner_command
    }

    fn get_command_mut(&mut self) -> &mut InnerCommand {
        &mut self.inner_command
    }
}

#[derive(Clone)]
pub struct InnerCommand {
    name: String,
    command: String,
    args: Vec<String>,
    cur_dir: Option<PathBuf>,
    env: HashMap<String, String>,
}

pub trait Command
where
    Self: Sized,
{
    fn insert_command(cmd: InnerCommand) -> Self;

    fn get_command(&self) -> &InnerCommand;

    fn get_command_mut(&mut self) -> &mut InnerCommand;

    fn from_argv<S, C, ArgType, Cmds>(name: S, command: C, args: Cmds) -> Result<Self, CommandError>
    where
        S: Into<String>,
        C: Into<String>,
        ArgType: Into<String>,
        Cmds: IntoIterator<Item = ArgType>,
    {
        let name = name.into();
        if name.is_empty() {
            return Err(CommandError::EmptyCommand);
        }
        let converted_args = args.into_iter().map(|s| s.into()).collect::<Vec<String>>();
        Ok(Self::insert_command(InnerCommand {
            name,
            command: command.into(),
            args: converted_args,
            cur_dir: None,
            env: HashMap::new(),
        }))
    }

    fn from_string<S, C>(name: S, command_string: C) -> Result<Self, CommandError>
    where
        S: Into<String>,
        C: Into<String>,
    {
        let (command, args) = parse_command_string(command_string)?;
        Ok(Self::insert_command(InnerCommand {
            name: name.into(),
            command,
            args,
            cur_dir: None,
            env: HashMap::new(),
        }))
    }

    fn cur_dir<D>(mut self, dir: D) -> Self
    where
        D: Into<PathBuf>,
    {
        self.get_command_mut().cur_dir = Some(dir.into());
        self
    }

    fn env<K, V>(mut self, key: K, val: V) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        self.get_command_mut().env.insert(key.into(), val.into());
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

impl Iterator for CommandHandle {
    type Item = OutputMessage;

    fn next(&mut self) -> Option<Self::Item> {
        self.channel.recv().ok()
    }
}

impl Iterator for &CommandHandle {
    type Item = OutputMessage;

    fn next(&mut self) -> Option<OutputMessage> {
        self.channel.recv().ok()
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
struct Options {
    restart: RestartOptions,
    quiet: bool,
    file_handle_flags: bool,
}

pub struct Runner<C: Command> {
    commands: Vec<C>,
    restart: RestartOptions,
    quiet: bool,
    file_handle_flags: bool,
}

impl<C: Command> Default for Runner<C> {
    fn default() -> Self {
        Runner::new()
    }
}

impl<CL: Command> Runner<CL> {
    pub fn new() -> Self {
        Runner {
            commands: Vec::new(),
            restart: RestartOptions::Continue,
            quiet: false,
            file_handle_flags: false,
        }
    }

    pub fn command(&mut self, cmd: CL) -> &mut Self {
        self.commands.push(cmd);
        self
    }

    pub fn restart(&mut self, restart_opt: RestartOptions) -> &mut Self {
        self.restart = restart_opt;
        self
    }

    pub fn quiet(&mut self, quiet_opt: bool) -> &mut Self {
        self.quiet = quiet_opt;
        self
    }

    pub fn should_show_file_handle(&mut self, file_handle_flag_opt: bool) -> &mut Self {
        self.file_handle_flags = file_handle_flag_opt;
        self
    }

    fn to_options(&self) -> Options {
        Options {
            restart: self.restart.clone(),
            quiet: self.quiet,
            file_handle_flags: self.file_handle_flags,
        }
    }
}

impl Runner<ChannelCommand> {
    pub fn execute(&mut self) -> CommandHandle {
        run_commands(self)
    }
}

impl Runner<WriterCommand> {
    pub fn execute<W: Write + Send + 'static>(&mut self, writer: W) -> ControlledCommandHandle {
        writer_api::run_commands_writer(self, writer)
    }
}

impl Runner<ConsoleCommand> {
    pub fn execute(&mut self) -> ControlledCommandHandle {
        standard_out_api::run_commands_stdout(self)
    }
}

fn run_commands<C: Command>(runner: &Runner<C>) -> CommandHandle {
    let actual_cmds = runner
        .commands
        .iter()
        .map(|c| c.get_command().clone())
        .collect::<Vec<InnerCommand>>();
    run_commands_internal(actual_cmds, runner.to_options())
}

fn run_commands_internal(commands: Vec<InnerCommand>, options: Options) -> CommandHandle {
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

fn run_command(
    command: InnerCommand,
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
        thread::spawn(move || kill_thread(&kill_trigger_clone, child_clone));

        if let Some(output) = std_out {
            let mut buffered_stdout = BufReader::new(output);
            let new_name = command_name.clone();
            let new_chan = send_chan.clone();
            std_out_handle = Some(thread::spawn(move || {
                read_stream(&new_name, new_chan, &mut buffered_stdout, true);
            }));
        }

        if let Some(output) = std_err {
            let mut buffered_stdout = BufReader::new(output);
            let new_name = command_name.clone();
            let new_chan = send_chan.clone();
            std_err_handle = Some(thread::spawn(move || {
                read_stream(&new_name, new_chan, &mut buffered_stdout, false);
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

fn kill_thread(kill_trigger: &kill_barrier::KillBarrier, child: Arc<Mutex<process::Child>>) {
    let _ = kill_trigger.wait();

    let lock_res = child.lock();
    if let Ok(mut locked_child) = lock_res {
        let _ = locked_child.kill();
    }
}

fn read_stream<R>(
    cmd_name: &str,
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
                    name: cmd_name.to_string(),
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
                    name: cmd_name.to_string(),
                    message: OutputMessagePayload::Error(e),
                });
            }
        }
    }
}
