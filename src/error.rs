use axum::{
    Json,
    extract::{
        FromRef, FromRequest, FromRequestParts, Path, Query, Request,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
};
use garde::{Unvalidated, Valid, Validate};
use serde::de::DeserializeOwned;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("authentication required")]
    Unauthorized,
    #[error("invalid email or password")]
    InvalidCredentials,
    #[error("{0}")]
    Forbidden(String),
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Conflict(String),
    #[error("content type must be application/json")]
    UnsupportedMediaType,
    #[error("{0}")]
    Unprocessable(String),
    #[error("{0}")]
    TooManyRequests(String),
    #[error("too many attempts. Wait a moment and try again")]
    RateLimited { retry_after: u64 },
    #[error("request body too large")]
    PayloadTooLarge,
    #[error("something went wrong on our side. Try again in a moment")]
    Internal(anyhow::Error),
}

impl From<libsql::Error> for AppError {
    fn from(e: libsql::Error) -> Self {
        Self::Internal(e.into())
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        Self::Internal(e)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            Self::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            Self::Unauthorized | Self::InvalidCredentials => {
                (StatusCode::UNAUTHORIZED, "unauthorized")
            }
            Self::Forbidden(_) => (StatusCode::FORBIDDEN, "forbidden"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            Self::UnsupportedMediaType => {
                (StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported_media_type")
            }
            Self::Unprocessable(_) => (StatusCode::UNPROCESSABLE_ENTITY, "unprocessable"),
            Self::TooManyRequests(_) | Self::RateLimited { .. } => {
                (StatusCode::TOO_MANY_REQUESTS, "too_many_requests")
            }
            Self::PayloadTooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large"),
            Self::Internal(e) => {
                tracing::error!(error = ?e, "internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal")
            }
        };
        let body = json!({ "error": { "code": code, "message": self.to_string() } });
        let mut res = (status, Json(body)).into_response();
        if let Self::RateLimited { retry_after } = self {
            res.headers_mut()
                .insert(axum::http::header::RETRY_AFTER, retry_after.into());
        }
        res
    }
}

impl AppError {
    /// Keeps axum's status; 5xx rejections are internal, so their text is logged, not returned.
    fn rejection(status: StatusCode, text: String) -> Self {
        match status {
            StatusCode::UNSUPPORTED_MEDIA_TYPE => Self::UnsupportedMediaType,
            StatusCode::UNPROCESSABLE_ENTITY => Self::Unprocessable(text),
            StatusCode::PAYLOAD_TOO_LARGE => Self::PayloadTooLarge,
            s if s.is_server_error() => Self::Internal(anyhow::anyhow!(text)),
            _ => Self::BadRequest(text),
        }
    }
}

/// First failed rule; rule messages name their field.
impl From<garde::Report> for AppError {
    fn from(r: garde::Report) -> Self {
        let msg = r
            .iter()
            .next()
            .map_or("invalid input", |(_, e)| e.message());
        Self::Unprocessable(msg.into())
    }
}

impl From<JsonRejection> for AppError {
    fn from(r: JsonRejection) -> Self {
        Self::rejection(r.status(), r.body_text())
    }
}

impl From<QueryRejection> for AppError {
    fn from(r: QueryRejection) -> Self {
        Self::rejection(r.status(), r.body_text())
    }
}

impl From<PathRejection> for AppError {
    fn from(r: PathRejection) -> Self {
        Self::rejection(r.status(), r.body_text())
    }
}

/// JSON body, deserialised (normalisers run here) then checked by garde with the `C` context from state.
/// Rejections use the `AppError` envelope.
pub struct ApiJson<T>(pub Valid<T>);

impl<S, T, C> FromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate<Context = C>,
    C: FromRef<S>,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, AppError> {
        let Json(v) = Json::<T>::from_request(req, state).await?;
        Ok(Self(
            Unvalidated::new(v).validate_with(&C::from_ref(state))?,
        ))
    }
}

/// Query string, as [`ApiJson`].
pub struct ApiQuery<T>(pub Valid<T>);

impl<S, T, C> FromRequestParts<S> for ApiQuery<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate<Context = C>,
    C: FromRef<S>,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, AppError> {
        let Query(v) = Query::<T>::from_request_parts(parts, state).await?;
        Ok(Self(
            Unvalidated::new(v).validate_with(&C::from_ref(state))?,
        ))
    }
}

/// `Path` with rejections in the `AppError` envelope.
pub struct ApiPath<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned + Send> FromRequestParts<S> for ApiPath<T> {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, AppError> {
        Ok(Self(Path::<T>::from_request_parts(parts, state).await?.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn json_shape() {
        let res = AppError::NotFound.into_response();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["error"]["code"], "not_found");
        assert_eq!(v["error"]["message"], "not found");
    }
}
