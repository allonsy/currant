use super::Command;
use super::ControlledCommandHandle;
use super::Options;
use super::OutputMessagePayload;

use std::io::Write;
use std::sync::mpsc;
use std::thread;

pub fn run_commands_writer<Cmds, W>(commands: Cmds, writer: W) -> ControlledCommandHandle
where
    Cmds: IntoIterator<Item = Command>,
    W: Write + Send + 'static,
{
    run_commands_writer_with_options(commands, writer, super::Options::new())
}

pub fn run_commands_writer_with_options<Cmds, W>(
    commands: Cmds,
    writer: W,
    options: Options,
) -> ControlledCommandHandle
where
    W: Write + Send + 'static,
    Cmds: IntoIterator<Item = Command>,
{
    let handle = super::run_commands(commands, options);

    let recv = handle.channel;

    thread::spawn(move || {
        process_channel(recv, writer);
    });
    ControlledCommandHandle {
        handle: handle.handle,
        kill_trigger: handle.kill_trigger,
    }
}

fn process_channel<W>(chan: mpsc::Receiver<super::OutputMessage>, mut writer: W)
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
