mod com;

use shared::MsgFromHost;

use crate::com::Tcp;

fn main() -> std::io::Result<()> {
    let mut stream = Tcp::new("0.0.0.0:12345")?;
    stream.write(MsgFromHost::Basic("Hello there.".to_string()));
    match stream.read() {
        Ok(msg) => println!("Received message: {:?}", msg),
        Err(e) => println!("Error receiving message: {e}"),
    }

    Ok(())
}
