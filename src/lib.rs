//! Run commands in a concurrant manner
//! There are three main components to this API:
//! 1) Channel-based API: a basic API that passes all messages, errors, and statuses to channels that the caller can consume at their leisure.
//! See [ChannelCommand]
//! 1) Standard-out based API: an API that prints messages and errors to the console (standard out).
//! See [ConsoleCommand]
//! 1) Writer-based API: similar to the standard-out API but prints to an arbitrary writer (like a log file) instead.
//! See [WriterCommand]

mod channel_api;
mod color;
mod control;
mod kill_barrier;
mod line_parse;
mod run;
mod standard_out_api;
mod template;
mod which;
mod writer_api;

use std::collections::HashMap;
use std::io;
use std::io::Write;
use std::path::PathBuf;
use std::process;
use std::process::ExitStatus;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;

pub use channel_api::ChannelCommand;
pub use color::Color;
pub use control::HandleControl;
pub use control::Signal;
pub use line_parse::LineEnding;
pub use standard_out_api::parse_command_string;
pub use standard_out_api::ConsoleCommand;
pub use writer_api::WriterCommand;

pub const CURRENT_WORKING_DIRECTORY: Option<String> = None;

/// Error type describing any errors encountered while constructing the command
#[derive(Debug)]
pub enum CommandError {
    /// No command (empty string) provided
    EmptyCommand,
    /// The command couldn't not be found (executable not in the PATH).
    /// Returns the command that couldn't be found
    CommandNotFound(String),
    /// Couldn't parse the command line string (when the entire command is provided via [Command::from_string]).
    /// Returns the command line string that couldn't be parsed.
    ParseError(String),
}

/// Various options for running commands
#[derive(Clone)]
struct Options {
    /// Set what should happen when a command exits with a non-zero exit code.
    /// See [RestartOptions] for possible values and defaults
    restart: RestartOptions,
    /// Supresses console messages about commands starting (defaults to false).
    /// This is only applicable for the standard out API and the Writer API
    quiet: bool,
    /// Select whether or not to include file handle flags on the Writer and Standard Out API
    /// (o) denotes standard out.
    /// (e) denotes standard error.
    /// Defaults to false (no file handle flags).
    /// If false, all output is dumped to the console (or writer) without these o/e prefixes.
    file_handle_flags: bool,
}

/// An Internal class that isn't really meant to be used externally.
/// If you wish to create other variants of the API (other Command formats).
/// You will need to wrap an internal command and provide accessors to it. See [Command] for more info
#[derive(Clone)]
pub struct InnerCommand {
    name: String,
    command: String,
    args: Vec<String>,
    cur_dir: Option<PathBuf>,
    env: HashMap<String, String>,
}

impl From<InnerCommand> for process::Command {
    fn from(cmd: InnerCommand) -> Self {
        let mut command_process = process::Command::new(cmd.command);
        command_process.args(cmd.args);
        if cmd.cur_dir.is_some() {
            command_process.current_dir(cmd.cur_dir.unwrap());
        }
        command_process.envs(cmd.env);
        command_process.stdout(process::Stdio::piped());

        command_process
    }
}

