use super::{executor, Body, Request, Response};

use std::{
  convert::Infallible,
  future::Future,
  io,
  net::{SocketAddr, ToSocketAddrs},
  pin::Pin,
  sync::Arc,
  time::Duration,
};

use hyper::rt::Executor;
use hyper::server::conn::Http;
use tokio::net::TcpListener;

const DATA: &[u8] = b"HTTP/1.1 200 Ok
Content-Length: 12
Content-Type: text/plain; charset=utf-8
\n
Hello World";

/// An HTTP server.
///
/// ```no_run
/// use astra::{Body, Request, Response, Server};
///
/// Server::bind("localhost:3000")
///     .serve(|mut req: Request, _info| {
///         println!("incoming {:?}", req.uri());
///         Response::new(Body::new("Hello World!"))
///     })
///     .expect("failed to start server");
/// ```
///
/// See the [crate-level documentation](crate#how-does-it-work) for details.
pub struct Server {
  addr: SocketAddr,
  http1_keep_alive: Option<bool>,
  http1_half_close: Option<bool>,
  http1_max_buf_size: Option<usize>,
  http1_pipeline_flush: Option<bool>,
  http1_writev: Option<bool>,
  http1_title_case_headers: Option<bool>,
  http1_preserve_header_case: Option<bool>,
  http1_only: Option<bool>,
  worker_keep_alive: Option<Duration>,
  max_workers: Option<usize>,
}

/// HTTP connection information.
#[derive(Clone, Debug)]
pub struct ConnectionInfo {
  peer_addr: Option<SocketAddr>,
}

impl ConnectionInfo {
  /// Returns the socket address of the remote peer of this connection.
  pub fn peer_addr(&self) -> Option<SocketAddr> {
    self.peer_addr
  }
}

/// A service capable of responding to an HTTP request.
///
/// This trait is automatically implemented for functions
/// from a [`Request`] to a [`Response`], but implementing
/// it manually allows for stateful services:
///
/// ```no_run
/// use astra::{Request, Response, Server, Service, Body, ConnectionInfo};
/// use std::sync::Mutex;
///
/// struct MyService {
///     count: Mutex<usize>,
/// }
///
/// impl Service for MyService {
///     fn call(&self, request: Request, _info: ConnectionInfo) -> Response {
///         let mut count = self.count.lock().unwrap();
///         *count += 1;
///         println!("request #{}", *count);
///         Response::new(Body::new("Hello world"))
///     }
/// }
///
///
/// Server::bind("localhost:3000")
///     .serve(MyService { count: Mutex::new(0) })
///     .expect("failed to start server");
/// ```
///
/// If your service is already cheaply cloneable, you can instead use `serve_clone` and avoid an extra `Arc` wrapper:
///
/// ```no_run
/// use astra::{Request, Response, Server, Service, Body, ConnectionInfo};
/// use std::sync::{Arc, Mutex};
///
/// #[derive(Clone)]
/// struct MyService {
///     count: Arc<Mutex<usize>>,
/// }
///
/// impl Service for MyService {
///     fn call(&self, request: Request, _info: ConnectionInfo) -> Response {
///         let mut count = self.count.lock().unwrap();
///         *count += 1;
///         println!("request #{}", *count);
///         Response::new(Body::new("Hello world"))
///     }
/// }
///
/// Server::bind("localhost:3000")
///     .serve_clone(MyService { count: Arc::new(Mutex::new(0)) })
///     .expect("failed to start server");
/// ```
pub trait Service: Send + 'static {
  fn call(&self, request: Request, info: ConnectionInfo) -> Response;
}

impl<F> Service for F
where
  F: Fn(Request, ConnectionInfo) -> Response + Send + 'static,
{
  fn call(&self, request: Request, info: ConnectionInfo) -> Response {
    (self)(request, info)
  }
}

impl<S> Service for Arc<S>
where
  S: Service + Sync,
{
  fn call(&self, request: Request, info: ConnectionInfo) -> Response {
    (**self).call(request, info)
  }
}

impl Server {
  /// Binds a server to the provided address.
  ///
  /// ```no_run
  /// use astra::Server;
  /// use std::net::SocketAddr;
  ///
  /// let server = Server::bind("localhost:3000");
  /// let server = Server::bind(SocketAddr::from(([127, 0, 0, 1], 3000)));
  /// ```
  ///
  /// # Panics
  ///
  /// This method will panic if binding to the address fails.
  pub async fn bind(addr: impl ToSocketAddrs) -> Server {
    let addr = addr.to_socket_addrs().unwrap().next().unwrap();

    Server {
      addr,
      http1_only: None,
      max_workers: None,
      http1_writev: None,
      http1_keep_alive: None,
      http1_half_close: None,
      worker_keep_alive: None,
      http1_max_buf_size: None,
      http1_pipeline_flush: None,
      http1_title_case_headers: None,
      http1_preserve_header_case: None,
    }
  }

