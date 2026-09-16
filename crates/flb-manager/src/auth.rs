use crate::{ApiError, ApiResult, AppState};
use axum::Json;
use axum::extract::{Request, State};
use axum::http::header::{AUTHORIZATION, COOKIE, SET_COOKIE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

const COOKIE_NAME: &str = "flb_token";
const JWT_TTL_SECS: u64 = 7 * 24 * 3600;

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    iat: u64,
    exp: u64,
}

#[derive(Deserialize)]
pub struct LoginInput {
    username: String,
    password: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginUser {
    username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    token: Option<String>,
}

pub async fn login(
    State(state): State<AppState>,
    Json(input): Json<LoginInput>,
) -> ApiResult<impl IntoResponse> {
    if input.username != state.settings.admin_user
        || input.password != state.settings.admin_password
    {
        return Err(ApiError::unauthorized("用户名或密码错误"));
    }
    let token = sign_jwt(&state.settings.jwt_secret, &state.settings.admin_user)
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let cookie =
        format!("{COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={JWT_TTL_SECS}");
    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        HeaderValue::from_str(&cookie).map_err(|e| ApiError::internal(e.to_string()))?,
    );
    Ok((
        headers,
        Json(LoginUser {
            username: state.settings.admin_user.clone(),
            token: Some(token),
        }),
    ))
}

pub async fn logout() -> impl IntoResponse {
    let cookie = format!("{COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0");
    (StatusCode::NO_CONTENT, [(SET_COOKIE, cookie)])
}

pub async fn me(State(state): State<AppState>) -> Json<LoginUser> {
    Json(LoginUser {
        username: state.settings.admin_user.clone(),
        token: None,
    })
}

pub async fn require_auth(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if let Some(token) = bearer_or_cookie(req.headers())
        && verify_jwt(
            &state.settings.jwt_secret,
            &token,
            &state.settings.admin_user,
        )
        .is_ok()
    {
        return next.run(req).await;
    }
    ApiError::unauthorized("未登录").into_response()
}

fn sign_jwt(secret: &str, username: &str) -> Result<String, jsonwebtoken::errors::Error> {
    let now = jsonwebtoken::get_current_timestamp();
    let claims = Claims {
        sub: username.to_string(),
        iat: now,
        exp: now.saturating_add(JWT_TTL_SECS),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

fn verify_jwt(
    secret: &str,
    token: &str,
    username: &str,
) -> Result<Claims, jsonwebtoken::errors::Error> {
    let mut validation = Validation::default();
    validation.set_required_spec_claims(&["exp", "sub"]);
    validation.sub = Some(username.to_string());
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )?;
    Ok(data.claims)
}

fn bearer_or_cookie(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok())
        && let Some(token) = value.strip_prefix("Bearer ")
    {
        let token = token.trim();
        if !token.is_empty() {
            return Some(token.to_string());
        }
    }
    let cookie = headers.get(COOKIE)?.to_str().ok()?;
    cookie.split(';').find_map(|part| {
        let part = part.trim();
        part.strip_prefix(&format!("{COOKIE_NAME}="))
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jwt_roundtrip() {
        let token = sign_jwt("test-secret", "admin").unwrap();
        let claims = verify_jwt("test-secret", &token, "admin").unwrap();
        assert_eq!(claims.sub, "admin");
    }

    #[test]
    fn jwt_rejects_wrong_user_or_secret() {
        let token = sign_jwt("test-secret", "admin").unwrap();
        assert!(verify_jwt("other-secret", &token, "admin").is_err());
        assert!(verify_jwt("test-secret", &token, "other").is_err());
    }
}
