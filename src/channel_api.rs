use super::Command;
use super::InnerCommand;

/// This is the lowest-level of the three apis.
/// It returns a channel which allows the caller to process all command output manually.
/// This API provides the most flexibility at the cost of the most work to the user.
/// ## Example:
/// ```
/// use currant::ChannelCommand;
/// use currant::Command;
/// use currant::OutputMessagePayload;
/// use currant::Runner;
///
/// let handle = Runner::new()
///     .command(ChannelCommand::from_string("test1", "ls -la .").unwrap())
///     .command(ChannelCommand::from_string("test2", "ls -la ..").unwrap())
///     .command(ChannelCommand::from_string("test3", "ls -la ../..").unwrap())
///     .execute();
///
/// for msg in &handle {
///     print!("{}: ", msg.name);
///     match msg.message {
///         OutputMessagePayload::Done(status) => println!("exited with status: {:?}", status),
///         OutputMessagePayload::Error(e) => println!("errored with message: {}", e),
///         OutputMessagePayload::Start => println!("Started"),
///         OutputMessagePayload::Stdout(_, bytes) => {
///             println!("stdout: {}", String::from_utf8_lossy(&bytes))
///         }
///         OutputMessagePayload::Stderr(_, bytes) => {
///             println!("stderr: {}", String::from_utf8_lossy(&bytes))
///         }
///     }
/// }
///
/// handle.join().unwrap();
/// ```
/// Also see `examples/main_channel.rs` for a full version
#[derive(Clone)]
pub struct ChannelCommand {
    inner_command: InnerCommand,
}

impl Command for ChannelCommand {
    fn insert_command(cmd: InnerCommand) -> Self {
        ChannelCommand { inner_command: cmd }
    }

    fn get_command(&self) -> &InnerCommand {
        &self.inner_command
    }

    fn get_command_mut(&mut self) -> &mut InnerCommand {
        &mut self.inner_command
    }
}

impl AsRef<ChannelCommand> for ChannelCommand {
    fn as_ref(&self) -> &ChannelCommand {
        self
    }
}