/// Common trait expressing all the various operations you can do with a `Command`
/// Includes methods to parse commands and includes various options common to all Commands (Channel/Stdout/Writer) like setting a current directory
/// and setting env vars.
pub trait Command: Clone
where
    Self: Sized,
{
    /// Inserts an [InnerCommand] into the Command structure
    fn insert_command(cmd: InnerCommand) -> Self;

    /// Provide a references to the wrapper [InnerCommand] that was inserted via [insert_command](Command::insert_command)
    fn get_command(&self) -> &InnerCommand;

    /// Provide a mutable reference to the wrapped [InnerCommand] that was inserted via [insert_command](Command::insert_command)
    fn get_command_mut(&mut self) -> &mut InnerCommand;

    /// Construct a command from a command name (human readable command name), command executable, a list of arguments, and a directory to run the command in.
    /// None for `cur_dir` will default to the current working directory (or use [CURRENT_WORKING_DIRECTORY] constant).
    ///
    /// If the command cannot be constructed for various reasons, an `Err(CommandError)` is returned. See [CommandError] for more info on errors.
    /// ## Example
    /// ```
    /// use currant::ConsoleCommand;
    /// use currant::Command;
    /// use currant::CURRENT_WORKING_DIRECTORY;
    ///
    /// let cmd = ConsoleCommand::from_argv("test_cmd", "ls", ["la", "."], CURRENT_WORKING_DIRECTORY).unwrap();
    /// ```
    fn from_argv<S, C, D, ArgType, Cmds>(
        name: S,
        command: C,
        args: Cmds,
        cur_dir: Option<D>,
    ) -> Result<Self, CommandError>
    where
        S: Into<String>,
        C: Into<String>,
        D: Into<PathBuf>,
        ArgType: Into<String>,
        Cmds: IntoIterator<Item = ArgType>,
    {
        let name = name.into();
        let cmd = command.into();
        let dir = cur_dir.map(|d| d.into());
        check_command(&cmd, &dir)?;

        if name.is_empty() || cmd.is_empty() {
            return Err(CommandError::EmptyCommand);
        }
        let converted_args = args.into_iter().map(|s| s.into()).collect::<Vec<String>>();
        Ok(Self::insert_command(InnerCommand {
            name,
            command: cmd,
            args: converted_args,
            cur_dir: dir,
            env: HashMap::new(),
        }))
    }

    /// Construct a command from a command name (human readable command name), a full cli string, and a current working directory.
    /// For options on `cur_dir`, please see [Command::from_argv].
    ///
    /// The API will parse the cli string into the executable and arguments automatically.
    /// The API supports some features like quotes but not advanced features like pipes or logical operators.
    /// For those advanced features, you will need to format the command as a subshell (via `sh -c "..."`).
    /// If the command cannot be constructed for various reasons, an `Err(CommandError)` is returned. See [CommandError] for more info on errors.
    /// ## Example
    /// ```
    /// use currant::ConsoleCommand;
    /// use currant::Command;
    /// use currant::CURRENT_WORKING_DIRECTORY;
    ///
    /// let cmd = ConsoleCommand::from_string("test_cmd", "ls -la .", CURRENT_WORKING_DIRECTORY).unwrap();
    /// let cmd = ConsoleCommand::from_string("test_cmd", "echo \"hello, world\"", CURRENT_WORKING_DIRECTORY).unwrap();
    /// // BAD: doesn't actually pipe: let cmd = ConsoleCommand::from_string("test_cmd", "ls . | ls ..").unwrap();
    /// ```
    fn from_string<S, C, D>(
        name: S,
        command_string: C,
        cur_dir: Option<D>,
    ) -> Result<Self, CommandError>
    where
        S: Into<String>,
        C: Into<String>,
        D: Into<PathBuf>,
    {
        let (command, args) = parse_command_string(command_string)?;
        let dir = cur_dir.map(|d| d.into());
        check_command(&command, &dir)?;

        Ok(Self::insert_command(InnerCommand {
            name: name.into(),
            command,
            args,
            cur_dir: dir,
            env: HashMap::new(),
        }))
    }

    /// Sets environment variables for this command.
    /// ## Example
    /// ```
    /// use currant::ConsoleCommand;
    /// use currant::Command;
    /// use currant::CURRENT_WORKING_DIRECTORY;
    ///
    /// let mut cmd = ConsoleCommand::from_string("test_cmd", "ls -la .", CURRENT_WORKING_DIRECTORY).unwrap();
    /// cmd.env("key", "val");
    /// ```
    fn env<K, V>(&mut self, key: K, val: V) -> &mut Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        self.get_command_mut().env.insert(key.into(), val.into());
        self
    }
}

/// Represents output from a command
pub struct OutputMessage {
    /// The human readable name of the command for this message.
    /// Corresponds to the `name` parameter passed into [Command::from_argv] or [Command::from_string].
    pub name: String,
    /// The message payload. See [OutputMessagePayload] for more info
    pub message: OutputMessagePayload,
}

