use super::kill_barrier;
use super::line_parse;
use super::CommandHandle;
use super::ExitResult;
use super::InnerCommand;
use super::Options;
use super::OutputMessage;
use super::OutputMessagePayload;
use super::RestartOptions;
use std::io::BufRead;
use std::io::BufReader;
use std::process;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;

pub(super) fn run_commands_internal(
    commands: Vec<InnerCommand>,
    options: Options,
) -> CommandHandle {
    let (send, recv) = mpsc::channel();
    let kill_trigger = kill_barrier::KillBarrier::new();
    let kill_trigger_clone = kill_trigger.clone();

    let command_names: Vec<String> = commands.iter().map(|cmd| cmd.name.clone()).collect();

    let handle = thread::spawn(move || {
        let mut handles = Vec::new();
        let mut statuses = Vec::new();
        for cmd in commands {
            handles.push(run_command(
                cmd,
                send.clone(),
                options.clone(),
                kill_trigger_clone.clone(),
            ));
        }

        for (idx, handle) in handles.into_iter().enumerate() {
            statuses.push(handle.join().unwrap_or((command_names[idx].clone(), None)));
        }

        statuses
    });

    CommandHandle {
        handle,
        channel: recv,
        kill_trigger,
    }
}

fn run_command(
    command: InnerCommand,
    send_chan: mpsc::Sender<OutputMessage>,
    options: Options,
    kill_trigger: kill_barrier::KillBarrier,
) -> thread::JoinHandle<ExitResult> {
    thread::spawn(move || loop {
        let mut command_process = process::Command::new(&command.command);
        command_process.args(&command.args);
        if command.cur_dir.is_some() {
            command_process.current_dir(command.cur_dir.clone().unwrap());
        }
        command_process.envs(command.env.clone());
        command_process.stdout(process::Stdio::piped());
        let command_name = command.name.clone();

        let _ = send_chan.send(OutputMessage {
            name: command_name.clone(),
            message: OutputMessagePayload::Start,
        });

        let mut cmd_handle = command_process
            .spawn()
            .unwrap_or_else(|_| panic!("Unable to spawn process: {}", command.command.clone()));
        let std_out = cmd_handle.stdout.take();
        let std_err = cmd_handle.stderr.take();
        let mut std_out_handle = None;
        let mut std_err_handle = None;

        let shared_handle = Arc::new(Mutex::new(cmd_handle));

        let child_clone = shared_handle.clone();
        let kill_trigger_clone = kill_trigger.clone();
        thread::spawn(move || kill_thread(&kill_trigger_clone, child_clone));

        if let Some(output) = std_out {
            let mut buffered_stdout = BufReader::new(output);
            let new_name = command_name.clone();
            let new_chan = send_chan.clone();
            std_out_handle = Some(thread::spawn(move || {
                read_stream(&new_name, new_chan, &mut buffered_stdout, true);
            }));
        }

        if let Some(output) = std_err {
            let mut buffered_stdout = BufReader::new(output);
            let new_name = command_name.clone();
            let new_chan = send_chan.clone();
            std_err_handle = Some(thread::spawn(move || {
                read_stream(&new_name, new_chan, &mut buffered_stdout, false);
            }));
        }

        if let Some(handle) = std_out_handle {
            let _ = handle.join();
        }

        if let Some(handle) = std_err_handle {
            let _ = handle.join();
        }

        let exit_status = shared_handle.lock().unwrap().wait();
        match exit_status {
            Ok(status) => {
                let _ = send_chan.send(OutputMessage {
                    name: command_name.clone(),
                    message: OutputMessagePayload::Done(status.code()),
                });

                match options.restart {
                    RestartOptions::Continue => {
                        return (command_name, Some(status));
                    }
                    RestartOptions::Restart => {
                        if status.success() {
                            return (command_name, Some(status));
                        }
                    }
                    RestartOptions::Kill => {
                        if !status.success() {
                            let _ = kill_trigger.initiate_kill();
                        }
                        return (command_name, Some(status));
                    }
                };
            }
            Err(e) => {
                let _ = send_chan.send(OutputMessage {
                    name: command_name.clone(),
                    message: OutputMessagePayload::Error(e),
                });
                return (command_name, None);
            }
        }
    })
}

fn kill_thread(kill_trigger: &kill_barrier::KillBarrier, child: Arc<Mutex<process::Child>>) {
    let _ = kill_trigger.wait();

    let lock_res = child.lock();
    if let Ok(mut locked_child) = lock_res {
        let _ = locked_child.kill();
    }
}

fn read_stream<R>(
    cmd_name: &str,
    send_chan: mpsc::Sender<OutputMessage>,
    reader: &mut R,
    is_stdout: bool,
) where
    R: BufRead,
{
    loop {
        let line = line_parse::get_line(reader);
        match line {
            Ok(Some(line_vec)) => {
                let _ = send_chan.send(OutputMessage {
                    name: cmd_name.to_string(),
                    message: if is_stdout {
                        OutputMessagePayload::Stdout(line_vec.0, line_vec.1)
                    } else {
                        OutputMessagePayload::Stderr(line_vec.0, line_vec.1)
                    },
                });
            }
            Ok(None) => {
                return;
            }
            Err(e) => {
                let _ = send_chan.send(OutputMessage {
                    name: cmd_name.to_string(),
                    message: OutputMessagePayload::Error(e),
                });
            }
        }
    }
}
