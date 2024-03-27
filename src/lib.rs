#![deny(clippy::all)]

use std::sync::Arc;

use actix_web::dev::ServiceRequest;
use actix_web::web::Bytes;
use actix_web::HttpMessage;
use actix_web::{dev::ServerHandle, web, App, HttpRequest, HttpResponse, HttpServer};
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

#[derive(Clone, Default)]
#[napi]
pub struct ActixApp {
  pub hostname: Option<String>,
  pub port: Option<u16>,

  router: Router<ThreadsafeFunction<(HttpRequest, Bytes), ErrorStrategy::Fatal>>,

  server_handler: Option<ServerHandle>,
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

    let server = HttpServer::new(move || {
      let router = Arc::clone(&router);

      App::new().default_service(web::to(move |req: HttpRequest, body: Bytes| {
        let router = Arc::clone(&router);

        async move {
          let val = router.at(req.path());

          match val {
            Ok(callback) => {
              let callback = callback.value;
              callback.call(
                (req, body).clone(),
                napi::threadsafe_function::ThreadsafeFunctionCallMode::NonBlocking,
              );
              HttpResponse::Found()
            }
            Err(MatchError::NotFound) => HttpResponse::NotFound(),
          }
        }
      }))
    })
    .bind((hostname, port))?
    .run();

    if let Some(callback) = callback {
      callback.call1::<ActixApp, ()>(self.clone())?;
    }

    let server_handler = server.handle();
    self.server_handler = Some(server_handler);

    env.execute_tokio_future(
      async {
        server.await?;
        // thread::sleep(Duration::MAX);
        Ok(())
      },
      |&mut env, _| env.get_undefined(),
    )
  }
}

fn req_to_jsreq(ctx: ThreadSafeCallContext<(HttpRequest, Bytes)>) -> Result<JsObject> {
  let req = ctx.value.0.clone();
  let href = {
    let href = req.connection_info().clone();
    let scheme = href.scheme();
    let host = href.host();
    let pathname = req.path();

    format!("{scheme}://{host}{pathname}")
  };
  let method = req.method().as_str().to_owned();
  let headers = req.headers().clone();

  let body = ctx.value.1;

  let jsreq = ctx.env.create_string("Request")?;
  let jsreq = ctx.env.get_global()?.get_property::<_, JsFunction>(jsreq)?;

  let href = ctx.env.create_string(&href)?;
  let mut options = ctx.env.create_object()?;

  let method = ctx.env.create_string(&method)?;
  options.set_named_property("method", method)?;

  let mut js_headers = ctx.env.create_object()?;

  for (name, value) in headers {
    let name = name.as_str();
    let value = value
      .to_str()
      .map_err(|err| Error::from_reason(err.to_string()))?;
    let value = ctx.env.create_string(value)?;

    js_headers.set_named_property(name, value)?;
  }
  options.set_named_property("headers", js_headers)?;

  if !body.is_empty() {
    let body = ctx.env.create_arraybuffer_with_data(body.to_vec())?;
    options.set_named_property("body", body.into_unknown())?;
  }

  jsreq.new_instance(&[href.into_unknown(), options.into_unknown()])
}
