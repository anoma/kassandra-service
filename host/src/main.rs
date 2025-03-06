mod com;

use std::io::Write;
use crate::com::Tcp;
use shared::MsgFromHost;

fn main() -> std::io::Result<()> {
    let mut stream = Tcp::new("0.0.0.0:12345")?;
    stream.write(MsgFromHost::Basic("Hello there.".to_string())).unwrap();
    match stream.read() {
        Ok(msg) => println!("Received message: {:?}", msg),
        Err(e) => println!("Error receiving message: {e}"),
    }

    Ok(())
}
