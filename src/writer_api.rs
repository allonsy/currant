use super::Command;
use super::ControlledCommandHandle;
use super::InnerCommand;
use super::OutputMessagePayload;
use super::Runner;
use std::io::Write;
use std::sync::mpsc;
use std::thread;

/// Represents a command that prints output to a given Writer.
/// All messages for all commands will be printed to the same writer. If you want different writers for different commands, you will
/// need to manually pipe the output via the channel API. See [ChannelCommand](crate::ChannelCommand) for more info.
/// In order to instantiate the `Runner` with the correct writer, see: [Runner::execute](struct.Runner.html#impl-2)
/// ## Example:
/// ```
/// use currant::Command;
/// use currant::Runner;
/// use currant::WriterCommand;
/// use fs::File;
/// use std::fs;
///
/// fn main() {
///     let log_file_name = "test_log.txt";
///     let log_file = File::create(log_file_name).unwrap();
///
///     run_cmds(log_file);
///
///     let log_file_contents = std::fs::read(log_file_name).unwrap();
///
///     println!("log file contents: ");
///     println!("{}", String::from_utf8_lossy(&log_file_contents));
///
///     fs::remove_file(log_file_name).unwrap();
/// }
///
/// fn run_cmds(file: File) {
///     // all commands output to the same writer
///     let handle = Runner::new()
///         .command(WriterCommand::from_string("test1", "ls -la .").unwrap())
///         .command(WriterCommand::from_string("test2", "ls -la ..").unwrap())
///         .command(WriterCommand::from_string("test3", "ls -la ../..").unwrap())
///         .execute(file);
///
///     handle.join().unwrap();
/// }
/// ```
#[derive(Clone)]
pub struct WriterCommand {
    inner_command: InnerCommand,
}

impl Command for WriterCommand {
    fn insert_command(cmd: InnerCommand) -> Self {
        WriterCommand { inner_command: cmd }
    }

    fn get_command(&self) -> &InnerCommand {
        &self.inner_command
    }

    fn get_command_mut(&mut self) -> &mut InnerCommand {
        &mut self.inner_command
    }
}

impl AsRef<WriterCommand> for WriterCommand {
    fn as_ref(&self) -> &WriterCommand {
        self
    }
}

pub fn run_commands_writer<W>(runner: &Runner<WriterCommand>, writer: W) -> ControlledCommandHandle
where
    W: Write + Send + 'static,
{
    let handle = super::run_commands(runner);

    let recv = handle.channel;

    thread::spawn(move || {
        process_channel(&recv, writer);
    });
    ControlledCommandHandle {
        handle: handle.handle,
        kill_trigger: handle.kill_trigger,
    }
}

fn process_channel<W>(chan: &mpsc::Receiver<super::OutputMessage>, mut writer: W)
where
    W: Write + Send,
{
    loop {
        let message = chan.recv();
        if message.is_err() {
            return;
        }

        let message = message.unwrap();
        let _ = match message.message {
            OutputMessagePayload::Start => {
                writer.write_all(format!("SYSTEM: starting process: {}\n", message.name).as_bytes())
            }
            OutputMessagePayload::Done(Some(exit_status)) => writer.write_all(
                format!(
                    "{}: process exited with status: {}\n",
                    message.name, exit_status
                )
                .as_bytes(),
            ),
            OutputMessagePayload::Done(None) => writer.write_all(
                format!("{}: process exited without exit status\n", message.name).as_bytes(),
            ),
            OutputMessagePayload::Stdout(_, mut bytes) => {
                let mut prefix = format!("{} (o): ", message.name).into_bytes();
                prefix.append(&mut bytes);
                prefix.push(b'\n');
                writer.write_all(&prefix)
            }
            OutputMessagePayload::Stderr(_, mut bytes) => {
                let mut prefix = format!("{} (e): ", message.name).into_bytes();
                prefix.append(&mut bytes);
                prefix.push(b'\n');
                writer.write_all(&prefix)
            }
            OutputMessagePayload::Error(e) => writer.write_all(
                format!(
                    "SYSTEM (e): Encountered error with process {}: {}\n",
                    message.name, e
                )
                .as_bytes(),
            ),
        };
    }
}