/// The payload of an output message
pub enum OutputMessagePayload {
    /// Command has started execution
    Start,
    /// Command has exited. Returns the exit status (if available) of the command
    Done(Option<i32>),
    /// A single line of standard out formatted as a byte vector. The line ending is included in the enum but not in the byte vector
    Stdout(line_parse::LineEnding, Vec<u8>),
    /// A single line of standard error formatted as a byte vector. The line ending is included in the enum but not in the byte vector
    Stderr(line_parse::LineEnding, Vec<u8>),
    /// An error has occurred with the command (usually a malformed command or I/O error). This doesn't include commands that fail via exit status.
    /// That is reported via [OutputMessagePayload::Done].
    Error(io::Error),
}

/// Exit status tuple. This string is the human-readable command name, the exit status is the exit
/// status code of the process if available
pub type ExitResult = (String, Option<ExitStatus>);

/// A handle so the caller can control various aspects of the running commands
pub struct CommandHandle {
    handle: thread::JoinHandle<Vec<ExitResult>>,
    channel: mpsc::Receiver<OutputMessage>,
    kill_trigger: kill_barrier::KillBarrier,
    pids: Vec<Arc<(String, Mutex<Option<u32>>)>>,
}

impl CommandHandle {
    /// Block the current thread and wait for all processes to exit.
    /// Returns a list of exit statuses from the child commands.
    /// If the currant overseer process panics, this function will Err with a string message.
    /// See [ExitResult] for info on this return type.
    pub fn join(self) -> Result<Vec<ExitResult>, String> {
        self.handle
            .join()
            .map_err(|_| "Thread panic'ed before exit".to_string())
    }

    /// returns a reference to the output channel (only in the channel based API).
    /// This channel will give the caller access to the output and status messages from the child commands.
    /// See [OutputMessage] for details on the channel payload.
    pub fn get_output_channel(&self) -> &mpsc::Receiver<OutputMessage> {
        &self.channel
    }

    /// kills all children processes without waiting for them to complete
    pub fn kill(&self) {
        let _ = self.kill_trigger.initiate_kill();
    }

    /// gets a handle to a [HandleControl] for the underlying threads.
    /// This allows you to kill and send signals to the underlying threads.
    /// See [HandleControl] for more details
    pub fn get_signaler(&self) -> control::HandleControl {
        HandleControl::new(self.pids.clone(), self.kill_trigger.clone())
    }
}

/// Iterates over the messages on the channel. Yields values of [OutputMessage]
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

/// provides a handle to the running children process for the Writer and Console API.
/// This differs from [CommandHandle] in that it doesn't provide any reference to the output channel since
/// that is managed internally by currant.
pub struct ControlledCommandHandle {
    supervisor: thread::JoinHandle<()>,
    handle: thread::JoinHandle<Vec<ExitResult>>,
    kill_trigger: kill_barrier::KillBarrier,
    pids: Vec<Arc<(String, Mutex<Option<u32>>)>>,
}

impl ControlledCommandHandle {
    /// Block the thread and wait until all processes have completed. See [CommandHandle::join] for more details.
    pub fn join(self) -> Result<Vec<ExitResult>, String> {
        self.supervisor
            .join()
            .map_err(|_| "thread panic'ed before exit".to_string())?;
        self.handle
            .join()
            .map_err(|_| "thread panic'ed before exit".to_string())
    }

    /// Kill all children processes without waiting for them to complete. See [CommandHandle::kill] for more details.
    pub fn kill(&self) {
        let _ = self.kill_trigger.initiate_kill();
    }

    /// gets a handle to a [HandleControl] for the underlying threads.
    /// This allows you to kill and send signals to the underlying threads.
    /// See [HandleControl] for more details
    pub fn get_signaler(&self) -> control::HandleControl {
        HandleControl::new(self.pids.clone(), self.kill_trigger.clone())
    }
}

/// An enum to tell currant what to do when a process exits with _nonzero_ (AKA failure) status
#[derive(Clone)]
pub enum RestartOptions {
    /// (DEFAULT): Let the failed process die (no-restart) and let all other processes continue as normal.
    Continue,
    /// Restart the failed process
    Restart,
    /// kill all children when any one process fails
    Kill,
}

