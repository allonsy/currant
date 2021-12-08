use currant::Command;
use currant::CommandOperations;
use currant::OutputMessagePayload;
use currant::Runner;

fn main() {
    let handle = currant::run_commands(
        Runner::new()
            .command(Command::from_string("test1", "ls -la .").unwrap())
            .command(Command::from_string("test2", "ls -la ..").unwrap())
            .command(Command::from_string("test3", "ls -la ../..").unwrap()),
    );

    for msg in &handle {
        print!("{}: ", msg.name);
        match msg.message {
            OutputMessagePayload::Done(status) => println!("exited with status: {:?}", status),
            OutputMessagePayload::Error(e) => println!("errored with message: {}", e),
            OutputMessagePayload::Start => println!("Started"),
            OutputMessagePayload::Stdout(_, bytes) => {
                println!("stdout: {}", String::from_utf8_lossy(&bytes))
            }
            OutputMessagePayload::Stderr(_, bytes) => {
                println!("stderr: {}", String::from_utf8_lossy(&bytes))
            }
        }
    }

    handle.join().unwrap();
}
