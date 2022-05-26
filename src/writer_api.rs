use super::template;
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
/// use currant::CURRENT_WORKING_DIRECTORY;
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
///         .command(WriterCommand::from_string("test1", "ls -la .", CURRENT_WORKING_DIRECTORY).unwrap())
///         .command(WriterCommand::from_string("test2", "ls -la ..", CURRENT_WORKING_DIRECTORY).unwrap())
///         .command(WriterCommand::from_string("test3", "ls -la ../..", CURRENT_WORKING_DIRECTORY).unwrap())
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

    let template_strings = runner.get_template_strings();

    let supervisor = thread::spawn(move || {
        process_channel(&recv, template_strings, writer);
    });
    ControlledCommandHandle {
        supervisor,
        handle: handle.handle,
        kill_trigger: handle.kill_trigger,
        pids: handle.pids,
    }
}

fn process_channel<W>(
    chan: &mpsc::Receiver<super::OutputMessage>,
    template_strings: template::TemplateStrings,
    mut writer: W,
) where
    W: Write + Send,
{
    loop {
        let message = chan.recv();
        if message.is_err() {
            return;
        }

        let message = message.unwrap();

        let mut template = template::Template::new(None);
        template.name = message.name;

        let _ = match message.message {
            OutputMessagePayload::Start => writer.write_all(
                format!(
                    "{}\n",
                    template.execute(&template_strings.start_message_template)
                )
                .as_bytes(),
            ),
            OutputMessagePayload::Done(exit_status) => {
                template.status_code = exit_status;
                writer.write_all(
                    format!(
                        "{}\n",
                        template.execute(&template_strings.done_message_template)
                    )
                    .as_bytes(),
                )
            }
            OutputMessagePayload::Stdout(_, mut bytes) => {
                template.handle_flag = " (o)".to_string();
                let mut prefix = template
                    .execute(&template_strings.payload_message_template)
                    .into_bytes();
                prefix.append(&mut bytes);
                prefix.push(b'\n');
                writer.write_all(&prefix)
            }
            OutputMessagePayload::Stderr(_, mut bytes) => {
                template.handle_flag = " (e)".to_string();
                let mut prefix = template
                    .execute(&template_strings.payload_message_template)
                    .into_bytes();
                prefix.append(&mut bytes);
                prefix.push(b'\n');
                writer.write_all(&prefix)
            }
            OutputMessagePayload::Error(e) => {
                template.error_message = e.to_string();
                writer.write_all(
                    format!(
                        "{}\n",
                        template.execute(&template_strings.error_message_template)
                    )
                    .as_bytes(),
                )
            }
        };
    }
}