/// A structure that represents a set of commands to run.
/// Essentially, this wraps a list of commands with some common options between them.
/// ## Example:
/// ```
/// use currant::{Command, ConsoleCommand, Runner, Color, CURRENT_WORKING_DIRECTORY};
///
/// let handle = Runner::new()
/// .command(
///     ConsoleCommand::from_string("test1", "ls -la .", CURRENT_WORKING_DIRECTORY)
///         .unwrap()
///         .color(Color::BLUE),
/// )
/// .command(
///     ConsoleCommand::from_string("test2", "ls -la ..", CURRENT_WORKING_DIRECTORY)
///         .unwrap()
///         .color(Color::RED),
/// )
/// .command(
///     ConsoleCommand::from_string("test3", "ls -la ../..", CURRENT_WORKING_DIRECTORY)
///         .unwrap()
///         .color(Color::GREEN),
/// )
/// .execute();
/// handle.join().unwrap();
/// ```
pub struct Runner<C: Command> {
    commands: Vec<C>,
    restart: RestartOptions,
    quiet: bool,
    file_handle_flags: bool,
    start_message_template: String,
    done_message_template: String,
    payload_message_template: String,
    error_message_template: String,
}

impl<C: Command> Default for Runner<C> {
    fn default() -> Self {
        Runner::new()
    }
}

impl<C: Command> Runner<C> {
    /// Instantiate a new runner with no commands and default options
    pub fn new() -> Self {
        Runner {
            commands: Vec::new(),
            restart: RestartOptions::Continue,
            quiet: false,
            file_handle_flags: false,
            start_message_template: "{{begin_color}}SYSTEM: starting process {{name}}{{reset_color}}"
                .to_string(),
            done_message_template:
                "{{begin_color}}{{name}}:{{reset_color}} process exited with status: {{status_code}}"
                    .to_string(),
            payload_message_template: "{{begin_color}}{{name}}{{handle_flag}}:{{reset_color}}".to_string(),
            error_message_template: "{{begin_color}}SYSTEM (e): Encountered error with process {{name}}: {{error_message}}{{reset_color}}".to_string(),
        }
    }

    /// Add a new command.
    /// All commands must be from the same API type (e.g. Console, Writer, or Console). No mixing and matching API types.
    pub fn command<T: AsRef<C>>(&mut self, cmd: T) -> &mut Self {
        self.commands.push(cmd.as_ref().clone());
        self
    }

    /// Set the restart behavior. The default is [RestartOptions::Continue].
    /// See [RestartOptions] for more info.
    pub fn restart(&mut self, restart_opt: RestartOptions) -> &mut Self {
        self.restart = restart_opt;
        self
    }

    /// Set the verbosity level of the output. For Writer and Console API, setting `quiet = true` will suppress housekeeping messages
    /// like start and stop messages and only display standard out/standard error output.
    /// The default is `false`.
    pub fn quiet(&mut self, quiet_opt: bool) -> &mut Self {
        self.quiet = quiet_opt;
        self
    }

    /// Set the verbosity on the file handle indicators.
    /// Normally, for the ConsoleApi and WriterApi, currant will display a `(o)` prefix for standard out
    /// and display a `(e)` prefix for standard error.
    /// If `file_handle_flag_opt` is `false` these indicators will be suppressed.
    /// This will also suppress output from the `{{handle_flag}}` interpolation in [Runner::payload_message_template]
    /// Default is `false`.
    pub fn should_show_file_handle(&mut self, file_handle_flag_opt: bool) -> &mut Self {
        self.file_handle_flags = file_handle_flag_opt;
        self
    }

    /// Set the start message template. This only affects the console and writer APIs.
    /// Default is:
    ///
    /// `{{begin_color} SYSTEM: starting process {{name}}{{reset_color}}`
    ///
    /// Possible interpolations are:
    /// * `{{name}}`: the name of the process
    /// * `{{begin_color}}`: color the following text with the color of the command (ignored for APIs without color)
    /// * `{{reset_color}}`: resets the color back to terminal default
    pub fn start_message_template<S: Into<String>>(&mut self, template: S) -> &mut Self {
        self.start_message_template = template.into();
        self
    }

