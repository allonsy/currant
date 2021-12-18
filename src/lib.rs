mod color;
mod kill_barrier;
mod line_parse;
mod run;
mod standard_out_api;
mod writer_api;

use std::collections::HashMap;
use std::io;
use std::io::Write;
use std::path::PathBuf;
use std::process;
use std::process::ExitStatus;
use std::sync::mpsc;
use std::thread;

pub use color::Color;
pub use standard_out_api::parse_command_string;
pub use standard_out_api::ConsoleCommand;
pub use writer_api::WriterCommand;

#[derive(Debug)]
pub enum CommandError {
    EmptyCommand,
    CommandNotFound(String),
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

impl AsRef<ChannelCommand> for ChannelCommand {
    fn as_ref(&self) -> &ChannelCommand {
        self
    }
}

#[derive(Clone)]
struct Options {
    restart: RestartOptions,
    quiet: bool,
    file_handle_flags: bool,
}

#[derive(Clone)]
pub struct InnerCommand {
    name: String,
    command: String,
    args: Vec<String>,
    cur_dir: Option<PathBuf>,
    env: HashMap<String, String>,
}

impl InnerCommand {
    fn to_stdlib_command(&self) -> process::Command {
        let mut command_process = process::Command::new(&self.command);
        command_process.args(&self.args);
        if self.cur_dir.is_some() {
            command_process.current_dir(self.cur_dir.clone().unwrap());
        }
        command_process.envs(self.env.clone());
        command_process.stdout(process::Stdio::piped());

        command_process
    }
}

pub trait Command: Clone
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
        let cmd = command.into();
        check_command(&cmd)?;

        if name.is_empty() || cmd.is_empty() {
            return Err(CommandError::EmptyCommand);
        }
        let converted_args = args.into_iter().map(|s| s.into()).collect::<Vec<String>>();
        Ok(Self::insert_command(InnerCommand {
            name,
            command: cmd,
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
        check_command(&command)?;

        Ok(Self::insert_command(InnerCommand {
            name: name.into(),
            command,
            args,
            cur_dir: None,
            env: HashMap::new(),
        }))
    }

    fn cur_dir<D>(&mut self, dir: D) -> &mut Self
    where
        D: Into<PathBuf>,
    {
        self.get_command_mut().cur_dir = Some(dir.into());
        self
    }

    fn env<K, V>(&mut self, key: K, val: V) -> &mut Self
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

pub type ExitResult = (String, Option<ExitStatus>);

pub struct CommandHandle {
    handle: thread::JoinHandle<Vec<ExitResult>>,
    channel: mpsc::Receiver<OutputMessage>,
    kill_trigger: kill_barrier::KillBarrier,
}

impl CommandHandle {
    pub fn join(self) -> Result<Vec<ExitResult>, String> {
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
    handle: thread::JoinHandle<Vec<ExitResult>>,
    kill_trigger: kill_barrier::KillBarrier,
}

impl ControlledCommandHandle {
    pub fn join(self) -> Result<Vec<ExitResult>, String> {
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

impl<C: Command> Runner<C> {
    pub fn new() -> Self {
        Runner {
            commands: Vec::new(),
            restart: RestartOptions::Continue,
            quiet: false,
            file_handle_flags: false,
        }
    }

    pub fn command<T: AsRef<C>>(&mut self, cmd: T) -> &mut Self {
        self.commands.push(cmd.as_ref().clone());
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
        .collect();
    run::run_commands_internal(actual_cmds, runner.to_options())
}

fn check_command(exec_name: &str) -> Result<(), CommandError> {
    which::which(exec_name).map_err(|_| CommandError::CommandNotFound(exec_name.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod test {
    use crate::Command;

    #[test]
    fn command_not_found() {
        let cmd = super::ConsoleCommand::from_string("test", "bogus_cmd_not_found");

        match cmd {
            Err(super::CommandError::CommandNotFound(name)) => {
                assert_eq!(
                    &name, "bogus_cmd_not_found",
                    "Command Not Found Error has wrong command name"
                )
            }
            _ => panic!("bogus command didn't return CommandNotFound"),
        }
    }
}
