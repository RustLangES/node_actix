#![deny(clippy::all)]

pub mod http;

use std::sync::{Arc, RwLock};

// use astra as http;
use futures::Future;
use http::{Body, ConnectionInfo, Request, ResponseBuilder, Server};
use hyper::service::Service;
use hyper::StatusCode;
use matchit::{MatchError, Router};
use napi::{
  bindgen_prelude::*,
  threadsafe_function::{ErrorStrategy, ThreadSafeCallContext, ThreadsafeFunction},
  JsFunction, JsObject,
};

#[macro_use]
extern crate napi_derive;

#[napi]
pub fn actix() -> ActixApp {
  ActixApp {
    ..Default::default()
  }
}

type MyRequest = Request;
type RouterNode = ThreadsafeFunction<MyRequest, ErrorStrategy::Fatal>;

#[derive(Clone, Default)]
#[napi]
pub struct ActixApp {
  pub hostname: Option<String>,
  pub port: Option<u16>,

  router: Router<RouterNode>,
}

#[napi]
impl ActixApp {
  #[napi]
  pub fn get(&mut self, path: String, callback: JsFunction) -> Result<()> {
    // req_to_jsreq(ctx).map(|v| vec![v])
    let callback = callback.create_threadsafe_function(0, |ctx| {
    req_to_jsreq(ctx).map(|v| vec![v])
      // let obj = ctx.env.create_object()?;
      // obj.set_named_property("url", ctx.env.create_string("some url")?)?;
      // Ok(vec![obj])
    })?;

    self
      .router
      .insert(path, callback)
      .map_err(|err| Error::from_reason(err.to_string()))?;

    Ok(())
  }

  #[napi]
  pub fn listen(
    &mut self,
    env: Env,
    port: u16,
    hostname: Option<Either<String, JsFunction>>,
    callback: Option<JsFunction>,
  ) -> Result<napi::JsObject> {
    let (hostname, callback) = match (hostname, callback) {
      (None, None) => (String::from("127.0.0.1"), None),
      (Some(Either::A(hostname)), None) => (hostname, None),
      (Some(Either::B(callback)), None) => (String::from("127.0.0.1"), Some(callback)),
      (Some(Either::A(hostname)), Some(callback)) => (hostname, Some(callback)),
      _ => unreachable!(),
    };

    self.hostname = Some(hostname.clone());
    self.port = Some(port);

    let router = Arc::new(self.router.clone());

    if let Some(callback) = callback {
      callback.call1::<ActixApp, ()>(self.clone())?;
    }

    env.execute_tokio_future(
      #[allow(unreachable_code)]
      async move {
        let router = Arc::clone(&router);
        // let tcp_listener = TcpListener::bind((hostname, port)).await?;

        let handler = move |router: Arc<Router<RouterNode>>, req: MyRequest| {
          let val = Arc::clone(&router);
          let val = val.at(req.uri().path());

          match val {
            Ok(callback) => {
              let callback = callback.value.clone();

              tokio::spawn(async move {
                let a = callback.call_async::<u16>(req).await.unwrap();
                println!("Callback resuelto: {a}");
              });

              ResponseBuilder::new()
                .status(StatusCode::FOUND)
                .body(Body::empty())
                .unwrap()
            }
            Err(MatchError::NotFound) => ResponseBuilder::new()
              .status(StatusCode::NOT_FOUND)
              .body(Body::empty())
              .unwrap(),
          }
        };

        Server::bind((hostname, port))
          .await
          .serve(move |req: Request, _: ConnectionInfo| handler.clone()(router.clone(), req))
          .await
          .unwrap();

        Ok(())
      },
      |&mut env, _| env.get_undefined(),
    )
  }
}

fn req_to_jsreq(ctx: ThreadSafeCallContext<MyRequest>) -> Result<JsObject> {
  let req = ctx.value;
  let href = String::from("http://localhost:3000/fake");
  // let href = {
  //   let href = req.connection_info().clone();
  //   let scheme = href.scheme();
  //   let host = href.host();
  //   let pathname = req.path();
  //
  //   format!("{scheme}://{host}{pathname}")
  // };
  let method = req.method().as_str().to_owned();
  let headers = req.headers().clone();

  let jsreq = ctx
    .env
    .get_global()?
    .get_named_property::<JsFunction>("Request")?;

  let href = ctx.env.create_string(&href)?;
  let mut options = ctx.env.create_object()?;

  let method = ctx.env.create_string(&method)?;
  options.set_named_property("method", method)?;

  let mut js_headers = ctx.env.create_object()?;

  for (name, value) in headers {
    assert!(name.is_some());
    let name = name.unwrap();
    let name = name.as_str();
    let value = value
      .to_str()
      .map_err(|err| Error::from_reason(err.to_string()))?;
    let value = ctx.env.create_string(value)?;

    js_headers.set_named_property(name, value)?;
  }
  options.set_named_property("headers", js_headers)?;

  // if !body.into_data_stream().is_empty() {
  //   let body = ctx.env.create_arraybuffer_with_data(body.to_vec())?;
  //   options.set_named_property("body", body.into_unknown())?;
  // }
  //

  jsreq.new_instance(&[href.into_unknown(), options.into_unknown()])
}

#[derive(Clone)]
struct ServiceFn<T>(Arc<Router<RouterNode>>, Arc<RwLock<T>>);

impl<T, F, Request, R, E> Service<Request> for ServiceFn<T>
where
  T: FnMut(Arc<Router<RouterNode>>, Request) -> F,
  F: Future<Output = std::result::Result<R, E>>,
{
  type Response = R;
  type Error = E;
  type Future = F;

  fn poll_ready(
    &mut self,
    _: &mut std::task::Context<'_>,
  ) -> std::task::Poll<std::prelude::v1::Result<(), Self::Error>> {
    std::task::Poll::Ready(Ok(()))
  }

  fn call(&mut self, req: Request) -> Self::Future {
    let mut handler = self.1.write().unwrap();
    handler(self.0.clone(), req)
  }
}
