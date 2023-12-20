use std::fmt::Display;
use std::io::Cursor;

use rocket::http::{Header, Status};
use rocket::response::Responder;
use rocket::{Request, Response};
use tracing::error;

use crate::inner::dto::Envelope;
use crate::inner::error::CollectorError;

pub(crate) struct HttpError<E> {
    error: E,
    status: Status,
}

impl<E> HttpError<E> {
    pub(crate) fn with_status(self, status: Status) -> HttpError<E> {
        Self { status, ..self }
    }
}

impl<E> HttpError<E> {
    pub(crate) fn new(error: E) -> Self {
        HttpError {
            error,
            status: Status::InternalServerError,
        }
    }
}

impl<E> From<E> for HttpError<E>
where
    E: Display + std::fmt::Debug,
{
    fn from(error: E) -> Self {
        HttpError::new(error)
    }
}

impl<'r, 'o: 'r, E> Responder<'r, 'o> for HttpError<E>
where
    E: Display + std::fmt::Debug,
{
    fn respond_to(self, _: &'r Request) -> rocket::response::Result<'o> {
        let status_code = self.status.to_string();
        let response_body = format!("{}: {}", status_code, self.error);
        let logged_error = match self.status.code {
            500 => format!("{:?}", self.error),
            _ => format!("{}", self.error),
        };
        error!("Responding with {}: {:?}", status_code, logged_error);
        Response::build()
            .status(self.status)
            .header(Header::new("Content-Type", "text/plain"))
            .sized_body(response_body.len(), Cursor::new(response_body))
            .ok()
    }
}

pub(crate) type WrappedJsonResult<T, E> =
    Result<rocket::serde::json::Json<Envelope<T>>, HttpError<E>>;
pub(crate) type ApiResult<T> = WrappedJsonResult<T, CollectorError>;
