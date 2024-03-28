#![deny(clippy::all)]

use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::response::Response;
use axum::serve::IncomingStream;
use axum::ServiceExt;
use futures::Future;
use matchit::{MatchError, Router};
use napi::tokio::net::TcpListener;
use napi::{
  bindgen_prelude::*,
  threadsafe_function::{ErrorStrategy, ThreadSafeCallContext, ThreadsafeFunction},
  JsFunction, JsObject,
};
use tower::{service_fn, Service};

#[macro_use]
extern crate napi_derive;

#[napi]
pub fn actix() -> ActixApp {
  ActixApp {
    ..Default::default()
  }
}

#[derive(Clone, Default)]
#[napi]
pub struct ActixApp {
  pub hostname: Option<String>,
  pub port: Option<u16>,

  router: Router<ThreadsafeFunction<Request, ErrorStrategy::Fatal>>,

  server_handler: Option<()>,
}

#[napi]
impl ActixApp {
  #[napi]
  pub fn get(&mut self, path: String, callback: JsFunction) -> Result<()> {
    let callback =
      callback.create_threadsafe_function(1, |ctx| req_to_jsreq(ctx).map(|v| vec![v]))?;

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
      async move {
        let router = Arc::clone(&router);
        let tcp_listener = TcpListener::bind((hostname, port)).await?;


        #[derive(Clone)]
        struct ServiceFn<F>(
          Arc<Router<ThreadsafeFunction<Request, ErrorStrategy::Fatal>>>,
          F,
        );

        impl<T, F, Request, R, E> Service<Request> for ServiceFn<T>
        where
          T: FnMut(Arc<Router<ThreadsafeFunction<axum::extract::Request, ErrorStrategy::Fatal>>>, Request) -> F,
          F: Future<Output = std::result::Result<R, E>>,
        {
          type Response = R;
          type Error = E;
          type Future = F;

          fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<std::result::Result<(), E>> {
            Ok(()).into()
          }

          fn call(&mut self, req: Request) -> Self::Future {
            (self.1)(self.0.clone(), req)
          }
        }

        let handler = ServiceFn(router, |router: Arc<Router<ThreadsafeFunction<Request, ErrorStrategy::Fatal>>>, req: Request| async move {
            let val = router.clone();
                    let val = val .at(req.uri().path());

            match val {
              Ok(callback) => {
                let callback = callback.value;
                callback.call(
                  req,
                  napi::threadsafe_function::ThreadsafeFunctionCallMode::NonBlocking,
                );
                Ok::<_, Infallible>(
                  Response::builder()
                    .status(StatusCode::FOUND)
                    .body(Body::empty())
                    .unwrap(),
                )
              },
              Err(MatchError::NotFound) => {
                Ok(
                  Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::empty())
                    .unwrap(),
                )
              },
            }

                });

        axum::serve(tcp_listener, handler.into_make_service()).await?;

        Ok(())
      },
      |&mut env, _| env.get_undefined(),
    )
  }
}

fn req_to_jsreq(ctx: ThreadSafeCallContext<Request>) -> Result<JsObject> {
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

  let body = req.body();

  let jsreq = ctx.env.create_string("Request")?;
  let jsreq = ctx.env.get_global()?.get_property::<_, JsFunction>(jsreq)?;

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

  jsreq.new_instance(&[href.into_unknown(), options.into_unknown()])
}
