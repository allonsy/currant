use super::Command;
use super::ControlledCommandHandle;
use super::Options;
use super::OutputMessagePayload;

use rand::seq::SliceRandom;
use std::collections::HashMap;
use std::io::Write;
use std::sync::mpsc;
use std::thread;

#[derive(Clone)]
pub enum Color {
    Default,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    Black,
}

impl Color {
    fn get_color_list() -> Vec<Color> {
        vec![
            Color::Red,
            Color::Green,
            Color::Yellow,
            Color::Blue,
            Color::Magenta,
            Color::Cyan,
        ]
    }
    pub fn random() -> Color {
        let rand_int: u32 = rand::random();
        let chosen_variant = rand_int % 8;

        match chosen_variant {
            0 => Color::Red,
            1 => Color::Green,
            2 => Color::Yellow,
            3 => Color::Blue,
            4 => Color::Magenta,
            5 => Color::Cyan,
            6 => Color::White,
            7 => Color::Black,
            _ => panic!("Unable to generate random color"),
        }
    }

    pub fn random_color_list(num_colors: usize) -> Vec<Color> {
        let mut colors = Color::get_color_list();
        let mut rng = rand::thread_rng();
        colors.shuffle(&mut rng);

        while colors.len() < num_colors {
            let mut new_colors = colors.clone();
            colors.append(&mut new_colors);
        }

        colors.into_iter().take(num_colors).collect()
    }

    fn open_sequence(&self) -> String {
        match self {
            Color::Default => "",
            Color::Black => "\x1b[30m",
            Color::Red => "\x1b[31m",
            Color::Green => "\x1b[32m",
            Color::Yellow => "\x1b[33m",
            Color::Blue => "\x1b[34m",
            Color::Magenta => "\x1b[35m",
            Color::Cyan => "\x1b[36m",
            Color::White => "\x1b[37m",
        }
        .to_string()
    }

    fn close_sequence(&self) -> String {
        "\x1b[0m".to_string()
    }
}

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
    ) -> StandardOutCommand
    where
        S: AsRef<str>,
        C: AsRef<str>,
        ArgType: AsRef<str>,
        Cmds: IntoIterator<Item = ArgType>,
        D: AsRef<str>,
    {
        StandardOutCommand {
            inner_command: Command::new(name, command, args, cur_dir),
            color: Color::Default,
        }
    }

    pub fn new_command_string<S, C, D>(
        name: S,
        command_string: C,
        cur_dir: Option<D>,
    ) -> StandardOutCommand
    where
        S: AsRef<str>,
        C: AsRef<str>,
        D: AsRef<str>,
    {
        let (command, args) = parse_command_string(command_string);
        StandardOutCommand {
            inner_command: Command::new(name, command, args, cur_dir),
            color: Color::Default,
        }
    }

    pub fn new_command_string_with_color<S, C, D>(
        name: S,
        command_string: C,
        cur_dir: Option<D>,
        color: Color,
    ) -> StandardOutCommand
    where
        S: AsRef<str>,
        C: AsRef<str>,
        D: AsRef<str>,
    {
        let (command, args) = parse_command_string(command_string);
        StandardOutCommand {
            inner_command: Command::new(name, command, args, cur_dir),
            color,
        }
    }

    pub fn new_with_color<S, C, ArgType, Cmds, D>(
        name: S,
        command: C,
        args: Cmds,
        cur_dir: Option<D>,
        color: Color,
    ) -> StandardOutCommand
    where
        S: AsRef<str>,
        C: AsRef<str>,
        ArgType: AsRef<str>,
        Cmds: IntoIterator<Item = ArgType>,
        D: AsRef<str>,
    {
        StandardOutCommand {
            inner_command: Command::new(name, command, args, cur_dir),
            color,
        }
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

    let handle = super::run_commands(inner_commands, options);

    let recv = handle.channel;

    thread::spawn(move || {
        process_channel(recv, name_color_hash, num_cmds);
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
        let mut stdout = std::io::stdout();
        let _ = stdout.write_all(color_open_sequence.as_bytes());
        let _ = match message.message {
            OutputMessagePayload::Start => stdout.write_all(
                format!(
                    "{}SYSTEM: starting process {}{}\n",
                    color_open_sequence, message.name, color_reset_sequence
                )
                .as_bytes(),
            ),
            OutputMessagePayload::Done(Some(exit_status)) => stdout.write_all(
                format!(
                    "{}{}:{} process exited with status: {}\n",
                    color_open_sequence, message.name, color_reset_sequence, exit_status
                )
                .as_bytes(),
            ),
            OutputMessagePayload::Done(None) => stdout.write_all(
                format!(
                    "{}{}:{} process exited without exit status\n",
                    color_open_sequence, message.name, color_reset_sequence
                )
                .as_bytes(),
            ),
            OutputMessagePayload::Stdout(ending, mut bytes) => {
                let mut prefix = format!(
                    "{}{} (o):{} ",
                    color_open_sequence, message.name, color_reset_sequence
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
                    "{}{} (e):{} ",
                    color_open_sequence, message.name, color_reset_sequence
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

pub fn parse_command_string<S>(command: S) -> (String, Vec<String>)
where
    S: AsRef<str>,
{
    let mut words = shell_words::split(command.as_ref()).unwrap();
    if words.is_empty() {
        panic!("Command string contains no command");
    }

    let parsed_command = words.remove(0);
    (parsed_command, words)
}

#[cfg(test)]
mod tests {
    use super::run_commands_stdout;
    use super::Color;
    use super::StandardOutCommand;
    use crate::RestartOptions;

    #[test]
    fn run_commands() {
        let dir: Option<String> = None;
        let commands = vec![
            StandardOutCommand::new_command_string_with_color(
                "test1",
                "ls -la .",
                dir.clone(),
                Color::Blue,
            ),
            StandardOutCommand::new_command_string_with_color(
                "test2",
                "ls -la ..",
                dir.clone(),
                Color::Red,
            ),
            StandardOutCommand::new_command_string_with_color(
                "test3",
                "ls -la ../..",
                Some(".."),
                Color::Green,
            ),
        ];

        let mut opts = super::Options::new();
        opts.restart(RestartOptions::Kill);

        let handle = run_commands_stdout(commands);
        handle.join();
    }
}
