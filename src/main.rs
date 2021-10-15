mod currant;
use std::fs::File;

fn main() {
    let newfile = File::create("outputlspar.txt").unwrap();
    let commands = vec![
        currant::Command::new(
            "lscur".to_string(),
            "ls".to_string(),
            vec!["-l".to_string(), ".".to_string()],
            currant::OutputType::Channel,
        ),
        currant::Command::new(
            "lspar".to_string(),
            "ls".to_string(),
            vec!["-l".to_string(), ".".to_string()],
            currant::OutputType::File(newfile),
        ),
        currant::Command::new(
            "lsparpar".to_string(),
            "ls".to_string(),
            vec!["-l".to_string(), "../..".to_string()],
            currant::OutputType::Stdout,
        ),
    ];

    let handle = currant::run_commands(commands);
    let recv = handle.get_output_channel();

    loop {
        let msg = recv.recv().unwrap();

        match msg.message {
            currant::OutputMessagePayload::Done(status) => {
                println!(
                    "currant: process {} exited with code: {}",
                    msg.name,
                    status.unwrap()
                );
                break;
            }
            currant::OutputMessagePayload::Stdout(line) => println!("{} (o): {}", msg.name, line),
            currant::OutputMessagePayload::Stderr(line) => println!("{} (e): {}", msg.name, line),
        }
    }
    handle.join();
}
