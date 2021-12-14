use super::color;
use super::color::Color;
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

#[derive(Clone)]
pub struct ConsoleCommand {
    inner_command: InnerCommand,
    color: Color,
}

impl ConsoleCommand {
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

    thread::spawn(move || {
        process_channel(&recv, &name_color_hash, num_cmds, quiet, file_handle_flags);
    });
    ControlledCommandHandle {
        handle: handle.handle,
        kill_trigger: handle.kill_trigger,
    }
}

fn process_channel(
    chan: &mpsc::Receiver<super::OutputMessage>,
    color_map: &HashMap<String, Color>,
    num_cmds: usize,
    quiet: bool,
    file_handle_flags: bool,
) {
    loop {
        let message = chan.recv();
        if message.is_err() {
            return;
        }

        let message = message.unwrap();
        let output_color = color_map.get(&message.name).unwrap();
        let color_open_sequence = color::open_sequence(output_color);
        let color_reset_sequence = color::close_sequence();
        let std_out_flag = if file_handle_flags { " (o)" } else { "" };
        let std_err_flag = if file_handle_flags { " (e)" } else { "" };
        let mut stdout = std::io::stdout();
        let _ = stdout.write_all(color_open_sequence.as_bytes());
        let _ = match message.message {
            OutputMessagePayload::Start => {
                if !quiet {
                    stdout.write_all(
                        format!(
                            "{}SYSTEM: starting process {}{}\n",
                            color_open_sequence, message.name, color_reset_sequence
                        )
                        .as_bytes(),
                    )
                } else {
                    Ok(())
                }
            }
            OutputMessagePayload::Done(Some(exit_status)) => {
                if !quiet {
                    stdout.write_all(
                        format!(
                            "{}{}:{} process exited with status: {}\n",
                            color_open_sequence, message.name, color_reset_sequence, exit_status
                        )
                        .as_bytes(),
                    )
                } else {
                    Ok(())
                }
            }
            OutputMessagePayload::Done(None) => {
                if !quiet {
                    stdout.write_all(
                        format!(
                            "{}{}:{} process exited without exit status\n",
                            color_open_sequence, message.name, color_reset_sequence
                        )
                        .as_bytes(),
                    )
                } else {
                    Ok(())
                }
            }
            OutputMessagePayload::Stdout(ending, mut bytes) => {
                let mut prefix = format!(
                    "{}{}{}:{} ",
                    color_open_sequence, message.name, std_out_flag, color_reset_sequence
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
                let mut prefix = format!(
                    "{}{}{}:{} ",
                    color_open_sequence, message.name, std_err_flag, color_reset_sequence
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
            OutputMessagePayload::Error(e) => stdout.write_all(
                format!(
                    "{}SYSTEM (e): Encountered error with process {}: {}{}\n",
                    color_open_sequence, message.name, e, color_reset_sequence
                )
                .as_bytes(),
            ),
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

    #[test]
    fn run_commands() {
        let handle = Runner::new()
            .command(ConsoleCommand::from_string("test1", "ls -la .").unwrap())
            .command(ConsoleCommand::from_string("test2", "ls -la ..").unwrap())
            .command(
                ConsoleCommand::from_string("test3", "ls -la ../..")
                    .unwrap()
                    .cur_dir(".."),
            )
            .restart(RestartOptions::Kill)
            .execute();
        let _ = handle.join();
    }
}
