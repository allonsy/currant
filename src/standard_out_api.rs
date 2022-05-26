use crate::template::TemplateStrings;

use super::color;
use super::color::Color;
use super::template;
use super::Command;
use super::CommandError;
use super::ControlledCommandHandle;
use super::InnerCommand;
use super::OutputMessagePayload;
use super::Runner;
use std::collections::HashMap;
use std::io::Write;
use std::sync::mpsc;
use std::thread;

/// Represents a command that prints all messages to the console.
/// You can set the color of the command via the [ConsoleCommand::color] function. The default is a random color ([Color::Random]).
/// In this case, currant will choose random but distinct colors so that all commands are as visually distant as possible.
/// ## Example:
/// ```
/// use currant::{Color, Command, ConsoleCommand, Runner, CURRENT_WORKING_DIRECTORY};
///
/// let handle = Runner::new()
///     .command(
///         ConsoleCommand::from_string("test1", "ls -la .", CURRENT_WORKING_DIRECTORY)
///             .unwrap()
///             .color(Color::BLUE),
///     )
///     .command(
///         ConsoleCommand::from_string("test2", "ls -la ..", CURRENT_WORKING_DIRECTORY)
///             .unwrap()
///             .color(Color::RED),
///     )
///     .command(
///         ConsoleCommand::from_string("test3", "ls -la ../..", CURRENT_WORKING_DIRECTORY)
///             .unwrap()
///             .color(Color::GREEN),
///     )
///     .execute();
/// handle.join().unwrap();
/// ```

#[derive(Clone)]
pub struct ConsoleCommand {
    inner_command: InnerCommand,
    color: Color,
}

impl ConsoleCommand {
    /// Set the color of the text. This defaults to a random color chosen by the system.
    /// The system will automatically choose visually distinct colors according to the commands passed to the `Runner` instance.
    pub fn color(&mut self, color: Color) -> &mut Self {
        self.color = color;
        self
    }
}

impl Command for ConsoleCommand {
    fn insert_command(cmd: InnerCommand) -> Self {
        ConsoleCommand {
            inner_command: cmd,
            color: Color::Random,
        }
    }

    fn get_command(&self) -> &InnerCommand {
        &self.inner_command
    }

    fn get_command_mut(&mut self) -> &mut InnerCommand {
        &mut self.inner_command
    }
}

impl AsRef<ConsoleCommand> for ConsoleCommand {
    fn as_ref(&self) -> &ConsoleCommand {
        self
    }
}

pub fn run_commands_stdout(runner: &Runner<ConsoleCommand>) -> ControlledCommandHandle {
    let mut name_color_hash = HashMap::new();
    let mut inner_commands = Vec::new();
    let mut num_cmds = 0;
    let options = runner.to_options();

    for cmd in &runner.commands {
        name_color_hash.insert(cmd.inner_command.name.to_string(), cmd.color.clone());
        inner_commands.push(cmd.inner_command.clone());
        num_cmds += 1;
    }

    color::populate_random_colors(&mut name_color_hash);

    let quiet = options.quiet;
    let file_handle_flags = options.file_handle_flags;

    let handle = super::run_commands(runner);

    let recv = handle.channel;

    let template_strings = runner.get_template_strings();

    let supervisor = thread::spawn(move || {
        process_channel(
            &recv,
            &name_color_hash,
            num_cmds,
            quiet,
            file_handle_flags,
            template_strings,
        );
    });
    ControlledCommandHandle {
        supervisor,
        handle: handle.handle,
        kill_trigger: handle.kill_trigger,
        pids: handle.pids,
    }
}

