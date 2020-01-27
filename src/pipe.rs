use rml_rtmp::handshake::{Handshake, PeerType, HandshakeProcessResult};
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use bytes::{Bytes, BytesMut, BufMut};
use rml_rtmp::sessions::{ServerSession, ServerSessionConfig, ServerSessionResult, ServerSessionEvent, ServerSessionError};
use crate::services::Service;

#[derive(Debug, Fail)]
pub enum ServerError {
    #[fail(display = "io error: {}", 0)]
    IoError(io::Error),
    #[fail(display = "SocketClosed")]
    SocketClosed,
    #[fail(display = "ServerSessionError: {}", 0)]
    ServerSessionError(ServerSessionError),
}

impl From<io::Error> for ServerError {
    fn from(error: io::Error) -> Self {
        ServerError::IoError(error)
    }
}

enum PullState {
    Handshaking(Handshake),
    Connecting,
    Connected(ServerSession),
    Pulling,
    Closed,
}

pub struct Server {
    state: PullState,
}

impl Server {
    pub fn new() -> Self {
        Self {
            state: PullState::Handshaking(Handshake::new(PeerType::Server)),
        }
    }
    pub async fn process(&mut self, socket: &mut TcpStream) -> Result<(), failure::Error> {
        // let mut buffer = [0; 1024];
        let mut buffer = BytesMut::with_capacity(4096);

        loop {
            match socket.read_buf(&mut buffer).await {
                // socket closed
                Ok(n) if n == 0 => break,
                Ok(n) => n,
                Err(e) => {
                    eprintln!("failed to read from socket; err = {:?}", e);
                    break;
                }
            };

            let mut new_state: Option<PullState>;
            while buffer.len() > 0 {
                new_state = match &mut self.state {
                    PullState::Handshaking(handshake) => {
                        Self::handle_handshake(socket, handshake, &mut buffer).await?
                    },
                    PullState::Connecting => {
                        let config = ServerSessionConfig::new();
                        let (session, initial_session_results) = ServerSession::new(config)?;
                        Self::handle_server_session_results(socket, initial_session_results).await?;
                        Some(PullState::Connected(session))
                    },
                    PullState::Connected(session) => {
                        Self::handle_connected(socket, session, &mut buffer).await?
                    },
                    PullState::Closed => break,
                    other => None,
                };
                if let Some(state) = new_state {
                    self.state = state;
                }
            }
        }
        Ok(())
    }

    async fn handle_connected(socket: &mut TcpStream, session: &mut ServerSession, bytes: &mut BytesMut) -> Result<Option<PullState>, ServerError> {
        let session_results = match session.handle_input(bytes) {
            Ok(results) => results,
            Err(err) => return Err(ServerError::ServerSessionError(err)),
        };
        for result in session_results {
            match result {
                ServerSessionResult::OutboundResponse(packet) => {
                    socket.write_all(&packet.bytes[..]).await?;
                },
                ServerSessionResult::RaisedEvent(ServerSessionEvent::ConnectionRequested {
                    request_id,
                    app_name,
                }) => {
                    println!("Server result received: {:?}", x);
                }
                x => println!("Server result received: {:?}", x),
            }
        }
        Ok(None)
    }

    async fn handle_server_session_results(socket: &mut TcpStream, session_results: Vec<ServerSessionResult>) -> Result<(), ServerError> {
        for result in session_results {
            match result {
                ServerSessionResult::OutboundResponse(packet) => {
                    socket.write_all(&packet.bytes[..]).await?;
                },
                x => println!("Server result received: {:?}", x),
            }
        }
        Ok(())
    }

    async fn handle_handshake(socket: &mut TcpStream, handshake: &mut Handshake, bytes: &mut BytesMut) -> Result<Option<PullState>, ServerError> {
        let result = match handshake.process_bytes(bytes) {
            Ok(result) => result,
            Err(error) => {
                println!("Handshake error: {:?}", error);
                return Err(ServerError::SocketClosed);
            }
        };
        bytes.clear();

        match result {
            HandshakeProcessResult::InProgress {response_bytes} => {
                if response_bytes.len() > 0 {
                    socket.write_all(&response_bytes).await?;
                }

                Ok(None)
            },

            HandshakeProcessResult::Completed {response_bytes, remaining_bytes} => {
                println!("Handshake successful!");
                if response_bytes.len() > 0 {
                    socket.write_all(&response_bytes).await?;
                }

                bytes.put(&remaining_bytes[..]);

                Ok(Some(PullState::Connecting))
            }
        }
    }
}
