use rml_rtmp::handshake::{Handshake, PeerType, HandshakeProcessResult};
use tokio::net::TcpStream;
use tokio::sync::mpsc::{self, Sender, Receiver};
use tokio::prelude::*;
use bytes::{Bytes, BytesMut, BufMut};
use rml_rtmp::sessions::{ServerSession, ServerSessionConfig, ServerSessionResult, ServerSessionEvent, ServerSessionError};
use rml_rtmp::sessions::{ClientSession, ClientSessionConfig, ClientSessionResult, ClientSessionEvent};
use crate::services::{Service, ServiceMap, BoxService, Authentication};
use std::collections::HashMap;
use url::{Url, ParseError, Position};

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
    #[fail(display = "Parse Error: {}", 0)]
    ParseError(ParseError),
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

impl From<String> for ServerError {
    fn from(error: String) -> Self {
        ServerError::ClientError(error)
    }
}

impl From<ParseError> for ServerError {
    fn from(error: ParseError) -> Self {
        ServerError::ParseError(error)
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
#[derive(Hash, Eq, PartialEq, Debug)]
struct AppKey(String, String);

pub struct Server {
    map: &'static ServiceMap,
    channel: HashMap<AppKey, Client>,
}

impl Server {
    pub fn new(map: &'static ServiceMap) -> Self {
        Self {
            map,
            channel: HashMap::new(),
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
        use ServerSessionEvent::*;
        use ServerSessionResult::*;

        let mut next: Vec<ServerSessionResult> = vec![];
        let mut round = session_results;
        while round.len() > 0 {
            for result in round {
                match result {
                    OutboundResponse(packet) => {
                        socket.write_all(&packet.bytes[..]).await?;
                        socket.flush().await?;
                    },
                    RaisedEvent(ConnectionRequested {
                        request_id,
                        app_name,
                    }) => {
                        println!("ConnectionRequested: {:?} {:?}", request_id, app_name);
                        next.append(&mut session.accept_request(request_id)?);
                    },
                    RaisedEvent(PublishStreamRequested {
                        request_id,
                        app_name,
                        stream_key,
                        mode,
                    }) => {
                        let service = self.map.get(&app_name);
                        if let Some(service) = service {
                            let client = Client::new(service, &stream_key).await?;
                            self.channel.insert(
                                AppKey(app_name, stream_key),
                                client
                            );
                            next.append(&mut session.accept_request(request_id)?);
                        } else {
                            println!("Could not find service for {}", app_name);
                            socket.shutdown().await?;
                        }
                    },
                    RaisedEvent(AudioDataReceived {
                        app_name,
                        stream_key,
                        data,
                        timestamp,
                    }) => {
                        if let Some(fuck) = self.channel.get(&AppKey(app_name, stream_key)) {

                        }
                    },
                    RaisedEvent(VideoDataReceived {
                        app_name,
                        stream_key,
                        data,
                        timestamp,
                    }) => {},
                    RaisedEvent(UnhandleableAmf0Command { .. }) => {},
                    // RaisedEvent(StreamMetadataChanged {
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


#[derive(Debug)]
enum ClientEvent {
    Read(Vec<u8>),
    FromServer,
    End,
}

struct Client {
    sender: Sender<ClientEvent>,
}

impl Client {
    async fn new(service: &'static BoxService, stream_key: &str) -> Result<Self, ServerError> {
        let Authentication { url, key } = service.get_auth(stream_key).await?;
        let url = Url::parse(&url)?;
        let rtmp_endpoint = Self::get_connect(&url)?;
        let app_name = url[Position::BeforePath..][1..].to_owned();

        println!("client connect: {}", rtmp_endpoint);
        let mut stream = TcpStream::connect(&rtmp_endpoint).await?;
        let buffer = Self::do_handshake(&mut stream).await?;
        let (mut read_half, write_half) = tokio::io::split(stream);
        let (sender, receiver) = mpsc::channel::<ClientEvent>(1);
        let mut read_sender = sender.clone();
        tokio::spawn(async move {
            loop {
                let mut buf = vec![0u8; 4096];
                match read_half.read_buf(&mut buf).await {
                    // socket closed
                    Ok(n) if n == 0 => break,
                    Ok(n) => n,
                    Err(e) => {
                        eprintln!("failed to read from socket; err = {:?}", e);
                        break;
                    }
                };
                read_sender.send(ClientEvent::Read(buf)).await.unwrap();
            }
        });
        tokio::spawn(async move {
            Self::process(receiver, write_half, app_name, key).await
        });

        Ok(Self {
            sender
        })
    }
    async fn process<W: AsyncWrite + Unpin>(mut receiver: Receiver<ClientEvent>, mut write: W, app_name: String, key: Option<String>) -> Result<(), failure::Error>
    {
        let mut buffer = BytesMut::with_capacity(4096);
        let (session, session_results) = ClientSession::new(ClientSessionConfig::new())?;

        println!("client process: {}, {:?}", app_name, key);

        loop {
            match receiver.recv().await {
                Some(ClientEvent::Read(buf)) => {
                    buffer.extend_from_slice(&buf);

                    while buffer.len() > 0 {
                        new_state = match &mut state {
                            ClientState::Handshaking(handshake) => {
                                Self::handle_handshake(&mut write, handshake, &mut buffer).await?
                            },
                            _ => None,
                        };
                        if let Some(new_state) = new_state {
                            state = new_state;
                        }
                    }
                },
                _ => break,
            }
        }
        Ok(())
    }
    async fn do_handshake(socket: &mut TcpStream) -> Result<BytesMut, ServerError> {
        let mut buffer = BytesMut::with_capacity(4096);
        let mut handshake = Handshake::new(PeerType::Client);

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
            let result = match handshake.process_bytes(&buffer) {
                Ok(result) => result,
                Err(error) => {
                    println!("Client Handshake error: {:?}", error);
                    return Err(ServerError::SocketClosed);
                }
            };
            buffer.clear();

            match result {
                HandshakeProcessResult::InProgress {response_bytes} => {
                    if response_bytes.len() > 0 {
                        socket.write_all(&response_bytes).await?;
                    }
                },

                HandshakeProcessResult::Completed {response_bytes, remaining_bytes} => {
                    println!("Client Handshake successful!");
                    if response_bytes.len() > 0 {
                        socket.write_all(&response_bytes).await?;
                    }

                    buffer.put(&remaining_bytes[..]);

                    return Ok(buffer)
                }
            }
        }

        Err(String::from("client handshake error").into())
    }
    // async fn handle_handshake<W: AsyncWrite + Unpin>(socket: &mut W, handshake: &mut Handshake, bytes: &mut BytesMut) -> Result<Option<ClientState>, ServerError> {
    //     let result = match handshake.process_bytes(bytes) {
    //         Ok(result) => result,
    //         Err(error) => {
    //             println!("Client Handshake error: {:?}", error);
    //             return Err(ServerError::SocketClosed);
    //         }
    //     };
    //     bytes.clear();

    //     match result {
    //         HandshakeProcessResult::InProgress {response_bytes} => {
    //             if response_bytes.len() > 0 {
    //                 socket.write_all(&response_bytes).await?;
    //             }

    //             Ok(None)
    //         },

    //         HandshakeProcessResult::Completed {response_bytes, remaining_bytes} => {
    //             println!("Client Handshake successful!");
    //             if response_bytes.len() > 0 {
    //                 socket.write_all(&response_bytes).await?;
    //             }

    //             bytes.put(&remaining_bytes[..]);

    //             Ok(Some(ClientState::Connected))
    //         }
    //     }
    // }
    fn get_connect(url: &Url) -> Result<String, ServerError> {
        if url.scheme() != "rtmp" {
            return Err(String::from("protocol error").into())
        }
        let host = match url.host() {
            Some(host) => host,
            None => return Err(String::from("get_auth failed: no host").into())
        };
        let port = url.port().unwrap_or(1935);
        Ok(format!("{}:{}", host, port))
    }
}
