use std::sync::{mpsc::{self, Sender, Receiver}, Arc, Mutex};
use std::thread::{self, JoinHandle};

pub type JobFn<T> = Arc<dyn Fn(T) + Send + Sync + 'static>;
pub type Job<T> = (JobFn<T>, T);

struct Worker<T> {
  id: usize,
  handle: JoinHandle<()>,
  rx: Arc<Mutex<Receiver<Job<T>>>>,
}

impl<T> Worker<T> 
where T: Send + 'static
{
  fn new(id: usize, rx: Arc<Mutex<Receiver<Job<T>>>>) -> Self {
    let _rx = Arc::clone(&rx);
    let handle = thread::spawn(move || {
      loop {
        let (f, arg) = _rx.lock().unwrap().recv().unwrap();
        println!("worker {} executing job", id);
        f(arg);
      }
    });
    Worker { id, handle, rx }
  }
}

pub struct ThreadPool<T> {
  workers: Vec<Worker<T>>,
  tx: Sender<Job<T>>,
}

impl<T> ThreadPool<T>
where T: Send + 'static
{
  pub fn new(n: usize) -> Self {
    let (tx, rx) = mpsc::channel();
    let rx = Arc::new(Mutex::new(rx));
    let workers: Vec<Worker<T>>= (0..n)
      .map(|i| Worker::new(i, Arc::clone(&rx))).collect();
    ThreadPool { workers: workers, tx: tx }
  }

  pub fn execute(&self, f: JobFn<T>, v: T) {
    self.tx.send((f, v)).unwrap();
  }

  pub fn shutdown(&mut self) {
  }
}

#[cfg(test)]
fn threadpool_test() {
    // Create a thread pool with 4 workers
    let pool = ThreadPool::<String>::new(4);

    // Define closures directly
    let closure1: JobFn<String> = Arc::new(|value: String| {
        println!("Executing closure 1 with value: {}", value);
        thread::sleep(Duration::from_secs(1));
    });

    let closure2: JobFn<String> = Arc::new(|value: String| {
        println!("Executing closure 2 with value: {}", value);
        thread::sleep(Duration::from_secs(2));
    });

    let closure3: JobFn<String> = Arc::new(|value: String| {
        println!("Executing closure 3 with value: {}", value);
        thread::sleep(Duration::from_secs(3));
    });

    // Execute closures in the thread pool with different values
    pool.execute(Arc::clone(&closure1), String::from("Value 1"));
    pool.execute(Arc::clone(&closure1), String::from("Value 1"));
    pool.execute(Arc::clone(&closure2), String::from("Value 2"));

    // Wait for the closures to finish
    thread::sleep(Duration::from_secs(5));
}
