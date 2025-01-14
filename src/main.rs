use std::collections::HashMap;
use::std::net::{TcpListener, TcpStream};
use std::io::{BufRead, BufReader, Write};
use std::fs;
use std::thread;
use std::sync::Arc;
use std::time::Duration;

pub mod threadpool;
use threadpool::ThreadPool;



#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum HttpError {
  BadRequest,
  NotFound,
  InternalServerError,
}

impl HttpError {
  fn to_string(&self) -> String {
    match self {
      Self::BadRequest => "400 Bad Request",
      Self::NotFound => "404 Not Found",
      Self::InternalServerError => "500 Internal Server Error",
    }.to_string()
  }
}

#[derive(PartialEq, Eq, Hash, Debug, Clone)]
enum HttpMethod {
  GET(String),
}

impl HttpMethod {
  fn from_header(req: &Vec<String>) -> Result<HttpMethod, HttpError> {
    if req.len() == 0 { return Err(HttpError::BadRequest); }
    let mut words = req[0].split_whitespace();
    let m = words.next().unwrap();
    let r = words.next().unwrap().to_string();
    match m {
      "GET" => Ok(HttpMethod::GET(r)),
      _ => Err(HttpError::BadRequest),
    }
  }
}

type HttpResponse = Result<String, HttpError>;
type LogEntry = (Option<HttpMethod>, HttpResponse);

struct HttpLogger {
  log: Vec<LogEntry>,
}

impl HttpLogger {
  fn new() -> Self { HttpLogger { log: Vec::new() } }

  fn push(&mut self, method: &Option<HttpMethod>, resp: &HttpResponse) {
    let entry = (method.clone(), resp.clone());
    println!("{:?}", entry);
    self.log.push(entry);
  }
}

type RouteFn = threadpool::JobFn<TcpStream>;
type Job = (TcpStream, RouteFn);

struct HttpServer {
  addr: String,
  routes: HashMap<HttpMethod, RouteFn>,
  error_handlers: HashMap<HttpError, RouteFn>,
  threadpool: ThreadPool<TcpStream>,
  logger: HttpLogger,
}

impl HttpServer {
  fn new(addr: &str) -> Self {
    let mut error_handlers = HashMap::<HttpError, RouteFn>::new();

    error_handlers.insert(
      HttpError::NotFound,
      Arc::new(|mut stream| {
        stream.write_all("404".as_bytes()).unwrap();
      }));
    
    error_handlers.insert(
      HttpError::BadRequest,
      Arc::new(|mut stream| {
        stream.write_all("400".as_bytes()).unwrap();
      }));
    
    error_handlers.insert(
      HttpError::InternalServerError,
      Arc::new(|mut stream| {
        stream.write_all("500".as_bytes()).unwrap();
      }));
    
    HttpServer {
      addr: addr.to_string(),
      routes: HashMap::new(),
      threadpool: ThreadPool::<TcpStream>::new(5),
      logger: HttpLogger::new(),
      error_handlers,
    }
  }

  fn route(&mut self, stream: TcpStream, method: HttpMethod) -> HttpResponse {
    match self.routes.get(&method) {
      Some(f) => {
        self.threadpool.execute(Arc::clone(f), stream);
        Ok("NotImplement: thread not joining".to_string())
      },
      None => {
        let handler = Arc::clone(
          self.error_handlers.get(&HttpError::NotFound).unwrap()
        );
        self.threadpool.execute(handler, stream);
        Err(HttpError::NotFound)
      }
    }
  }

  fn respond(&mut self, stream: TcpStream) {
    let reader = BufReader::new(&stream);
    let req = reader
      .lines()
      .map(|res| res.unwrap())
      .take_while(|line| !line.is_empty())
      .collect::<Vec<_>>();

    let (method, resp): (Option<HttpMethod>, HttpResponse) =
      match HttpMethod::from_header(&req) {
        Ok(m) => (Some(m.clone()), self.route(stream, m.clone())),
        Err(_) => (None, Err(HttpError::BadRequest)),
      };

    self.logger.push(&method, &resp);
  }

  fn serve(&mut self) {
    let listener = TcpListener::bind(&self.addr).unwrap();
    for stream in listener.incoming() {
      self.respond(stream.unwrap());
    }
  }

  fn postprocess_response(resp: HttpResponse) -> String {
    let (status, content) = match resp {
      Ok(s) => ("200 OK".to_string(), s),
      Err(e) => {
        (e.to_string(), e.to_string())
      },
    };
    let len = content.len();
    format!("HTTP/1.1 {status}\r\nContent-length: {len}\r\n\r\n{content}")
  }

  fn add_route(&mut self, m: HttpMethod, f: fn(HttpMethod) -> HttpResponse) {
    self.routes.insert(
      m.clone(),
      Arc::new(move |mut stream: TcpStream| {
        stream.write_all(Self::postprocess_response(f(m.clone())).as_bytes()).unwrap();
      })
    );
  }
}


fn ok_html(filename: &str) -> HttpResponse {
  match fs::read_to_string(filename) {
    Ok(s) => Ok(s),
    Err(_) => Err(HttpError::NotFound),
  }
}

fn main() {
  use HttpMethod::*;
  let mut app = HttpServer::new("127.0.0.1:8080");


  app.add_route(GET("/".to_string()),
                |_| ok_html("hello.html"));
  
  app.add_route(GET("/sleep".to_string()),
                |_| {
                  thread::sleep(Duration::from_secs(5));
                  ok_html("hello.html")});
  app.serve();
}

