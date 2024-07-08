use futures_util::future;
use reqwest::{
    RequestBuilder,
    Response,
};
use serde::{
    Deserialize,
    Serialize,
};
use skunk_api_protocol::error::ApiError;

use crate::Error;

pub trait RequestBuilderExt: Sized {
    fn msgpack<T: Serialize>(self, body: &T) -> Result<Self, Error>;
}

impl RequestBuilderExt for RequestBuilder {
    fn msgpack<T: Serialize>(self, body: &T) -> Result<Self, Error> {
        let body = rmp_serde::to_vec(body)?;
        Ok(self.body(body))
    }
}

pub trait ResponseExt {
    async fn msgpack<T: for<'de> Deserialize<'de>>(self) -> Result<T, Error>;
}

impl ResponseExt for Response {
    async fn msgpack<T: for<'de> Deserialize<'de>>(self) -> Result<T, Error> {
        let status_code = self.status();
        let body = self.bytes().await?;

        if status_code.is_success() {
            Ok(rmp_serde::from_slice(&body)?)
        }
        else {
            // todo: currently the response can be text/plain as well
            // we should really look at the Content-Type first
            let error: ApiError = rmp_serde::from_slice(&body)?;
            Err(Error::ApiError { status_code, error })
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub mod platform {
    use std::{
        pin::Pin,
        task::{
            Context,
            Poll,
        },
        time::Duration,
    };

    use futures_util::{Future, FutureExt, Stream};

    pub fn spawn_local<F>(future: F)
    where
        F: Future<Output = ()> + 'static,
    {
        wasm_bindgen_futures::spawn_local(future)
    }

    pub fn sleep(time: Duration) -> Sleep {
        Sleep {
            inner: gloo_timers::future::sleep(time),
        }
    }

    #[derive(Debug)]
    pub struct Sleep {
        inner: gloo_timers::future::TimeoutFuture,
    }

    impl Future for Sleep {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            self.inner.poll_unpin(cx)
        }
    }

    pub fn interval(period: Duration) -> Interval {
        let period = u32::try_from(period.as_millis()).unwrap_or(u32::MAX);
        Interval {
            inner: gloo_timers::future::IntervalStream::new(period),
        }
    }

    #[derive(Debug)]
    pub struct Interval {
        inner: gloo_timers::future::IntervalStream,
    }

    impl Interval {
        pub fn poll_tick(&mut self, cx: &mut Context) -> Poll<()> {
            Pin::new(&mut self.inner).poll_next(cx).map(|_|())
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub mod platform {
    use std::{
        pin::Pin,
        task::{
            Context,
            Poll,
        },
        time::Duration,
    };

    use futures_util::Future;

    pub fn spawn_local<F>(future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        tokio::task::spawn(future);
    }

    pub fn sleep(time: Duration) -> Sleep {
        Sleep {
            inner: Box::pin(tokio::time::sleep(time)),
        }
    }

    #[derive(Debug)]
    pub struct Sleep {
        inner: Pin<Box<tokio::time::Sleep>>,
    }

    impl Future for Sleep {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            self.inner.as_mut().poll(cx)
        }
    }

    pub fn interval(period: Duration) -> Interval {
        Interval {
            inner: tokio::time::interval(period),
        }
    }

    #[derive(Debug)]
    pub struct Interval {
        inner: tokio::time::Interval,
    }

    impl Interval {
        pub fn poll_tick(&mut self, cx: &mut Context) -> Poll<()> {
            self.inner.poll_tick(cx).map(|_| ())
        }
    }
}

impl platform::Interval {
    pub async fn tick(&mut self) {
        future::poll_fn(|cx| self.poll_tick(cx)).await
    }
}
