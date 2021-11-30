use super::color;
use super::color::Color;
use super::Command;
use super::CommandError;
use super::ControlledCommandHandle;
use super::Options;
use super::OutputMessagePayload;

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::sync::mpsc;
use std::thread;
pub struct ConsoleCommand {
    inner_command: Command,
    color: Color,
}

impl ConsoleCommand {
    pub fn new<S, C, ArgType, Cmds>(
        name: S,
        command: C,
        args: Cmds,
    ) -> Result<ConsoleCommand, CommandError>
    where
        S: AsRef<str>,
        C: AsRef<str>,
        ArgType: AsRef<str>,
        Cmds: IntoIterator<Item = ArgType>,
    {
        Ok(ConsoleCommand {
            inner_command: Command::new(name, command, args)?,
            color: Color::Random,
        })
    }

    pub fn full_cmd<S, C>(name: S, command_string: C) -> Result<ConsoleCommand, CommandError>
    where
        S: AsRef<str>,
        C: AsRef<str>,
    {
        let (command, args) = parse_command_string(command_string)?;
        Ok(ConsoleCommand {
            inner_command: Command::new(name, command, args)?,
            color: Color::Random,
        })
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn cur_dir<D>(mut self, cur_dir: D) -> Self
    where
        D: AsRef<Path>,
    {
        self.inner_command = self.inner_command.cur_dir(cur_dir);
        self
    }

    pub fn env<K, V>(mut self, key: K, val: V) -> Self
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        self.inner_command = self.inner_command.env(key, val);
        self
    }
}

pub fn run_commands_stdout<Cmds>(commands: Cmds) -> ControlledCommandHandle
where
    Cmds: IntoIterator<Item = ConsoleCommand>,
{
    run_commands_stdout_with_options(commands, super::Options::new())
}

pub fn run_commands_stdout_with_options<Cmds>(
    commands: Cmds,
    options: Options,
) -> ControlledCommandHandle
where
    Cmds: IntoIterator<Item = ConsoleCommand>,
{
    let mut name_color_hash = HashMap::new();
    let mut inner_commands = Vec::new();
    let mut num_cmds = 0;

    for cmd in commands {
        name_color_hash.insert(cmd.inner_command.name.to_string(), cmd.color.clone());
        inner_commands.push(cmd.inner_command);
        num_cmds += 1;
    }

    color::populate_random_colors(&mut name_color_hash);

    let verbose = options.verbose;
    let file_handle_flags = options.file_handle_flags;

    let handle = super::run_commands(inner_commands, options);

    let recv = handle.channel;

    thread::spawn(move || {
        process_channel(recv, name_color_hash, num_cmds, verbose, file_handle_flags);
    });
    ControlledCommandHandle {
        handle: handle.handle,
        kill_trigger: handle.kill_trigger,
    }
}

fn process_channel(
    chan: mpsc::Receiver<super::OutputMessage>,
    color_map: HashMap<String, Color>,
    num_cmds: usize,
    verbose: bool,
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
        let std_out_flag = if file_handle_flags { "(o)" } else { "" };
        let std_err_flag = if file_handle_flags { "(e)" } else { "" };
        let mut stdout = std::io::stdout();
        let _ = stdout.write_all(color_open_sequence.as_bytes());
        let _ = match message.message {
            OutputMessagePayload::Start => {
                if verbose {
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
                if verbose {
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
                if verbose {
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
                    "{}{} {}:{} ",
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
                    "{}{} {}:{} ",
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
    S: AsRef<str>,
{
    let mut words = shell_words::split(command.as_ref())
        .map_err(|_| CommandError::ParseError(command.as_ref().to_string()))?;
    if words.is_empty() {
        return Err(CommandError::EmptyCommand);
    }

    let parsed_command = words.remove(0);
    Ok((parsed_command, words))
}

#[cfg(test)]
mod tests {
    use super::run_commands_stdout;

    use super::ConsoleCommand;
    use crate::RestartOptions;

    #[test]
    fn run_commands() {
        let commands = vec![
            ConsoleCommand::full_cmd("test1", "ls -la .").unwrap(),
            ConsoleCommand::full_cmd("test2", "ls -la ..").unwrap(),
            ConsoleCommand::full_cmd("test3", "ls -la ../..")
                .unwrap()
                .cur_dir(".."),
        ];

        let mut opts = super::Options::new();
        opts.restart(RestartOptions::Kill);

        let handle = run_commands_stdout(commands);
        let _ = handle.join();
    }
}