fn process_channel(
    chan: &mpsc::Receiver<super::OutputMessage>,
    color_map: &HashMap<String, Color>,
    num_cmds: usize,
    quiet: bool,
    file_handle_flags: bool,
    template_strings: TemplateStrings,
) {
    loop {
        let message = chan.recv();
        if message.is_err() {
            return;
        }

        let message = message.unwrap();
        let output_color = color_map.get(&message.name).unwrap();
        let color_open_sequence = color::open_sequence(output_color);
        let mut template = template::Template::new(Some(output_color));
        template.name = message.name.clone();
        let color_reset_sequence = color::close_sequence();
        let std_out_flag = if file_handle_flags { " (o)" } else { "" };
        let std_err_flag = if file_handle_flags { " (e)" } else { "" };
        let mut stdout = std::io::stdout();
        let _ = stdout.write_all(color_open_sequence.as_bytes());
        let _ = match message.message {
            OutputMessagePayload::Start => {
                if !quiet {
                    let template_string =
                        template.execute(&template_strings.start_message_template);
                    stdout.write_all(
                        format!("{}{}\n", template_string, color_reset_sequence).as_bytes(),
                    )
                } else {
                    Ok(())
                }
            }
            OutputMessagePayload::Done(exit_status) => {
                if !quiet {
                    template.status_code = exit_status;
                    let template_string = template.execute(&template_strings.done_message_template);
                    stdout.write_all(
                        format!("{}{}\n", template_string, color_reset_sequence).as_bytes(),
                    )
                } else {
                    Ok(())
                }
            }
            OutputMessagePayload::Stdout(ending, mut bytes) => {
                template.handle_flag = std_out_flag.to_string();
                let mut prefix = format!(
                    "{}{} ",
                    template.execute(&template_strings.payload_message_template),
                    color_reset_sequence
                )
                .into_bytes();
                prefix.append(&mut bytes);
                if num_cmds == 1 && ending.is_carriage_return() {
                    prefix.push(b'\r');
                } else {
                    prefix.push(b'\n');
                }
                stdout.write_all(&prefix)
            }
            OutputMessagePayload::Stderr(ending, mut bytes) => {
                template.handle_flag = std_err_flag.to_string();
                let mut prefix = format!(
                    "{}{} ",
                    template.execute(&template_strings.payload_message_template),
                    color_reset_sequence
                )
                .into_bytes();
                prefix.append(&mut bytes);
                if num_cmds == 1 && ending.is_carriage_return() {
                    prefix.push(b'\r');
                } else {
                    prefix.push(b'\n');
                }
                stdout.write_all(&prefix)
            }
            OutputMessagePayload::Error(e) => {
                template.error_message = e.to_string();
                stdout.write_all(
                    format!(
                        "{}{}\n",
                        template.execute(&template_strings.error_message_template),
                        color_reset_sequence
                    )
                    .as_bytes(),
                )
            }
        };
    }
}

pub fn parse_command_string<S>(command: S) -> Result<(String, Vec<String>), CommandError>
where
    S: Into<String>,
{
    let command_string = command.into();
    let mut words = shell_words::split(&command_string)
        .map_err(|_| CommandError::ParseError(command_string))?;
    if words.is_empty() {
        return Err(CommandError::EmptyCommand);
    }

    let parsed_command = words.remove(0);
    Ok((parsed_command, words))
}

#[cfg(test)]
mod tests {
    use super::ConsoleCommand;
    use crate::Command;
    use crate::RestartOptions;
    use crate::Runner;
    use crate::CURRENT_WORKING_DIRECTORY;

    #[test]
    fn run_commands() {
        let handle = Runner::new()
            .command(
                ConsoleCommand::from_string("test1", "ls -la .", CURRENT_WORKING_DIRECTORY)
                    .unwrap(),
            )
            .command(
                ConsoleCommand::from_string("test2", "ls -la ..", CURRENT_WORKING_DIRECTORY)
                    .unwrap(),
            )
            .command(ConsoleCommand::from_string("test3", "ls -la ../..", Some("..")).unwrap())
            .restart(RestartOptions::Kill)
            .execute();
        let _ = handle.join();
    }
}
