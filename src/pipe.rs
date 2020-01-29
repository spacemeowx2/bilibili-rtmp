use rml_rtmp::handshake::{Handshake, PeerType, HandshakeProcessResult};
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use bytes::{Bytes, BytesMut, BufMut};
use rml_rtmp::sessions::{ServerSession, ServerSessionConfig, ServerSessionResult, ServerSessionEvent, ServerSessionError};
use crate::services::{Service, ServiceMap};
use std::collections::HashMap;


#[derive(Debug, Fail)]
pub enum ServerError {
    #[fail(display = "io error: {}", 0)]
    IoError(io::Error),
    #[fail(display = "SocketClosed")]
    SocketClosed,
    #[fail(display = "ServerSessionError: {}", 0)]
    ServerSessionError(ServerSessionError),
    #[fail(display = "State Error: {}", 0)]
    StateError(String),
    #[fail(display = "Client Error: {}", 0)]
    ClientError(String),
}

impl From<io::Error> for ServerError {
    fn from(error: io::Error) -> Self {
        ServerError::IoError(error)
    }
}

impl From<ServerSessionError> for ServerError {
    fn from(error: ServerSessionError) -> Self {
        ServerError::ServerSessionError(error)
    }
}

enum PullState {
    Handshaking(Handshake),
    Connecting,
    Connected {
        session: ServerSession,
    },
    Pulling,
    Closed,
}

struct Connection {}

pub struct Server {
    map: &'static ServiceMap,
    request: HashMap<u32, Option<Client>>,
}

impl Server {
    pub fn new(map: &'static ServiceMap) -> Self {
        Self {
            map,
            request: HashMap::new(),
        }
    }
    pub async fn process(&mut self, socket: &mut TcpStream) -> Result<(), failure::Error> {
        let mut buffer = BytesMut::with_capacity(4096);
        let mut state: PullState = PullState::Handshaking(Handshake::new(PeerType::Server));

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
                new_state = match &mut state {
                    PullState::Handshaking(handshake) => {
                        self.handle_handshake(socket, handshake, &mut buffer).await?
                    },
                    PullState::Connecting => {
                        self.handle_server_session_results(socket).await?
                    },
                    PullState::Connected {
                        session,
                    } => {
                        self.handle_connected(socket, session, &mut buffer).await?
                    },
                    PullState::Closed => break,
                    other => None,
                };
                if let Some(new_state) = new_state {
                    state = new_state;
                }
            }
        }
        Ok(())
    }

    async fn handle_session_result(&mut self, socket: &mut TcpStream, session: &mut ServerSession, session_results: Vec<ServerSessionResult>) -> Result<(), ServerError> {
        let mut next: Vec<ServerSessionResult> = vec![];
        let mut round = session_results;
        while round.len() > 0 {
            for result in round {
                match result {
                    ServerSessionResult::OutboundResponse(packet) => {
                        socket.write_all(&packet.bytes[..]).await?;
                        socket.flush().await?;
                    },
                    ServerSessionResult::RaisedEvent(ServerSessionEvent::ConnectionRequested {
                        request_id,
                        app_name,
                    }) => {
                        println!("ConnectionRequested: {:?} {:?}", request_id, app_name);
                        next.append(&mut session.accept_request(request_id)?);
                    },
                    ServerSessionResult::RaisedEvent(ServerSessionEvent::PublishStreamRequested {
                        request_id,
                        app_name,
                        stream_key,
                        mode,
                    }) => {

                        next.append(&mut session.accept_request(request_id)?);
                    },
                    ServerSessionResult::RaisedEvent(ServerSessionEvent::AudioDataReceived {
                        app_name,
                        stream_key,
                        data,
                        timestamp,
                    }) => {},
                    ServerSessionResult::RaisedEvent(ServerSessionEvent::VideoDataReceived {
                        app_name,
                        stream_key,
                        data,
                        timestamp,
                    }) => {},
                    // ServerSessionResult::RaisedEvent(ServerSessionEvent::StreamMetadataChanged {
                    //     app_name,
                    //     stream_key,
                    //     metadata,
                    // }) => {
                    //     // next.append(&mut session.accept_request(request_id)?);
                    // },
                    x => println!("Server result received: {:?}", x),
                }
            }
            round = next;
            next = vec![];
        }
        Ok(())
    }

    async fn handle_connected(&mut self, socket: &mut TcpStream, session: &mut ServerSession, bytes: &mut BytesMut) -> Result<Option<PullState>, ServerError> {
        let session_results = match session.handle_input(bytes) {
            Ok(results) => results,
            Err(err) => return Err(ServerError::ServerSessionError(err)),
        };
        bytes.clear();
        self.handle_session_result(socket, session, session_results).await?;
        Ok(None)
    }

    async fn handle_server_session_results(&mut self, socket: &mut TcpStream) -> Result<Option<PullState>, ServerError> {
        let config = ServerSessionConfig::new();
        let (mut session, initial_session_results) = ServerSession::new(config)?;

        self.handle_session_result(socket, &mut session, initial_session_results).await?;

        Ok(Some(PullState::Connected {
            session,
        }))
    }

    async fn handle_handshake(&mut self, socket: &mut TcpStream, handshake: &mut Handshake, bytes: &mut BytesMut) -> Result<Option<PullState>, ServerError> {
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

enum ClientState {
    Handshaking(Handshake),
    Connecting,
}

struct Client {
    state: ClientState,
    map: &'static ServiceMap,
}

impl Client {
    fn new(map: &'static ServiceMap) -> Self {
        Self {
            state: ClientState::Handshaking(Handshake::new(PeerType::Client)),
            map,
        }
    }
    async fn connect(&mut self, request_id: u32, app_name: String) -> Result<(), ServerError> {
        match (&self.state, self.map.get(&app_name)) {
            (ClientState::Handshaking(_handshake), Some(clinet)) => {

                Ok(())
            },
            (_, None) => Err(ServerError::ClientError(String::from("service is not found"))),
            (_, _) => Err(ServerError::StateError(String::from("should be Handshaking"))),
        }
    }
    async fn get_url_key(&mut self, service: &Box<dyn Service + Send + Sync>) -> Result<(), ServerError> {
        // service.get_auth
        Ok(())
    }
}
