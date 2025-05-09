use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::thread;

use clap::Parser;

use std::sync::Arc;

// === Request ===
#[derive(Debug)]
pub struct Request {
    pub method: String,
    pub path: Vec<String>,
    pub version: String,
    pub headers: HashMap<String, String>,
    pub body: String,
}

impl Request {
    pub fn from_stream(stream: &TcpStream) -> Option<Self> {
        let mut reader = BufReader::new(stream);
        let mut first_line = String::new();
        reader.read_line(&mut first_line).ok()?;

        let mut parts = first_line.trim_end().split_whitespace();
        let method = parts.next()?.to_string();
        let path: Vec<String> = parts
            .next()?
            .to_string()
            .split('/')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        let version = parts.next()?.to_string();

        // Read headers
        let mut headers = HashMap::new();
        let mut line = String::new();
        while reader.read_line(&mut line).ok()? > 0 {
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                break; // End of headers
            }
            if let Some((key, value)) = trimmed.split_once(": ") {
                headers.insert(key.to_string(), value.to_string());
            }
            line.clear();
        }

        // Read body if Content-Length exists
        let content_length = headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Content-Length"))
            .and_then(|(_, v)| v.parse::<usize>().ok())
            .unwrap_or(0);

        let mut body_buf = vec![0; content_length];
        reader.read_exact(&mut body_buf).ok()?;

        let body = String::from_utf8_lossy(&body_buf).to_string();

        Some(Self {
            method,
            path,
            version,
            headers,
            body,
        })
    }
}

#[derive(Debug)]
pub struct Response {
    pub status_code: String,
    pub headers: HashMap<String, String>,
    pub body: String,
}

impl Response {
    pub fn new(status_code: &str, body: &str) -> Self {
        let mut obj = Self {
            status_code: status_code.to_string(),
            body: body.to_string(),
            headers: HashMap::new(),
        };

        obj.headers
            .insert("Content-Type".to_string(), "text/plain".to_string());
        obj.headers
            .insert("Content-Length".to_string(), body.len().to_string());

        return obj;
    }

    pub fn add_header(&mut self, key: String, value: String) {
        self.headers.insert(key, value);
    }

    pub fn send(&self, mut stream: &TcpStream) {
        let mut response = format!("HTTP/1.1 {}\r\n", self.status_code);
        for (k, v) in &self.headers {
            response.push_str(&format!("{}: {}\r\n", k, v));
        }
        response.push_str("\r\n");
        response.push_str(&self.body);
        let _ = stream.write_all(response.as_bytes());
    }
}

fn read_file_content(path: &str) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}

fn create_file(path: &str, content: &str) -> io::Result<usize> {
    let mut file = File::create(path)?;
    file.write(content.as_bytes())
}

struct HTTPHandler {
    base_path: String,
}

impl HTTPHandler {
    fn new(base_path: String) -> Self {
        Self { base_path }
    }

    fn handle_root(&self, _: &Request) -> Response {
        Response::new("200 OK", "")
    }

    fn handle_not_found(&self) -> Response {
        Response::new("404 Not Found", "")
    }

    fn handle_echo(&self, req: &Request) -> Response {
        return if req.path.len() < 1 {
            Response::new("400 Bad Request", "")
        } else {
            Response::new("200 OK", req.path.get(1).unwrap())
        };
    }

    fn handle_user_agent(&self, req: &Request) -> Response {
        let borrow = &"Unknown".to_string();
        let user_agent = req
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("User-Agent"))
            .map(|(_, v)| v)
            .unwrap_or(&borrow);
        Response::new("200 OK", user_agent)
    }

    fn handle_file(&self, req: &Request) -> Response {
        match req.method.as_str() {
            "POST" => {
                if req.path.len() < 1 {
                    Response::new("400 Bad Request", "")
                } else {
                    let base = PathBuf::from(self.base_path.to_owned());
                    let joined = base.join(req.path.get(1).unwrap());

                    match create_file(joined.to_str().unwrap(), &req.body) {
                        io::Result::Ok(_) => Response::new("201 Created", ""),
                        io::Result::Err(error) => {
                            Response::new("400 Bad Request", &format!("{:?}", error))
                        }
                    }
                }
            }
            "GET" => {
                if req.path.len() < 1 {
                    Response::new("400 Bad Request", "")
                } else {
                    let base = PathBuf::from(self.base_path.to_owned());
                    let joined = base.join(req.path.get(1).unwrap());
                    match read_file_content(joined.to_str().unwrap()) {
                        io::Result::Ok(content) => {
                            let mut resp = Response::new("200 OK", &content);
                            resp.add_header(
                                "Content-Type".to_owned(),
                                "application/octet-stream".to_owned(),
                            );
                            resp
                        }
                        io::Result::Err(error) => {
                            Response::new("400 Bad Request", &format!("{:?}", error))
                        }
                    }
                }
            }
            _ => Response::new("405 Method Not Allowed", ""),
        }
    }
}
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    #[arg(long, default_value = "./")]
    pub dir: String,
}

struct Server {
    handler: HTTPHandler,
}

impl Server {
    fn new(args: Args) -> Self {
        Self {
            handler: HTTPHandler::new(args.dir.clone()),
        }
    }

    fn start_server(self: Arc<Self>) {
        let listener = TcpListener::bind("127.0.0.1:4221").unwrap();
        println!("Listening on http://127.0.0.1:4221");

        for stream in listener.incoming() {
            if let Ok(stream) = stream {
                let server = Arc::clone(&self); // clone the Arc, not the Server itself
                thread::spawn(move || {
                    server.handle_connection(stream);
                });
            }
        }
    }

    fn dispatch(&self, req: Request) -> Response {
        match req.path.get(0).unwrap_or(&"".to_string()).as_str() {
            "" => self.handler.handle_root(&req),
            "echo" => self.handler.handle_echo(&req),
            "user-agent" => self.handler.handle_user_agent(&req),
            "files" => self.handler.handle_file(&req),
            _ => self.handler.handle_not_found(),
        }
    }

    fn handle_connection(&self, stream: TcpStream) {
        println!("New connection from {}", stream.peer_addr().unwrap());
        let writer = stream.try_clone().unwrap();

        loop {
            if let Some(request) = Request::from_stream(&stream) {
                let connection_close = request
                    .headers
                    .get("Connection")
                    .map_or(false, |v| v == "close");

                println!("Request: {:?}", request);
                let response = self.dispatch(request);
                println!("Response: {:?}", response);
                response.send(&writer);

                if connection_close {
                    break;
                }
            } else {
                eprintln!("Malformed request");
                break;
            }
        }
    }
}

fn main() {
    let args = Args::parse();
    println!("Args: {:?}", args);

    let server = Arc::new(Server::new(args));
    server.start_server();
}
