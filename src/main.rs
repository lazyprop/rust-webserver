use std::collections::HashMap;
use::std::net::{TcpListener, TcpStream};
use std::io::{BufRead, BufReader, Write};
use std::fs;
use std::thread::{self, JoinHandle};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;


struct Worker {
  id: usize,
  handle: JoinHandle<()>,
}

impl Worker {
  fn new(id: usize, rx: Arc<Mutex<mpsc::Receiver<Job>>>) -> Self {
    Worker {
      id: id,
      handle: thread::spawn(move || {
        let rx = rx;
        loop {
          let job = rx.lock().unwrap().recv().unwrap();
          println!("worker {} got job", &id);
          job();
        }
      }),
    }
  }
}

type Job = Box<dyn FnOnce() + Send + 'static>;

struct ThreadPool {
  workers: Vec<Worker>,
  tx: mpsc::Sender<Job>,
  n: usize,
}

impl ThreadPool {
  fn new(n: usize) -> Self {
    let (tx, rx) = mpsc::channel();
    let rx = Arc::new(Mutex::new(rx));
    let mut workers: Vec<Worker> = Vec::with_capacity(n);
    for i in 0..n {
      workers.push(Worker::new(i, Arc::clone(&rx)));
    }
    ThreadPool { workers: workers, tx: tx, n: n }
  }

  fn execute<F>(&mut self, f: F)
    where F: FnOnce() + Send + 'static
  {
    let job = Box::new(f);
    self.tx.send(job).unwrap();
  }
}

#[derive(Debug, Clone)]
enum HttpError {
  BadRequest,
  NotFound,
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

type RouteFn = fn(TcpStream);
type HttpResponse = Result<(), HttpError>;
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

struct HttpServer {
  addr: String,
  routes: HashMap<HttpMethod, Box<RouteFn>>,
  threadpool: ThreadPool,
  logger: HttpLogger,
}

impl HttpServer {
  fn new(addr: &str) -> Self {
    HttpServer {
      addr: addr.to_string(),
      routes: HashMap::new(),
      threadpool: ThreadPool::new(5),
      logger: HttpLogger::new(),
    }
  }

  fn route(&mut self, stream: TcpStream, method: &HttpMethod) -> HttpResponse {
    match self.routes.get(&method) {
      Some(r) => {
        let r = r.clone();
        self.threadpool.execute(move ||  r(stream));
        return Ok(());
      },
      None => Err(HttpError::NotFound),
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
        Ok(m) => (Some(m.clone()), self.route(stream, &m)),
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

  fn add_route(&mut self, m: HttpMethod, f: RouteFn) {
    self.routes.insert(m, Box::new(f));
  }
}


fn constr_response(status: u16, content: String) -> String {
  let status_line = match status {
    200 => "200 OK",
    404 => "404 Not Found",
    400 => "400 Bad Request",
    _ => ""
  };
  let len = content.len();
  format!("HTTP/1.1 {status_line}\r\nContent-length: {len}\r\n\r\n{content}")
}

fn ok_html(stream: &mut TcpStream, filename: &str) {
  let resp = constr_response(
    200,
    fs::read_to_string(filename).unwrap());
  stream.write_all(resp.as_bytes()).unwrap();
}

fn main() {
  use HttpMethod::*;
  let mut app = HttpServer::new("127.0.0.1:8080");


  app.add_route(GET("/".to_string()),
                |mut stream| ok_html(&mut stream, "hello.html"));
  
  app.add_route(GET("/sleep".to_string()),
                |mut stream| {
                  thread::sleep(Duration::from_secs(5));
                  ok_html(&mut stream, "hello.html")});
  app.serve();
}

