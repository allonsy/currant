mod currant;
mod line_parse;

fn main() {
    let commands = vec![
        currant::Command::new("lscur", "ls", vec!["-l", "."]),
        currant::Command::new("lspar", "ls", vec!["-l", ".."]),
        currant::Command::new("lsparpar", "ls", vec!["-l", "../.."]),
    ];

    let handle = currant::run_commands(commands);
    let recv = handle.get_output_channel();
    let mut num_done = 0;

    loop {
        if num_done >= 3 {
            break;
        }
        let msg = recv.recv().unwrap();

        match msg.message {
            currant::OutputMessagePayload::Done(status) => {
                println!(
                    "currant: process {} exited with code: {}",
                    msg.name,
                    status.unwrap()
                );
                num_done += 1;
            }
            currant::OutputMessagePayload::Stdout(_, line) => {
                println!("{} (o): {}", msg.name, String::from_utf8(line).unwrap());
            }
            currant::OutputMessagePayload::Stderr(_, line) => {
                println!("{} (e): {}", msg.name, String::from_utf8(line).unwrap());
            }
            currant::OutputMessagePayload::Error(e) => {
                println!("currant (e): {}", e);
            }
        }
    }
    handle.join();
}
