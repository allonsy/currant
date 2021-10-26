use super::line_parse;
use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::process;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Condvar;
use std::sync::Mutex;
use std::thread;

pub struct Command {
    name: String,
    command: String,
    args: Vec<String>,
}

impl Command {
    pub fn new<S, C, ArgType, Cmds>(name: S, command: C, args: Cmds) -> Command
    where
        S: AsRef<str>,
        C: AsRef<str>,
        ArgType: AsRef<str>,
        Cmds: IntoIterator<Item = ArgType>,
    {
        let converted_args = args
            .into_iter()
            .map(|s| s.as_ref().to_string())
            .collect::<Vec<String>>();
        Command {
            name: name.as_ref().to_string(),
            command: command.as_ref().to_string(),
            args: converted_args,
        }
    }
}

pub struct OutputMessage {
    pub name: String,
    pub message: OutputMessagePayload,
}

pub enum OutputMessagePayload {
    Done(Option<i32>),
    Stdout(line_parse::LineEnding, Vec<u8>),
    Stderr(line_parse::LineEnding, Vec<u8>),
    Error(io::Error),
}

pub struct CommandHandle {
    handle: thread::JoinHandle<()>,
    channel: mpsc::Receiver<OutputMessage>,
}

impl CommandHandle {
    pub fn join(self) {
        self.handle
            .join()
            .unwrap_or_else(|_| panic!("Unable to join on handle"));
    }

    pub fn get_output_channel(&self) -> &mpsc::Receiver<OutputMessage> {
        &self.channel
    }
}

#[derive(Clone)]
pub enum RestartOptions {
    Continue,
    Restart,
    Kill,
}

#[derive(Clone)]
pub struct Options {
    restart: RestartOptions,
}

impl Options {
    pub fn new() -> Options {
        Options {
            restart: RestartOptions::Continue,
        }
    }

    pub fn restart(&mut self, restart: RestartOptions) {
        self.restart = restart;
    }
}

pub fn run_commands<Cmds>(commands: Cmds, options: Options) -> CommandHandle
where
    Cmds: IntoIterator<Item = Command>,
{
    let actual_cmds = commands.into_iter().collect::<Vec<Command>>();
    run_commands_internal(actual_cmds, options)
}

fn run_commands_internal(commands: Vec<Command>, options: Options) -> CommandHandle {
    let (send, recv) = mpsc::channel();
    let is_dead = Arc::new(Mutex::new(false));
    let condvar = Arc::new(Condvar::new());

    let handle = thread::spawn(move || {
        let mut handles = Vec::new();
        for cmd in commands {
            let is_dead_clone = is_dead.clone();
            let condvar_clone = condvar.clone();
            handles.push(run_command(
                cmd,
                send.clone(),
                options.clone(),
                is_dead_clone,
                condvar_clone,
            ));
        }

        for handle in handles {
            let _ = handle.join();
        }
    });

    CommandHandle {
        handle,
        channel: recv,
    }
}

pub fn run_command(
    command: Command,
    send_chan: mpsc::Sender<OutputMessage>,
    options: Options,
    kill_mutex: Arc<Mutex<bool>>,
    condvar: Arc<Condvar>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || loop {
        let mut command_process = process::Command::new(&command.command);
        command_process.args(&command.args);
        command_process.stdout(process::Stdio::piped());
        let command_name = command.name.clone();
        let mut cmd_handle = command_process
            .spawn()
            .unwrap_or_else(|_| panic!("Unable to spawn process: {}", command.command.clone()));
        let std_out = cmd_handle.stdout.take();
        let std_err = cmd_handle.stderr.take();
        let mut std_out_handle = None;
        let mut std_err_handle = None;

        let shared_handle = Arc::new(Mutex::new(cmd_handle));

        if let RestartOptions::Kill = options.restart {
            let child_clone = shared_handle.clone();
            let kill_mutex_clone = kill_mutex.clone();
            let condvar_clone = condvar.clone();
            thread::spawn(move || kill_thread(kill_mutex_clone, condvar_clone, child_clone));
        }

        if let Some(output) = std_out {
            let mut buffered_stdout = BufReader::new(output);
            let new_name = command_name.clone();
            let new_chan = send_chan.clone();
            std_out_handle = Some(thread::spawn(move || {
                read_stream(new_name, new_chan, &mut buffered_stdout, true);
            }));
        }

        if let Some(output) = std_err {
            let mut buffered_stdout = BufReader::new(output);
            let new_name = command_name.clone();
            let new_chan = send_chan.clone();
            std_err_handle = Some(thread::spawn(move || {
                read_stream(new_name, new_chan, &mut buffered_stdout, false);
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
                        break;
                    }
                    RestartOptions::Restart => {
                        if status.success() {
                            break;
                        }
                    }
                    RestartOptions::Kill => {
                        if !status.success() {
                            let mut is_dead = kill_mutex.lock().unwrap();
                            *is_dead = true;
                            condvar.notify_one();
                        }
                        break;
                    }
                };
            }
            Err(e) => {
                let _ = send_chan.send(OutputMessage {
                    name: command_name.clone(),
                    message: OutputMessagePayload::Error(e),
                });
            }
        }
    })
}

fn kill_thread(
    kill_mutex: Arc<Mutex<bool>>,
    condvar: Arc<Condvar>,
    child: Arc<Mutex<process::Child>>,
) {
    let mut is_dead = kill_mutex.lock().unwrap();

    while !*is_dead {
        is_dead = condvar.wait(is_dead).unwrap();
    }

    let lock_res = child.lock();
    if let Ok(mut locked_child) = lock_res {
        let _ = locked_child.kill();
    }

    condvar.notify_one();
}

fn read_stream<R>(
    cmd_name: String,
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
                    name: cmd_name.clone(),
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
                    name: cmd_name.clone(),
                    message: OutputMessagePayload::Error(e),
                });
            }
        }
    }
}