    /// Set the message template for when the command exits.
    /// The same interpolations for [Runner::start_message_template] apply here.
    /// Default is:
    ///
    /// `{{begin_color}}{{name}}:{{reset_color}} process exited with status: {{status_code}}`
    ///
    /// Additional interpolations:
    /// * `{{status_code}}`: the exit status of the command. Changes to `(none)` when there is no exit status.
    pub fn done_message_template<S: Into<String>>(&mut self, template: S) -> &mut Self {
        self.done_message_template = template.into();
        self
    }

    /// Set the message template for when the command has payload to display.
    /// The same interpolations for [Runner::start_message_template] apply here.
    /// The actual payload message is displayed directly after the template is executed and printed.
    /// Default is:
    ///
    /// `{{begin_color}}{{name}}{{handle_flag}}:{{reset_color}}`
    ///
    /// Additional interpolations:
    /// * `{{handle_flag}}`: the handle flag for which file handle the payload is on. `(o)` for stdout and `(e)` for standard error.
    /// If [Runner::should_show_file_handle] is set to false (default), these interpolations will be set to empty string.
    pub fn payload_message_template<S: Into<String>>(&mut self, template: S) -> &mut Self {
        self.payload_message_template = template.into();
        self
    }

    /// Set the message template for when the command fails.
    /// The same interpolations for [Runner::start_message_template] apply here.
    /// Default is:
    ///
    /// `{{begin_color}}SYSTEM (e): Encountered error with process {{name}}: {{error_message}}{{reset_color}}`
    ///
    /// Additional interpolations:
    /// * `{{error_message}}`: the error message of what went wrong.
    pub fn error_message_template<S: Into<String>>(&mut self, template: S) -> &mut Self {
        self.error_message_template = template.into();
        self
    }

    fn to_options(&self) -> Options {
        Options {
            restart: self.restart.clone(),
            quiet: self.quiet,
            file_handle_flags: self.file_handle_flags,
        }
    }

    fn get_template_strings(&self) -> template::TemplateStrings {
        template::TemplateStrings {
            start_message_template: self.start_message_template.clone(),
            done_message_template: self.done_message_template.clone(),
            payload_message_template: self.payload_message_template.clone(),
            error_message_template: self.error_message_template.clone(),
        }
    }
}

impl Runner<ChannelCommand> {
    /// Execute the commands using the Channel API. The `Runner` must be constructed with `ChannelCommand`s.
    pub fn execute(&mut self) -> CommandHandle {
        run_commands(self)
    }
}

impl Runner<WriterCommand> {
    /// Execute the commands using the Writer API. The writer must be provided here. The `Runner` must be constructed with `WriterCommand`s
    pub fn execute<W: Write + Send + 'static>(&mut self, writer: W) -> ControlledCommandHandle {
        writer_api::run_commands_writer(self, writer)
    }
}

impl Runner<ConsoleCommand> {
    /// Execute the commands using the Console API. The `Runner` must be constructed with `ConsoleCommand`s.
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

fn check_command(exec_name: &str, dir: &Option<PathBuf>) -> Result<(), CommandError> {
    if which::exec_exists(exec_name, dir) {
        Ok(())
    } else {
        Err(CommandError::CommandNotFound(exec_name.to_string()))
    }
}

#[cfg(test)]
mod test {
    use crate::Command;

    #[test]
    fn command_not_found() {
        let cmd = super::ConsoleCommand::from_string(
            "test",
            "bogus_cmd_not_found",
            super::CURRENT_WORKING_DIRECTORY,
        );

        match cmd {
            Err(super::CommandError::CommandNotFound(name)) => {
                assert_eq!(&name, "bogus_cmd_not_found",)
            }
            _ => panic!("bogus command didn't return CommandNotFound"),
        }
    }

    #[test]
    fn command_empty() {
        let cmd = super::ConsoleCommand::from_string("test", "", super::CURRENT_WORKING_DIRECTORY);

        match cmd {
            Err(super::CommandError::EmptyCommand) => {}
            _ => panic!("empty command didn't error out"),
        }
    }
}
