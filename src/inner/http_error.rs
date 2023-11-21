use std::fmt::Display;
use std::io::Cursor;

use log::error;
use rocket::{Request, Response};
use rocket::http::{Header, Status};
use rocket::response::Responder;

pub(crate) struct HttpError<E> {
    error: E,
    status: Status,
}

impl<E> HttpError<E> {
    pub(crate) fn new(error: E) -> Self {
        HttpError {
            error,
            status: Status::InternalServerError,
        }
    }

    pub(crate) fn with_status(mut self, status: Status) -> Self {
        self.status = status;
        self
    }
}

impl<E> From<E> for HttpError<E> where E: Display + std::fmt::Debug {
    fn from(error: E) -> Self {
        HttpError::new(error)
    }
}


impl<'r, 'o: 'r, E> Responder<'r, 'o> for HttpError<E> where E: Display + std::fmt::Debug {
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

pub(crate) type JsonResult<T, E> = Result<rocket::serde::json::Json<T>, HttpError<E>>;
