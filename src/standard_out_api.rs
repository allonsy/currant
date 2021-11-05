use super::Command;
use super::ControlledCommandHandle;
use super::Options;
use super::OutputMessagePayload;

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

    fn open_sequence(&self) -> Vec<u8> {
        match self {
            Color::Default => Vec::new(),
            Color::Black => vec![27, 91, 48, 59, 51, 48, 109],
            Color::Red => vec![27, 91, 48, 59, 51, 49, 109],
            Color::Green => vec![27, 91, 48, 59, 51, 50, 109],
            Color::Yellow => vec![27, 91, 48, 59, 51, 51, 109],
            Color::Blue => vec![27, 91, 48, 59, 51, 52, 109],
            Color::Magenta => vec![27, 91, 48, 59, 51, 53, 109],
            Color::Cyan => vec![27, 91, 48, 59, 51, 54, 109],
            Color::White => vec![27, 91, 48, 59, 51, 55, 109],
        }
    }

    fn close_sequence(&self) -> Vec<u8> {
        vec![27, 91, 48, 109]
    }
}

pub struct StandardOutCommand {
    inner_command: Command,
    color: Color,
}

impl StandardOutCommand {
    pub fn new<S, C, ArgType, Cmds>(name: S, command: C, args: Cmds) -> StandardOutCommand
    where
        S: AsRef<str>,
        C: AsRef<str>,
        ArgType: AsRef<str>,
        Cmds: IntoIterator<Item = ArgType>,
    {
        StandardOutCommand {
            inner_command: Command::new(name, command, args),
            color: Color::Default,
        }
    }

    pub fn new_command_string<S, C>(name: S, command_string: C) -> StandardOutCommand
    where
        S: AsRef<str>,
        C: AsRef<str>,
    {
        let (command, args) = parse_command_string(command_string);
        StandardOutCommand {
            inner_command: Command::new(name, command, args),
            color: Color::Default,
        }
    }

    pub fn new_command_string_with_color<S, C>(
        name: S,
        command_string: C,
        color: Color,
    ) -> StandardOutCommand
    where
        S: AsRef<str>,
        C: AsRef<str>,
    {
        let (command, args) = parse_command_string(command_string);
        StandardOutCommand {
            inner_command: Command::new(name, command, args),
            color,
        }
    }

    pub fn new_with_color<S, C, ArgType, Cmds>(
        name: S,
        command: C,
        args: Cmds,
        color: Color,
    ) -> StandardOutCommand
    where
        S: AsRef<str>,
        C: AsRef<str>,
        ArgType: AsRef<str>,
        Cmds: IntoIterator<Item = ArgType>,
    {
        StandardOutCommand {
            inner_command: Command::new(name, command, args),
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
        let mut stdout = std::io::stdout();
        let _ = stdout.write_all(&output_color.open_sequence());
        let _ = match message.message {
            OutputMessagePayload::Done(Some(exit_status)) => stdout.write_all(
                format!(
                    "{}: process exited with status: {}\n",
                    message.name, exit_status
                )
                .as_bytes(),
            ),
            OutputMessagePayload::Done(None) => stdout.write_all(
                format!("{}: process exited without exit status\n", message.name).as_bytes(),
            ),
            OutputMessagePayload::Stdout(ending, mut bytes) => {
                let mut prefix = format!("{} (o): ", message.name,).into_bytes();
                prefix.append(&mut bytes);
                if num_cmds == 1 && ending.is_carriage_return() {
                    prefix.push(b'\r');
                } else {
                    prefix.push(b'\n');
                }
                stdout.write_all(&prefix)
            }
            OutputMessagePayload::Stderr(ending, mut bytes) => {
                let mut prefix = format!("{} (e): ", message.name,).into_bytes();
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
                    "currant (e): Encountered error with process {}: {}\n",
                    message.name, e
                )
                .as_bytes(),
            ),
        };

        let _ = std::io::stdout().write_all(&output_color.close_sequence());
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
