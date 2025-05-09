use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

// === Request ===
#[derive(Debug)]
pub struct Request {
    pub method: String,
    pub path: Vec<String>,
    pub version: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Request {
    pub fn from_stream(stream: &TcpStream) -> Option<Self> {
        let mut reader = BufReader::new(stream);
        let mut first_line = String::new();
        reader.read_line(&mut first_line).ok()?;

        let mut parts = first_line.trim_end().split_whitespace();
        let method = parts.next()?.to_string();
        let path: Vec<String> = parts.next()?.to_string()
            .split('/')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        let version = parts.next()?.to_string();

        // Read headers
        let mut headers = Vec::new();
        let mut line = String::new();
        while reader.read_line(&mut line).ok()? > 0 {
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                break; // End of headers
            }
            if let Some((key, value)) = trimmed.split_once(": ") {
                headers.push((key.to_string(), value.to_string()));
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
            path: path,
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

        obj.headers.insert("Content-Type".to_string(), "text/plain".to_string());
        obj.headers.insert("Content-Length".to_string(), body.len().to_string());

        return obj;
    }

    pub fn add_header(&mut self, key: String, value: String) {
        self.headers.insert(key, value);
    }

    pub fn send(&self, mut stream: TcpStream) {
        let mut response = format!("HTTP/1.1 {}\r\n", self.status_code);
        for (k, v) in &self.headers {
            response.push_str(&format!("{}: {}\r\n", k, v));
        }
        response.push_str("\r\n");
        response.push_str(&self.body);
        let _ = stream.write_all(response.as_bytes());
    }

}

fn handle_root(_: &Request) -> Response {
    Response::new("200 OK", "")
}

fn handle_not_found() -> Response {
    Response::new("404 Not Found", "")
}

fn handle_echo(req: &Request) -> Response {
    return if req.path.len() < 1 {
        Response::new("400 Bad Request", "")
    } else {
        Response::new("200 OK", req.path.get(1).unwrap())
    }
}

fn dispatch(req: Request) -> Response {
    match req.path.get(0).unwrap_or(&"".to_string()).as_str() {
        "" => handle_root(&req),
        "echo" => handle_echo(&req),
        _ => handle_not_found(),
    }
}

fn handle_connection(stream: TcpStream) {
    if let Some(request) = Request::from_stream(&stream) {
        println!("Request: {:?}", request);
        let response = dispatch(request);
        println!("Response: {:?}", response);
        response.send(stream);
    } else {
        eprintln!("Malformed request");
    }
}

fn main() {
    let listener = TcpListener::bind("127.0.0.1:4221").unwrap();
    println!("Listening on http://127.0.0.1:4221");

    for stream in listener.incoming() {
        if let Ok(stream) = stream {
            handle_connection(stream);
        }
    }
}
