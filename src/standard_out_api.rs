use super::color;
use super::color::Color;
use super::Command;
use super::CommandError;
use super::ControlledCommandHandle;
use super::Options;
use super::OutputMessagePayload;

use std::collections::HashMap;
use std::io::Write;
use std::sync::mpsc;
use std::thread;

pub struct StandardOutCommand {
    inner_command: Command,
    color: Color,
}

impl StandardOutCommand {
    pub fn new<S, C, ArgType, Cmds, D>(
        name: S,
        command: C,
        args: Cmds,
        cur_dir: Option<D>,
    ) -> Result<StandardOutCommand, CommandError>
    where
        S: AsRef<str>,
        C: AsRef<str>,
        ArgType: AsRef<str>,
        Cmds: IntoIterator<Item = ArgType>,
        D: AsRef<str>,
    {
        Ok(StandardOutCommand {
            inner_command: Command::new(name, command, args, cur_dir)?,
            color: Color::Random,
        })
    }

    pub fn new_command_string<S, C, D>(
        name: S,
        command_string: C,
        cur_dir: Option<D>,
    ) -> Result<StandardOutCommand, CommandError>
    where
        S: AsRef<str>,
        C: AsRef<str>,
        D: AsRef<str>,
    {
        let (command, args) = parse_command_string(command_string)?;
        Ok(StandardOutCommand {
            inner_command: Command::new(name, command, args, cur_dir)?,
            color: Color::Random,
        })
    }

    pub fn new_command_string_with_color<S, C, D>(
        name: S,
        command_string: C,
        cur_dir: Option<D>,
        color: Color,
    ) -> Result<StandardOutCommand, CommandError>
    where
        S: AsRef<str>,
        C: AsRef<str>,
        D: AsRef<str>,
    {
        let (command, args) = parse_command_string(command_string)?;
        Ok(StandardOutCommand {
            inner_command: Command::new(name, command, args, cur_dir)?,
            color,
        })
    }

    pub fn new_with_color<S, C, ArgType, Cmds, D>(
        name: S,
        command: C,
        args: Cmds,
        cur_dir: Option<D>,
        color: Color,
    ) -> Result<StandardOutCommand, CommandError>
    where
        S: AsRef<str>,
        C: AsRef<str>,
        ArgType: AsRef<str>,
        Cmds: IntoIterator<Item = ArgType>,
        D: AsRef<str>,
    {
        Ok(StandardOutCommand {
            inner_command: Command::new(name, command, args, cur_dir)?,
            color,
        })
    }
}

pub fn run_commands_stdout<Cmds>(commands: Cmds) -> ControlledCommandHandle
where
    Cmds: IntoIterator<Item = StandardOutCommand>,
{
    run_commands_stdout_with_options(commands, super::Options::new())
}

pub fn run_commands_stdout_with_options<Cmds>(
    commands: Cmds,
    options: Options,
) -> ControlledCommandHandle
where
    Cmds: IntoIterator<Item = StandardOutCommand>,
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
        let color_open_sequence = output_color.open_sequence();
        let color_reset_sequence = output_color.close_sequence();
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
    let mut words = shell_words::split(command.as_ref()).map_err(|_| CommandError::ParseError)?;
    if words.is_empty() {
        return Err(CommandError::EmptyCommand);
    }

    let parsed_command = words.remove(0);
    Ok((parsed_command, words))
}

#[cfg(test)]
mod tests {
    use super::run_commands_stdout;

    use super::StandardOutCommand;
    use crate::RestartOptions;

    #[test]
    fn run_commands() {
        let dir: Option<String> = None;
        let commands = vec![
            StandardOutCommand::new_command_string("test1", "ls -la .", dir.clone()).unwrap(),
            StandardOutCommand::new_command_string("test2", "ls -la ..", dir.clone()).unwrap(),
            StandardOutCommand::new_command_string("test3", "ls -la ../..", Some("..")).unwrap(),
        ];

        let mut opts = super::Options::new();
        opts.restart(RestartOptions::Kill);

        let handle = run_commands_stdout(commands);
        handle.join();
    }
}