  /// Serve incoming connections with the provided service.
  ///
  /// ```no_run
  /// use astra::{Body, Request, Response, Server};
  ///
  /// Server::bind("localhost:3000")
  ///     .serve(|mut req: Request, _| {
  ///         println!("incoming {:?}", req.uri());
  ///         Response::new(Body::new("Hello World!"))
  ///     })
  ///     .expect("failed to start server");
  /// ```
  pub async fn serve<S>(self, service: S) -> io::Result<()>
  where
    S: Service + Sync,
  {
    self.serve_clone(Arc::new(service)).await
  }

  /// Like [`Self::serve`] but does not wrap `service` in an `Arc` and expects it to
  /// implement `Clone` and `Sync` internally.
  pub async fn serve_clone<S>(self, service: S) -> io::Result<()>
  where
    S: Service + Clone,
  {
    // let executor = executor::Executor::new(self.max_workers, self.worker_keep_alive);
    let mut http = Http::new();
    self.configure(&mut http);

    // let reactor = Reactor::new().expect("failed to create reactor");

    let addr = self.addr;
    let server = TcpListener::bind(addr).await?;

    loop {
      let (conn, _) = server.accept().await?;

      let http = http.clone();
      let service = service.clone();
      let info = ConnectionInfo {
        peer_addr: conn.peer_addr().ok(),
      };

      tokio::task::spawn(async move {
        if let Err(err) = http
          .serve_connection(conn, service::HyperService(service, info))
          .await
        {
          eprintln!("Error on connection: {err}");
        };
      });
    }

    #[allow(unreachable_code)]
    Ok(())
  }

  /// Sets whether to use keep-alive for HTTP/1 connections.
  ///
  /// Default is `true`.
  pub fn http1_keep_alive(mut self, val: bool) -> Self {
    self.http1_keep_alive = Some(val);
    self
  }

  /// Set the maximum buffer size.
  ///
  /// Default is ~ 400kb.
  pub fn http1_max_buf_size(mut self, val: usize) -> Self {
    self.http1_max_buf_size = Some(val);
    self
  }

  /// Get the local address of the bound socket
  pub fn local_addr(&self) -> SocketAddr {
    self.addr
  }

  fn configure<T>(&self, http: &mut Http<T>) {
    macro_rules! configure {
            ($self:ident, $other:expr, [$($option:ident),* $(,)?], [$($other_option:ident => $this_option:ident),* $(,)?]) => {{
                $(
                    if let Some(val) = $self.$option {
                        $other.$option(val);
                    }
                )*
                $(
                    if let Some(val) = $self.$this_option {
                        $other.$other_option(val);
                    }
                )*
            }};
        }

    configure!(
        self,
        http,
        [
            http1_keep_alive,
            http1_half_close,
            http1_writev,
            http1_title_case_headers,
            http1_preserve_header_case,
            http1_only,
        ],
        [
            max_buf_size => http1_max_buf_size,
            pipeline_flush => http1_pipeline_flush,
        ]
    );
  }
}

mod service {
  use std::task::Context;

  use super::*;

  type HyperRequest = hyper::Request<hyper::Body>;

  pub struct HyperService<S>(pub S, pub ConnectionInfo);

  impl<S> hyper::service::Service<HyperRequest> for HyperService<S>
  where
    S: Service + Clone,
    // <S as Deref>::Target: Unpin,
  {
    type Response = Response;
    type Error = Infallible;
    type Future = Lazy<S>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
      std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: HyperRequest) -> Self::Future {
      Lazy(self.0.clone(), Some(req), self.1.clone())
    }
  }

  pub struct Lazy<S>(S, Option<HyperRequest>, ConnectionInfo);

  impl<S> Unpin for Lazy<S> {}

  impl<S> Future for Lazy<S>
  where
    S: Service,
  {
    type Output = Result<Response, Infallible>;

    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> std::task::Poll<Self::Output> {
      let (parts, body) = self.1.take().unwrap().into_parts();
      let req = Request::from_parts(parts, Body(body));

      let res = self.0.call(req, self.2.clone());
      std::task::Poll::Ready(Ok(res))
    }
  }
}
