#[macro_use] extern crate failure;
mod pipe;
mod services;

use tokio::net::TcpListener;
use pipe::Server;
use services::BilibiliService;

#[tokio::main]
async fn main() -> Result<(), failure::Error> {
    let mut listener = TcpListener::bind("127.0.0.1:19350").await?;

    loop {
        let (mut socket, _) = listener.accept().await?;
        let mut server = Server::new();

        tokio::spawn(async move {
            if let Err(err) = server.process(&mut socket).await {
                dbg!(err);
            }
        });
    }
}
